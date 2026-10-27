use crate::cli::{Common, ReadTiming};
use anyhow::Context as _;
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration, Instant};
use tokio_modbus::client;
use tokio_modbus::prelude::*;

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

    fn allows(self, elapsed: Duration, index: u32) -> bool {
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
    let days = (unix_secs / 86_400) as i64;
    let time_of_day = unix_secs % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = time_of_day / 3_600;
    let minute = (time_of_day % 3_600) / 60;
    let second = time_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Howard Hinnant's `civil_from_days`. `days` is the count since 1970-01-01.
fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let day_of_year = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year as i32, month as u32, day as u32)
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

pub async fn connect_to_modbus(common: &Common) -> anyhow::Result<client::Context> {
    match (&common.ip, &common.device) {
        (Some(ip), None) => {
            // TCP connection
            let socket_addr = SocketAddr::new(*ip, common.port);
            if common.verbose {
                println!(
                    "Connecting to Modbus TCP server at {ip}:{} (Modbus address: {})...",
                    common.port, common.unit
                );
            }

            let connect_timeout = Duration::from_secs(common.response_timeout);
            match timeout(connect_timeout, TcpStream::connect(socket_addr)).await {
                Ok(connect_result) => match connect_result {
                    Ok(stream) => {
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
                    Err(e) => {
                        eprintln!("Failed to connect to {ip}:{} - Error: {e}", common.port);
                        Err(e.into())
                    }
                },
                Err(_) => {
                    eprintln!(
                        "Connection to {ip}:{} timed out after {} seconds",
                        common.port, common.response_timeout
                    );
                    Err(anyhow::anyhow!("Connection timeout"))
                }
            }
        }
        (None, Some(device)) => {
            // RTU connection
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
                Ok(serial_result) => match serial_result {
                    Ok(mut serial) => {
                        // Disable exclusive access for virtual ports
                        if let Err(e) = serial.set_exclusive(false) {
                            if common.verbose {
                                println!("Warning: Could not disable exclusive access: {e}");
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
                    Err(e) => {
                        eprintln!("Failed to connect to {} - Error: {e}", device.display());
                        Err(e.into())
                    }
                },
                Err(_) => {
                    eprintln!(
                        "Connection to {} timed out after {} seconds",
                        device.display(),
                        common.response_timeout
                    );
                    Err(anyhow::anyhow!("Connection timeout"))
                }
            }
        }
        (None, None) => Err(anyhow::anyhow!(
            "Must specify either --ip for TCP or --device for RTU"
        )),
        (Some(_), Some(_)) => Err(anyhow::anyhow!("Cannot specify both --ip and --device")),
    }
}

// Generic helper for handling Modbus response errors with timeout
pub async fn handle_modbus_response_with_timeout<T, E>(
    result: Result<Result<Result<T, E>, tokio_modbus::Error>, tokio::time::error::Elapsed>,
    operation: &str,
    timeout_secs: u64,
) -> anyhow::Result<T>
where
    E: std::fmt::Debug,
{
    match result {
        Ok(modbus_result) => match modbus_result {
            Ok(response) => match response {
                Ok(data) => Ok(data),
                Err(exception) => {
                    eprintln!("Modbus exception response: {exception:?}");
                    Err(anyhow::anyhow!("Modbus exception: {:?}", exception))
                }
            },
            Err(e) => {
                eprintln!("Failed to {operation}: {e}");
                Err(e.into())
            }
        },
        Err(_) => {
            eprintln!("Operation '{operation}' timed out after {timeout_secs} seconds");
            Err(anyhow::anyhow!("Operation timeout"))
        }
    }
}

// Helper function to perform Modbus operations with timeout
pub async fn modbus_operation_with_timeout<T, E, F, Fut>(
    operation: F,
    operation_name: &str,
    timeout_secs: u64,
) -> anyhow::Result<T>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<Result<T, E>, tokio_modbus::Error>>,
    E: std::fmt::Debug,
{
    let op_timeout = Duration::from_secs(timeout_secs);
    let result = timeout(op_timeout, operation()).await;
    handle_modbus_response_with_timeout(result, operation_name, timeout_secs).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ReadTiming;
    use tokio_modbus::prelude::ExceptionCode;

    #[test]
    fn test_handle_modbus_response_with_timeout_success() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let success_result: Result<
                Result<Result<Vec<bool>, ExceptionCode>, tokio_modbus::Error>,
                tokio::time::error::Elapsed,
            > = Ok(Ok(Ok([true, false, true].to_vec())));

            let result =
                handle_modbus_response_with_timeout(success_result, "test operation", 5).await;
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), [true, false, true]);
        });
    }

    #[test]
    fn test_handle_modbus_response_with_timeout_exception() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let exception_result: Result<
                Result<Result<Vec<bool>, ExceptionCode>, tokio_modbus::Error>,
                tokio::time::error::Elapsed,
            > = Ok(Ok(Err(ExceptionCode::IllegalDataAddress)));

            let result =
                handle_modbus_response_with_timeout(exception_result, "test operation", 5).await;
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Modbus exception"));
        });
    }

    #[tokio::test]
    async fn test_handle_modbus_response_with_timeout_elapsed() {
        // Simulate a timeout by creating an Elapsed error
        use std::future::Future;
        use std::pin::Pin;
        use std::task::{Context, Poll};

        struct TimeoutFuture;
        impl Future for TimeoutFuture {
            type Output = Result<Result<Vec<bool>, ExceptionCode>, tokio_modbus::Error>;
            fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
                Poll::Pending // Never completes
            }
        }

        let result = tokio::time::timeout(Duration::from_millis(1), TimeoutFuture).await;

        let timeout_result = handle_modbus_response_with_timeout(result, "test operation", 5).await;
        assert!(timeout_result.is_err());
        assert!(timeout_result
            .unwrap_err()
            .to_string()
            .contains("Operation timeout"));
    }

    #[test]
    fn test_handle_modbus_response_with_timeout_modbus_error() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let io_error =
                std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection failed");
            let modbus_error_result: Result<
                Result<Result<Vec<bool>, ExceptionCode>, tokio_modbus::Error>,
                tokio::time::error::Elapsed,
            > = Ok(Err(tokio_modbus::Error::Transport(io_error)));

            let result =
                handle_modbus_response_with_timeout(modbus_error_result, "test operation", 5).await;
            assert!(result.is_err());
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("connection failed"));
        });
    }

    #[test]
    fn read_window_follows_the_timeout_flag() {
        assert_eq!(ReadWindow::from_seconds(None), ReadWindow::Once);
        assert_eq!(
            ReadWindow::from_seconds(Some(10)),
            ReadWindow::For(Duration::from_secs(10))
        );
        assert_eq!(ReadWindow::from_seconds(Some(0)), ReadWindow::Forever);
        assert_eq!(ReadWindow::from_seconds(Some(-3)), ReadWindow::Forever);
        let finite = ReadWindow::For(Duration::from_secs(10));
        assert!(finite.allows(Duration::from_secs(9), 9));
        assert!(!finite.allows(Duration::from_secs(10), 10));
        assert!(ReadWindow::Once.allows(Duration::ZERO, 0));
        assert!(!ReadWindow::Once.allows(Duration::ZERO, 1));
    }

    #[tokio::test]
    async fn omitted_timeout_reads_once() {
        let mut count = 0;
        let timing = ReadTiming {
            timeout: None,
            interval_ms: 1000,
            log: None,
        };
        each_read(&timing, async || {
            count += 1;
            Ok(String::from("sample\n"))
        })
        .await
        .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn a_scan_keeps_an_error_and_counts_valid_replies() {
        let err = present_sample(
            ReadWindow::For(Duration::from_secs(2)),
            Err(anyhow::anyhow!("no reply")),
            2,
            1,
        );
        let frame = err.expect("scan continues");
        assert!(frame.contains("no reply"), "{frame}");
        assert!(frame.contains("requests=2 valid=1"), "{frame}");

        let stopped = present_sample(ReadWindow::Once, Err(anyhow::anyhow!("no reply")), 1, 0);
        assert!(stopped.is_err());
    }

    #[test]
    fn log_record_is_one_line() {
        let line = log_record("2026-10-28T00:00:00Z", "Read 1\n0        7\n");
        assert_eq!(line, "2026-10-28T00:00:00Z\tRead 1 | 0        7");
        assert!(!line.contains('\n'));
    }

    #[test]
    fn utc_stamp_formats_the_epoch() {
        assert_eq!(utc_stamp(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc_stamp(86_400), "1970-01-02T00:00:00Z");
    }

    // Test timeout configuration ranges
    #[test]
    fn test_timeout_duration_creation() {
        let duration1 = Duration::from_secs(1);
        assert_eq!(duration1.as_secs(), 1);

        let duration30 = Duration::from_secs(30);
        assert_eq!(duration30.as_secs(), 30);

        let duration_max = Duration::from_secs(u64::MAX);
        assert_eq!(duration_max.as_secs(), u64::MAX);
    }
}
