use crate::serial::{DataBits, Parity, StopBits};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::{Instant, timeout};
use tokio_modbus::prelude::*;
use tokio_modbus::server::{Service, rtu, rtu_over_tcp, tcp};

/// Runtime options shared by the TCP and RTU servers.
#[derive(Clone)]
pub struct ServerRuntime {
    pub units: Vec<u8>,
    pub verbose: bool,
    pub timeout_secs: u64,
    pub delay: Duration,
    /// Function code that returns `exception` and does not change memory.
    pub forced: Option<(u8, ExceptionCode)>,
    pub silence_writes: bool,
}

/// TCP, RTU-over-TCP, or serial RTU listener.
#[derive(Debug, PartialEq, Eq)]
pub enum ServerListen {
    Tcp(IpAddr),
    RtuOverTcp(IpAddr),
    Rtu(PathBuf),
}

/// Select the listener. No `--device` means TCP, or RTU over TCP when asked.
/// The default TCP address is `0.0.0.0`.
pub fn server_listen(
    ip: Option<IpAddr>,
    device: Option<PathBuf>,
    rtu_over_tcp: bool,
) -> anyhow::Result<ServerListen> {
    if device.is_some() && (ip.is_some() || rtu_over_tcp) {
        return Err(anyhow::anyhow!(
            "Cannot combine --device with --ip or --rtu-over-tcp"
        ));
    }
    if let Some(device) = device {
        return Ok(ServerListen::Rtu(device));
    }
    let ip = ip.unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    if rtu_over_tcp {
        Ok(ServerListen::RtuOverTcp(ip))
    } else {
        Ok(ServerListen::Tcp(ip))
    }
}

pub(crate) fn log_line(verbose: bool, peer: Option<&str>, message: &str) {
    if !verbose {
        return;
    }
    match peer {
        Some(peer) => println!("{message} from {peer}"),
        None => println!("{message}"),
    }
}

fn idle_sleep(timeout_secs: Duration) -> Option<Pin<Box<tokio::time::Sleep>>> {
    let deadline = Instant::now().checked_add(timeout_secs)?;
    Some(Box::pin(tokio::time::sleep_until(deadline)))
}

/// Closes a quiet TCP connection after `timeout` with a clean EOF.
pub struct IdleTimeoutStream<S> {
    inner: S,
    timeout: Duration,
    idle: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl<S> IdleTimeoutStream<S> {
    pub fn new(inner: S, timeout: Duration) -> Self {
        let idle = idle_sleep(timeout);
        Self {
            inner,
            timeout,
            idle,
        }
    }

    fn reset_idle(&mut self) {
        self.idle = idle_sleep(self.timeout);
    }
}

impl<S> AsyncRead for IdleTimeoutStream<S>
where
    S: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if let Some(idle) = this.idle.as_mut() {
            if idle.as_mut().poll(cx).is_ready() {
                return Poll::Ready(Ok(()));
            }
        }

        let filled = buf.filled().len();
        match Pin::new(&mut this.inner).poll_read(cx, buf) {
            Poll::Ready(Ok(())) if buf.filled().len() > filled => {
                this.reset_idle();
                Poll::Ready(Ok(()))
            }
            other => other,
        }
    }
}

