use crate::cli::{Common, ReadTiming};
use anyhow::Context as _;
use backon::BackoffBuilder;
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::time::{Duration, Instant, timeout};
use tokio_modbus::client;
use tokio_modbus::prelude::*;

/// Extra attempts after the first try. The command stops when these are used up.
pub const DEFAULT_RETRIES: u32 = 2;

/// Retries for one request. Station 0 stays silent by specification, so a timeout there is not a failed delivery.
pub fn transport_retries(common: &Common) -> u32 {
    if common.unit == 0 { 0 } else { common.retries }
}

/// Default bound for opening a connection and for one Modbus request.
pub const REQUEST_TIMEOUT_SECS: u64 = 5;

/// How many times a read command runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadWindow {
    /// The flag was omitted.
    Once,
    /// Repeat until this duration has elapsed.
    For(Duration),
    /// `0` or a negative `--timeout`. Repeat until interrupted.
    Forever,
}

impl ReadWindow {
    /// `None` reads once. A positive count is that many seconds. Anything else repeats.
    pub fn from_seconds(timeout: Option<i64>) -> Self {
        match timeout {
            None => Self::Once,
            Some(seconds) if seconds > 0 => Self::For(Duration::from_secs(seconds as u64)),
            Some(_) => Self::Forever,
        }
    }

    pub fn allows(self, elapsed: Duration, index: u32) -> bool {
        match self {
            Self::Once => index == 0,
            Self::Forever => true,
            Self::For(limit) => elapsed < limit,
        }
    }

    fn repeating(self) -> bool {
        !matches!(self, Self::Once)
    }
}

/// Turn one sample into the text that is drawn. A repeating scan keeps an error on screen.
pub fn present_sample(
    window: ReadWindow,
    result: Result<String, anyhow::Error>,
    requests: u32,
    valid: u32,
) -> Result<String, anyhow::Error> {
    match result {
        Ok(mut frame) => {
            if window.repeating() {
                push_counts(&mut frame, requests, valid);
            }
            Ok(frame)
        }
        Err(err) if window.repeating() => {
            let mut frame = format!("{err}\n");
            push_counts(&mut frame, requests, valid);
            Ok(frame)
        }
        Err(err) => Err(err),
    }
}

fn push_counts(frame: &mut String, requests: u32, valid: u32) {
    if !frame.ends_with('\n') {
        frame.push('\n');
    }
    let _ = writeln!(frame, "requests={requests} valid={valid}");
}

/// One scan compressed onto a single log line.
pub fn log_record(stamp: &str, frame: &str) -> String {
    let body = frame.lines().collect::<Vec<_>>().join(" | ");
    format!("{stamp}\t{body}")
}

/// `YYYY-MM-DDTHH:MM:SSZ` from seconds since 1970-01-01 UTC.
pub fn utc_stamp(unix_secs: u64) -> String {
    let seconds = i64::try_from(unix_secs).unwrap_or(i64::MAX);
    let stamp = jiff::Timestamp::from_second(seconds).unwrap_or(jiff::Timestamp::UNIX_EPOCH);
    stamp.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn now_stamp() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    utc_stamp(seconds)
}