impl<S> AsyncWrite for IdleTimeoutStream<S>
where
    S: AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_write(cx, buf) {
            Poll::Ready(Ok(n)) if n > 0 => {
                this.reset_idle();
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

/// Turns two requests the TCP codec would drop into requests it can decode.
///
/// tokio-modbus rejects every MEI type except 14, and it rejects a function 05
/// value other than `0x0000` or `0xFF00`. Either error closes the connection.
/// The replacement keeps the MBAP transaction id. `unsupported_mei` asks for
/// exception 01. `illegal_coil` asks for exception 03, and the coil is not written.
struct MeiRewrite<S> {
    inner: S,
    incoming: Vec<u8>,
    pending: VecDeque<u8>,
    unsupported_mei: Arc<AtomicBool>,
    illegal_coil: Arc<AtomicBool>,
}

impl<S> MeiRewrite<S> {
    fn new(inner: S, unsupported_mei: Arc<AtomicBool>, illegal_coil: Arc<AtomicBool>) -> Self {
        Self {
            inner,
            incoming: Vec::new(),
            pending: VecDeque::new(),
            unsupported_mei,
            illegal_coil,
        }
    }
}

/// One MBAP frame, or `None` while the header or the declared body is incomplete.
pub fn pop_mbap_frame(buf: &mut Vec<u8>) -> Option<Vec<u8>> {
    if buf.len() < 7 {
        return None;
    }
    let length = usize::from(u16::from_be_bytes([buf[4], buf[5]]));
    // Unit id plus at most 253 PDU bytes. A larger length would stall the reader.
    if length == 0 || length > 254 {
        return Some(vec![buf.remove(0)]);
    }
    let total = 6 + length;
    if buf.len() < total {
        return None;
    }
    Some(buf.drain(..total).collect())
}

/// Pass a normal frame through.
///
/// An MEI type other than 14 becomes a specific device-id read of object 0.
/// A function 05 value other than `0x0000` or `0xFF00` becomes OFF, so the codec
/// can deliver the request. The flags tell the service which exception to send.
pub fn adjust_mei_frame(
    frame: &[u8],
    unsupported_mei: &AtomicBool,
    illegal_coil: &AtomicBool,
) -> Vec<u8> {
    let length = usize::from(u16::from_be_bytes([frame[4], frame[5]]));
    let pdu = frame.get(7..6 + length).unwrap_or(&[]);
    if let Some(rewritten) = rewrite_illegal_coil(frame, pdu) {
        illegal_coil.store(true, Ordering::SeqCst);
        return rewritten;
    }
    let supported = pdu.first() == Some(&0x2B) && pdu.get(1) == Some(&0x0E) && pdu.len() >= 4;
    let other_mei = pdu.first() == Some(&0x2B) && !supported;
    if !other_mei {
        return frame.to_vec();
    }
    unsupported_mei.store(true, Ordering::SeqCst);
    let mut out = frame[..6].to_vec();
    let body_len = 1 + 4;
    out[4] = (body_len as u16 >> 8) as u8;
    out[5] = body_len as u8;
    out.push(frame[6]);
    out.extend_from_slice(&[0x2B, 0x0E, 0x04, 0x00]);
    out
}

/// Function 05 with a value other than OFF or ON. The address is kept.
fn rewrite_illegal_coil(frame: &[u8], pdu: &[u8]) -> Option<Vec<u8>> {
    if pdu.len() != 5 || pdu[0] != 0x05 {
        return None;
    }
    let value = u16::from_be_bytes([pdu[3], pdu[4]]);
    if value == 0x0000 || value == 0xFF00 {
        return None;
    }
    let mut out = frame.to_vec();
    let value_at = frame.len().checked_sub(2)?;
    out[value_at] = 0;
    out[value_at + 1] = 0;
    Some(out)
}

impl<S> AsyncRead for MeiRewrite<S>
where
    S: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        loop {
            if !this.pending.is_empty() {
                let n = this.pending.len().min(buf.remaining());
                let bytes: Vec<u8> = this.pending.drain(..n).collect();
                buf.put_slice(&bytes);
                return Poll::Ready(Ok(()));
            }
            let mut tmp = [0u8; 512];
            let mut read = tokio::io::ReadBuf::new(&mut tmp);
            match Pin::new(&mut this.inner).poll_read(cx, &mut read) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(err)) => return Poll::Ready(Err(err)),
                Poll::Ready(Ok(())) => {
                    let got = read.filled();
                    if got.is_empty() {
                        return Poll::Ready(Ok(()));
                    }
                    this.incoming.extend_from_slice(got);
                }
            }
            while let Some(frame) = pop_mbap_frame(&mut this.incoming) {
                this.pending.extend(adjust_mei_frame(
                    &frame,
                    &this.unsupported_mei,
                    &this.illegal_coil,
                ));
            }
        }
    }
}

impl<S> AsyncWrite for MeiRewrite<S>
where
    S: AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

/// Counters and flags for function 08/11/12.
#[derive(Debug, Clone)]
pub struct Diagnostics {
    pub register: u16,
    pub delimiter: u8,
    pub listen_only: bool,
    pub event_count: u16,
    pub message_count: u16,
    pub comm_error_count: u16,
    pub exception_count: u16,
    pub server_message_count: u16,
    pub no_response_count: u16,
    pub nak_count: u16,
    pub busy_count: u16,
    pub overrun_count: u16,
    pub events: VecDeque<u8>,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            register: 0,
            delimiter: 0x0A,
            listen_only: false,
            event_count: 0,
            message_count: 0,
            comm_error_count: 0,
            exception_count: 0,
            server_message_count: 0,
            no_response_count: 0,
            nak_count: 0,
            busy_count: 0,
            overrun_count: 0,
            events: VecDeque::new(),
        }
    }
}

#[derive(Debug)]
pub struct ModbusData {
    pub coils: Vec<bool>,
    pub discrete_inputs: Vec<bool>,
    pub holding_registers: Vec<u16>,
    pub input_registers: Vec<u16>,
    /// File records addressed by `(file number, record number)`.
    pub files: HashMap<(u16, u16), u16>,
    pub diagnostics: Diagnostics,
}

impl ModbusData {
    /// Draw a new value for every coil, holding register, and input register.
    /// Discrete inputs stay OFF. The draw changes on each call.
    pub fn new(num_coils: u16, num_discrete: u16, num_holding: u16, num_input: u16) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos() as u64)
            .unwrap_or(1);
        let seed = nanos ^ (u64::from(std::process::id()) << 32);
        Self::from_seed(num_coils, num_discrete, num_holding, num_input, seed)
    }

    /// Same layout as [`Self::new`], from a fixed seed. Used by tests and startup.
    pub fn from_seed(
        num_coils: u16,
        num_discrete: u16,
        num_holding: u16,
        num_input: u16,
        seed: u64,
    ) -> Self {
        let mut rng = SplitMix::new(seed);
        Self {
            coils: (0..num_coils).map(|_| rng.next_u64() & 1 == 1).collect(),
            discrete_inputs: vec![false; usize::from(num_discrete)],
            holding_registers: (0..num_holding).map(|_| rng.next_u64() as u16).collect(),
            input_registers: (0..num_input).map(|_| rng.next_u64() as u16).collect(),
            files: HashMap::new(),
            diagnostics: Diagnostics::default(),
        }
    }
}

/// SplitMix64. Every `u16` it produces is an integer, which is a rational number.
struct SplitMix {
    state: u64,
}

impl SplitMix {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut mixed = self.state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }
}

#[derive(Clone)]
pub struct ModbusService {
    data: Arc<tokio::sync::RwLock<ModbusData>>,
    units: Vec<u8>,
    verbose: bool,
    delay: Duration,
    forced: Option<(u8, ExceptionCode)>,
    silence_writes: bool,
    peer: Option<String>,
    /// Set when a TCP request used an MEI type other than 14. The codec cannot
    /// represent that request, so the bytes were replaced with a device-id read.
    unsupported_mei: Arc<AtomicBool>,
    /// Set when function 05 carried a value other than `0x0000` or `0xFF00`.
    /// The codec cannot represent that value, so the bytes were replaced with OFF.
    illegal_coil: Arc<AtomicBool>,
}

impl ModbusService {
    pub fn new(data: Arc<tokio::sync::RwLock<ModbusData>>, runtime: &ServerRuntime) -> Self {
        Self {
            data,
            units: runtime.units.clone(),
            verbose: runtime.verbose,
            delay: runtime.delay,
            forced: runtime.forced,
            silence_writes: runtime.silence_writes,
            peer: None,
            unsupported_mei: Arc::new(AtomicBool::new(false)),
            illegal_coil: Arc::new(AtomicBool::new(false)),
        }
    }