/// Run `read_once` once, for `--timeout` seconds, or until interrupted.
///
/// Samples after the first wait until `--interval` has passed since the previous
/// sample started. On a terminal, each later sample replaces the previous one.
/// A failed sample during a repeating scan is drawn and the scan continues.
/// `--log` appends one line per sample, without the request counters.
pub async fn each_read(
    timing: &ReadTiming,
    mut read_once: impl AsyncFnMut() -> anyhow::Result<String>,
) -> anyhow::Result<()> {
    let window = ReadWindow::from_seconds(timing.timeout);
    let interval = Duration::from_millis(timing.interval_ms);
    let mut log = match &timing.log {
        Some(path) => Some(
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .with_context(|| format!("opening {}", path.display()))?,
        ),
        None => None,
    };
    let started = Instant::now();
    let mut index = 0u32;
    let mut requests = 0u32;
    let mut valid = 0u32;
    let mut drawn = 0usize;
    loop {
        if !window.allows(started.elapsed(), index) {
            return Ok(());
        }
        let tick = Instant::now();
        let result = read_once().await;
        requests += 1;
        if result.is_ok() {
            valid += 1;
        }
        if let Some(file) = log.as_mut() {
            if let Ok(frame) = result.as_ref() {
                writeln!(file, "{}", log_record(&now_stamp(), frame))?;
            } else if let Err(err) = result.as_ref() {
                writeln!(file, "{}", log_record(&now_stamp(), &format!("{err}\n")))?;
            }
        }
        let frame = present_sample(window, result, requests, valid)?;
        drawn = crate::output::paint(drawn, &frame)?;
        index += 1;
        if matches!(window, ReadWindow::Once) {
            return Ok(());
        }
        let mut gap = interval.saturating_sub(tick.elapsed());
        if let ReadWindow::For(limit) = window {
            let remaining = limit.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return Ok(());
            }
            gap = gap.min(remaining);
        }
        if !gap.is_zero() {
            tokio::time::sleep(gap).await;
        }
    }
}

/// One failed try. Transport failures can be sent again. A Modbus exception is the device's answer.
pub struct AttemptError {
    message: String,
    retryable: bool,
}

impl AttemptError {
    pub fn retry(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: true,
        }
    }

    pub fn stop(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            retryable: false,
        }
    }
}

/// Delays between extra attempts. `retries` is how many times a failure is sent again.
fn retry_delays(retries: u32) -> impl Iterator<Item = Duration> {
    backon::ConstantBuilder::new()
        .with_delay(Duration::ZERO)
        .with_max_times(usize::try_from(retries).unwrap_or(usize::MAX))
        .build()
}

/// Run `attempt` once, then up to `retries` more times after a transport failure.
pub async fn with_retries<T>(
    retries: u32,
    mut attempt: impl AsyncFnMut() -> Result<T, AttemptError>,
) -> anyhow::Result<T> {
    let mut delays = retry_delays(retries);
    loop {
        match attempt().await {
            Ok(value) => return Ok(value),
            Err(err) if err.retryable => match delays.next() {
                Some(delay) if !delay.is_zero() => tokio::time::sleep(delay).await,
                Some(_) => {}
                None => return Err(anyhow::anyhow!(err.message)),
            },
            Err(err) => return Err(anyhow::anyhow!(err.message)),
        }
    }
}

pub async fn connect_to_modbus(common: &Common) -> anyhow::Result<client::Context> {
    match (&common.ip, &common.device) {
        (None, None) => {
            anyhow::bail!("Must specify either --ip for TCP or --device for RTU")
        }
        (Some(_), Some(_)) => anyhow::bail!("Cannot specify both --ip and --device"),
        (Some(ip), None) => {
            if common.verbose {
                println!(
                    "Connecting to Modbus TCP server at {ip}:{} (Modbus address: {})...",
                    common.port, common.unit
                );
            }
            let ip = *ip;
            with_retries(common.retries, async || connect_tcp(common, ip).await).await
        }
        (None, Some(device)) => {
            if common.verbose {
                println!(
                    "Connecting to Modbus RTU device at {} (Baud: {}, Parity: {:?}, Stop Bits: {:?}, Data Bits: {:?}, Modbus address: {})...",
                    device.display(),
                    common.baud,
                    common.parity,
                    common.stop_bits,
                    common.data_bits,
                    common.unit
                );
            }
            let device = device.clone();
            with_retries(common.retries, async || connect_rtu(common, &device).await).await
        }
    }
}