    fn with_peer(mut self, peer: String) -> Self {
        self.peer = Some(peer);
        self
    }

    fn with_unsupported_mei(mut self, flag: Arc<AtomicBool>) -> Self {
        self.unsupported_mei = flag;
        self
    }

    fn with_illegal_coil(mut self, flag: Arc<AtomicBool>) -> Self {
        self.illegal_coil = flag;
        self
    }
}

impl Service for ModbusService {
    type Request = SlaveRequest<'static>;
    type Response = Option<tokio_modbus::Response>;
    type Exception = ExceptionCode;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Exception>> + Send>,
    >;

    fn call(&self, req: Self::Request) -> Self::Future {
        let data = self.data.clone();
        let units = self.units.clone();
        let verbose = self.verbose;
        let delay = self.delay;
        let forced = self.forced;
        let silence_writes = self.silence_writes;
        let peer = self.peer.clone();
        let unsupported_mei = self.unsupported_mei.clone();
        let illegal_coil = self.illegal_coil.clone();
        Box::pin(async move {
            let SlaveRequest { slave, request } = req;
            let station = slave;
            // The TCP codec rejects these frames before this runs. The bytes were
            // rewritten into a request it accepts, and the flag says the original
            // request was not that substitute. Answer the specification exception
            // and keep the connection open.
            let rewritten = matches!(request, Request::ReadDeviceIdentification(_, _))
                && unsupported_mei.swap(false, Ordering::SeqCst);
            let rejected_coil = matches!(request, Request::WriteSingleCoil(_, _))
                && illegal_coil.swap(false, Ordering::SeqCst);
            let outcome = if rewritten || rejected_coil {
                let exception = if rewritten {
                    ExceptionCode::IllegalFunction
                } else {
                    ExceptionCode::IllegalDataValue
                };
                if station != 0 && units.contains(&station) {
                    pause(delay).await;
                    Err(exception)
                } else {
                    Ok(None)
                }
            } else {
                answer(
                    &data,
                    &units,
                    station,
                    request.clone(),
                    delay,
                    forced,
                    silence_writes,
                )
                .await
            };
            log_line(
                verbose,
                peer.as_deref(),
                &crate::handle::summarize(station, &request, &outcome),
            );
            outcome
        })
    }
}

async fn answer(
    data: &Arc<tokio::sync::RwLock<ModbusData>>,
    units: &[u8],
    station: u8,
    request: Request<'static>,
    delay: Duration,
    forced: Option<(u8, ExceptionCode)>,
    silence_writes: bool,
) -> Result<Option<Response>, ExceptionCode> {
    if station != 0 && !units.contains(&station) {
        return Ok(None);
    }

    let function = request.function_code().value();
    let write = crate::handle::is_write(&request);
    let forced_hit = forced.is_some_and(|(code, _)| code == function);

    // Station 0 is broadcast: writes may update memory, and nothing is sent back.
    if station == 0 {
        if !write || forced_hit || silence_writes {
            return Ok(None);
        }
        let mut guard = data.write().await;
        let _ = crate::handle::handle_request(&mut guard, request, station);
        return Ok(None);
    }

    if let Some((_, exception)) = forced.filter(|(code, _)| *code == function) {
        pause(delay).await;
        return Err(exception);
    }
    if silence_writes && write {
        return Ok(None);
    }

    let mut guard = data.write().await;
    let outcome = crate::handle::handle_request(&mut guard, request, station);
    drop(guard);
    if sends_response(&outcome) {
        pause(delay).await;
    }
    outcome
}

fn sends_response(outcome: &Result<Option<Response>, ExceptionCode>) -> bool {
    matches!(outcome, Ok(Some(_)) | Err(_))
}

async fn pause(delay: Duration) {
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
}

pub async fn run_tcp_server(
    ip_addr: IpAddr,
    port: u16,
    runtime: ServerRuntime,
    data: Arc<tokio::sync::RwLock<ModbusData>>,
) -> anyhow::Result<()> {
    let socket_addr = SocketAddr::new(ip_addr, port);
    let listener = match timeout(
        Duration::from_secs(runtime.timeout_secs),
        tokio::net::TcpListener::bind(socket_addr),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            eprintln!(
                "Binding {ip_addr}:{port} timed out after {} seconds",
                runtime.timeout_secs
            );
            return Err(anyhow::anyhow!("Connection timeout"));
        }
    };
    println!("Modbus TCP server listening on {ip_addr}:{port}");
    println!("Press Ctrl+C to stop the server");
    serve_tcp_connections(tcp::Server::new(listener), runtime, data).await
}

pub async fn run_rtu_over_tcp_server(
    ip_addr: IpAddr,
    port: u16,
    runtime: ServerRuntime,
    data: Arc<tokio::sync::RwLock<ModbusData>>,
) -> anyhow::Result<()> {
    let socket_addr = SocketAddr::new(ip_addr, port);
    let listener = match timeout(
        Duration::from_secs(runtime.timeout_secs),
        tokio::net::TcpListener::bind(socket_addr),
    )
    .await
    {
        Ok(result) => result?,
        Err(_) => {
            eprintln!(
                "Binding {ip_addr}:{port} timed out after {} seconds",
                runtime.timeout_secs
            );
            return Err(anyhow::anyhow!("Connection timeout"));
        }
    };
    println!("Modbus RTU over TCP server listening on {ip_addr}:{port}");
    println!("Press Ctrl+C to stop the server");
    serve_rtu_over_tcp_connections(rtu_over_tcp::Server::new(listener), runtime, data).await
}

async fn serve_tcp_connections(
    server: tcp::Server,
    runtime: ServerRuntime,
    data: Arc<tokio::sync::RwLock<ModbusData>>,
) -> anyhow::Result<()> {
    let idle = Duration::from_secs(runtime.timeout_secs);
    let on_connected = move |stream: tokio::net::TcpStream, socket_addr: SocketAddr| {
        let data = data.clone();
        let runtime = runtime.clone();
        async move {
            if runtime.verbose {
                println!("Client connected: {socket_addr}");
            }
            let unsupported_mei = Arc::new(AtomicBool::new(false));
            let illegal_coil = Arc::new(AtomicBool::new(false));
            let service = ModbusService::new(data, &runtime)
                .with_peer(socket_addr.to_string())
                .with_unsupported_mei(unsupported_mei.clone())
                .with_illegal_coil(illegal_coil.clone());
            let stream = MeiRewrite::new(
                IdleTimeoutStream::new(stream, idle),
                unsupported_mei,
                illegal_coil,
            );
            Ok(Some((service, stream)))
        }
    };
    finish_tcp(server, on_connected, "TCP").await
}

async fn serve_rtu_over_tcp_connections(
    server: rtu_over_tcp::Server,
    runtime: ServerRuntime,
    data: Arc<tokio::sync::RwLock<ModbusData>>,
) -> anyhow::Result<()> {
    let idle = Duration::from_secs(runtime.timeout_secs);
    let on_connected = move |stream: tokio::net::TcpStream, socket_addr: SocketAddr| {
        let data = data.clone();
        let runtime = runtime.clone();
        async move {
            if runtime.verbose {
                println!("Client connected: {socket_addr}");
            }
            let service = ModbusService::new(data, &runtime).with_peer(socket_addr.to_string());
            let stream = IdleTimeoutStream::new(stream, idle);
            Ok(Some((service, stream)))
        }
    };
    finish_rtu_over_tcp(server, on_connected, "RTU over TCP").await
}