async fn connect_tcp(
    common: &Common,
    ip: std::net::IpAddr,
) -> Result<client::Context, AttemptError> {
    let socket_addr = SocketAddr::new(ip, common.port);
    let connect_timeout = Duration::from_secs(common.response_timeout);
    match timeout(connect_timeout, TcpStream::connect(socket_addr)).await {
        Ok(Ok(stream)) => {
            let stream = crate::trace::Trace::new(stream, common.verbose);
            let ctx = client::tcp::attach_slave(stream, Slave(common.unit));
            if common.verbose {
                println!(
                    "Successfully connected to Modbus TCP server at {ip}:{}",
                    common.port
                );
            }
            Ok(ctx)
        }
        Ok(Err(err)) => Err(AttemptError::retry(format!(
            "Failed to connect to {ip}:{}: {err}",
            common.port
        ))),
        Err(_) => Err(AttemptError::retry(format!(
            "Connection to {ip}:{} timed out after {} seconds",
            common.port, common.response_timeout
        ))),
    }
}

async fn connect_rtu(
    common: &Common,
    device: &std::path::Path,
) -> Result<client::Context, AttemptError> {
    let connect_timeout = Duration::from_secs(common.response_timeout);
    match timeout(connect_timeout, async {
        let builder = tokio_serial::new(device.to_string_lossy(), common.baud)
            .parity(common.parity.into())
            .stop_bits(common.stop_bits.into())
            .data_bits(common.data_bits.into());
        tokio_serial::SerialStream::open(&builder)
    })
    .await
    {
        Ok(Ok(mut serial)) => {
            if let Err(err) = serial.set_exclusive(false) {
                if common.verbose {
                    println!("Warning: Could not disable exclusive access: {err}");
                }
            }
            let serial = crate::trace::Trace::new(serial, common.verbose);
            let ctx = client::rtu::attach_slave(serial, Slave(common.unit));
            if common.verbose {
                println!(
                    "Successfully connected to Modbus RTU device at {}",
                    device.display()
                );
            }
            Ok(ctx)
        }
        Ok(Err(err)) => Err(AttemptError::retry(format!(
            "Failed to connect to {}: {err}",
            device.display()
        ))),
        Err(_) => Err(AttemptError::retry(format!(
            "Connection to {} timed out after {} seconds",
            device.display(),
            common.response_timeout
        ))),
    }
}

fn classify_modbus<T, E>(
    result: Result<Result<Result<T, E>, tokio_modbus::Error>, tokio::time::error::Elapsed>,
    operation: &str,
    timeout_secs: u64,
) -> Result<T, AttemptError>
where
    E: std::fmt::Debug,
{
    match result {
        Ok(Ok(Ok(data))) => Ok(data),
        Ok(Ok(Err(exception))) => Err(AttemptError::stop(format!(
            "Modbus exception response: {exception:?}"
        ))),
        Ok(Err(err)) => Err(AttemptError::retry(format!("Failed to {operation}: {err}"))),
        Err(_) => Err(AttemptError::retry(format!(
            "Operation '{operation}' timed out after {timeout_secs} seconds"
        ))),
    }
}

/// Turn one framed result into the message printed for that outcome.
#[cfg(test)]
pub async fn handle_modbus_response_with_timeout<T, E>(
    result: Result<Result<Result<T, E>, tokio_modbus::Error>, tokio::time::error::Elapsed>,
    operation: &str,
    timeout_secs: u64,
) -> anyhow::Result<T>
where
    E: std::fmt::Debug,
{
    classify_modbus(result, operation, timeout_secs).map_err(|err| anyhow::anyhow!(err.message))
}

/// Send one request. A timeout or a broken connection is tried `retries` more times.
pub async fn modbus_operation_with_timeout<T, E>(
    mut operation: impl AsyncFnMut() -> Result<Result<T, E>, tokio_modbus::Error>,
    operation_name: &str,
    timeout_secs: u64,
    retries: u32,
) -> anyhow::Result<T>
where
    E: std::fmt::Debug,
{
    let op_timeout = Duration::from_secs(timeout_secs);
    with_retries(retries, async || {
        let result = timeout(op_timeout, operation()).await;
        classify_modbus(result, operation_name, timeout_secs)
    })
    .await
}