async fn finish_tcp<F, OnConnected>(
    server: tcp::Server,
    on_connected: OnConnected,
    label: &str,
) -> anyhow::Result<()>
where
    F: Future<
            Output = io::Result<
                Option<(
                    ModbusService,
                    MeiRewrite<IdleTimeoutStream<tokio::net::TcpStream>>,
                )>,
            >,
        > + Send,
    OnConnected: Fn(tokio::net::TcpStream, SocketAddr) -> F + Sync,
{
    let on_process_error = |err| {
        eprintln!("Server error: {err}");
    };
    let ctrl_c = Box::pin(async {
        tokio::signal::ctrl_c().await.ok();
    });
    match server
        .serve_until(&on_connected, on_process_error, ctrl_c)
        .await?
    {
        tokio_modbus::server::Terminated::Finished => println!("\n{label} server finished"),
        tokio_modbus::server::Terminated::Aborted => println!("\n{label} server stopped"),
    }
    Ok(())
}

async fn finish_rtu_over_tcp<F, OnConnected>(
    server: rtu_over_tcp::Server,
    on_connected: OnConnected,
    label: &str,
) -> anyhow::Result<()>
where
    F: Future<
            Output = io::Result<Option<(ModbusService, IdleTimeoutStream<tokio::net::TcpStream>)>>,
        > + Send,
    OnConnected: Fn(tokio::net::TcpStream, SocketAddr) -> F + Sync,
{
    let on_process_error = |err| {
        eprintln!("Server error: {err}");
    };
    let ctrl_c = Box::pin(async {
        tokio::signal::ctrl_c().await.ok();
    });
    match server
        .serve_until(&on_connected, on_process_error, ctrl_c)
        .await?
    {
        tokio_modbus::server::Terminated::Finished => println!("\n{label} server finished"),
        tokio_modbus::server::Terminated::Aborted => println!("\n{label} server stopped"),
    }
    Ok(())
}

pub async fn run_rtu_server(
    device_path: &std::path::Path,
    baud: u32,
    parity: &Parity,
    stop_bits: &StopBits,
    data_bits: &DataBits,
    runtime: ServerRuntime,
    data: Arc<tokio::sync::RwLock<ModbusData>>,
) -> anyhow::Result<()> {
    println!("Serial Configuration:");
    println!("  Baud Rate: {baud}");
    println!("  Parity: {:?}", parity);
    println!("  Stop Bits: {:?}", stop_bits);
    println!("  Data Bits: {:?}", data_bits);

    let builder = tokio_serial::new(device_path.to_string_lossy(), baud)
        .parity((*parity).into())
        .stop_bits((*stop_bits).into())
        .data_bits((*data_bits).into());

    let opened = timeout(Duration::from_secs(runtime.timeout_secs), async {
        tokio_serial::SerialStream::open(&builder)
    })
    .await;

    match opened {
        Ok(Ok(mut serial)) => {
            // Disable exclusive access for virtual ports
            if let Err(e) = serial.set_exclusive(false) {
                println!("Warning: Could not disable exclusive access: {e}");
            }

            let rtu_server = rtu::Server::new(serial);
            let service = ModbusService::new(data, &runtime);
            println!("Modbus RTU server listening on {}", device_path.display());
            println!("Press Ctrl+C to stop the server");

            let serve_task = tokio::spawn(async move { rtu_server.serve_forever(service).await });

            // Wait for Ctrl+C
            tokio::signal::ctrl_c().await?;
            println!("\nStopping RTU server...");

            // Abort the serve task
            serve_task.abort();
            println!("RTU server stopped");
            Ok(())
        }
        Ok(Err(e)) => {
            eprintln!(
                "Failed to open serial device {}: {}",
                device_path.display(),
                e
            );
            Err(e.into())
        }
        Err(_) => {
            eprintln!(
                "Opening {} timed out after {} seconds",
                device_path.display(),
                runtime.timeout_secs
            );
            Err(anyhow::anyhow!("Connection timeout"))
        }
    }
}
