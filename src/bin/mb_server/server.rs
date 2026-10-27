use mb_cli::serial::{DataBits, Parity, StopBits};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::{timeout, Instant};
use tokio_modbus::prelude::*;
use tokio_modbus::server::{rtu, rtu_over_tcp, tcp, Service};

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
struct IdleTimeoutStream<S> {
    inner: S,
    timeout: Duration,
    idle: Option<Pin<Box<tokio::time::Sleep>>>,
}

impl<S> IdleTimeoutStream<S> {
    fn new(inner: S, timeout: Duration) -> Self {
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
        }
    }

    fn with_peer(mut self, peer: String) -> Self {
        self.peer = Some(peer);
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
        Box::pin(async move {
            let SlaveRequest { slave, request } = req;
            let station = slave;
            let outcome = answer(
                &data,
                &units,
                station,
                request.clone(),
                delay,
                forced,
                silence_writes,
            )
            .await;
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
            let service = ModbusService::new(data, &runtime).with_peer(socket_addr.to_string());
            let stream = IdleTimeoutStream::new(stream, idle);
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

#[cfg(test)]
mod tests {
    use super::{
        server_listen, IdleTimeoutStream, ModbusData, ModbusService, ServerListen, ServerRuntime,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;
    use tokio_modbus::prelude::{ExceptionCode, Request, Response, SlaveRequest};
    use tokio_modbus::server::Service;

    fn runtime(units: &[u8]) -> ServerRuntime {
        ServerRuntime {
            units: units.to_vec(),
            verbose: false,
            timeout_secs: 5,
            delay: Duration::ZERO,
            forced: None,
            silence_writes: false,
        }
    }

    fn service(data: Arc<tokio::sync::RwLock<ModbusData>>, units: &[u8]) -> ModbusService {
        ModbusService::new(data, &runtime(units))
    }

    #[test]
    fn test_modbus_data_new() {
        let data = ModbusData::new(100, 200, 300, 400);

        assert_eq!(data.coils.len(), 100);
        assert_eq!(data.discrete_inputs.len(), 200);
        assert_eq!(data.holding_registers.len(), 300);
        assert_eq!(data.input_registers.len(), 400);
        assert!(data.discrete_inputs.iter().all(|bit| !bit));
    }

    #[test]
    fn startup_draw_is_stable_for_one_seed_and_changes_with_another() {
        let first = ModbusData::from_seed(8, 4, 8, 8, 1);
        let again = ModbusData::from_seed(8, 4, 8, 8, 1);
        let other = ModbusData::from_seed(8, 4, 8, 8, 2);
        assert_eq!(first.coils, again.coils);
        assert_eq!(first.holding_registers, again.holding_registers);
        assert_eq!(first.input_registers, again.input_registers);
        assert!(first.discrete_inputs.iter().all(|bit| !bit));
        assert_ne!(first.holding_registers, other.holding_registers);
    }

    #[test]
    fn test_modbus_data_small_sizes() {
        let data = ModbusData::new(1, 2, 3, 4);

        assert_eq!(data.coils.len(), 1);
        assert_eq!(data.discrete_inputs.len(), 2);
        assert_eq!(data.holding_registers.len(), 3);
        assert_eq!(data.input_registers.len(), 4);
        assert!(data.discrete_inputs.iter().all(|bit| !bit));
    }

    #[test]
    fn test_modbus_data_zero_sizes() {
        let data = ModbusData::new(0, 0, 0, 0);

        assert!(data.coils.is_empty());
        assert!(data.discrete_inputs.is_empty());
        assert!(data.holding_registers.is_empty());
        assert!(data.input_registers.is_empty());
    }

    #[tokio::test]
    async fn test_modbus_service_read_coils_valid() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(10, 10, 10, 10)));

        // Set some coil values for testing
        {
            let mut data_lock = data.write().await;
            data_lock.coils[0] = true;
            data_lock.coils[1] = false;
            data_lock.coils[2] = true;
        }

        let service = service(data, &[1]);
        let request = Request::ReadCoils(0, 3);

        let result = service.call(SlaveRequest { slave: 1, request }).await;
        assert!(result.is_ok());

        if let Ok(Some(Response::ReadCoils(coils))) = result {
            assert_eq!(coils.len(), 3);
            assert!(coils[0]);
            assert!(!coils[1]);
            assert!(coils[2]);
        } else {
            panic!("Expected ReadCoils response");
        }
    }

    #[tokio::test]
    async fn test_modbus_service_read_coils_out_of_bounds() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(5, 5, 5, 5)));
        let service = service(data, &[1]);

        // Try to read beyond available coils
        let request = Request::ReadCoils(3, 5); // starts at 3, wants 5 coils = addresses 3,4,5,6,7 but only 0-4 exist

        let result = service.call(SlaveRequest { slave: 1, request }).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ExceptionCode::IllegalDataAddress);
    }

    #[tokio::test]
    async fn test_modbus_service_read_holding_registers() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(10, 10, 10, 10)));
        {
            let mut guard = data.write().await;
            guard.holding_registers[5] = 5;
            guard.holding_registers[6] = 6;
            guard.holding_registers[7] = 7;
        }
        let service = service(data, &[1]);

        let request = Request::ReadHoldingRegisters(5, 3);

        let result = service.call(SlaveRequest { slave: 1, request }).await;
        assert!(result.is_ok());

        if let Ok(Some(Response::ReadHoldingRegisters(registers))) = result {
            assert_eq!(registers.len(), 3);
            assert_eq!(registers[0], 5); // address 5 = value 5
            assert_eq!(registers[1], 6); // address 6 = value 6
            assert_eq!(registers[2], 7); // address 7 = value 7
        } else {
            panic!("Expected ReadHoldingRegisters response");
        }
    }

    #[tokio::test]
    async fn test_modbus_service_write_single_coil() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(10, 10, 10, 10)));
        let service = service(data.clone(), &[1]);

        let request = Request::WriteSingleCoil(3, true);

        let result = service.call(SlaveRequest { slave: 1, request }).await;
        assert!(result.is_ok());

        if let Ok(Some(Response::WriteSingleCoil(addr, value))) = result {
            assert_eq!(addr, 3);
            assert!(value);
        } else {
            panic!("Expected WriteSingleCoil response");
        }

        // Verify the coil was actually set
        let data_lock = data.read().await;
        assert!(data_lock.coils[3]);
    }

    #[tokio::test]
    async fn test_modbus_service_write_single_register() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(10, 10, 10, 10)));
        let service = service(data.clone(), &[1]);

        let request = Request::WriteSingleRegister(2, 12345);

        let result = service.call(SlaveRequest { slave: 1, request }).await;
        assert!(result.is_ok());

        if let Ok(Some(Response::WriteSingleRegister(addr, value))) = result {
            assert_eq!(addr, 2);
            assert_eq!(value, 12345);
        } else {
            panic!("Expected WriteSingleRegister response");
        }

        // Verify the register was actually set
        let data_lock = data.read().await;
        assert_eq!(data_lock.holding_registers[2], 12345);
    }

    #[tokio::test]
    async fn test_modbus_service_write_multiple_coils() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(10, 10, 10, 10)));
        let service = service(data.clone(), &[1]);

        let values = [true, false, true];
        let request = Request::WriteMultipleCoils(1, values.to_vec().into());

        let result = service.call(SlaveRequest { slave: 1, request }).await;
        assert!(result.is_ok());

        if let Ok(Some(Response::WriteMultipleCoils(addr, qty))) = result {
            assert_eq!(addr, 1);
            assert_eq!(qty, 3);
        } else {
            panic!("Expected WriteMultipleCoils response");
        }

        // Verify the coils were actually set
        let data_lock = data.read().await;
        assert!(data_lock.coils[1]);
        assert!(!data_lock.coils[2]);
        assert!(data_lock.coils[3]);
    }

    #[tokio::test]
    async fn test_modbus_service_ignores_other_unit_ids() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
            10, 10, 10, 10, 11,
        )));
        data.write().await.coils[0] = false;
        let service = service(data.clone(), &[1]);
        let request = Request::WriteSingleCoil(0, true);

        let result = service
            .call(SlaveRequest { slave: 2, request })
            .await
            .unwrap();
        assert!(result.is_none());

        let data_lock = data.read().await;
        assert!(!data_lock.coils[0]);
    }

    #[test]
    fn test_server_listen_defaults_to_all_interfaces() {
        let ip = "127.0.0.1".parse().unwrap();
        assert_eq!(
            server_listen(Some(ip), None, false).unwrap(),
            ServerListen::Tcp(ip)
        );
        assert_eq!(
            server_listen(None, Some(PathBuf::from("/dev/ttyUSB0")), false).unwrap(),
            ServerListen::Rtu(PathBuf::from("/dev/ttyUSB0"))
        );
        assert_eq!(
            server_listen(None, None, false).unwrap(),
            ServerListen::Tcp(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
        );
        assert_eq!(
            server_listen(None, None, true).unwrap(),
            ServerListen::RtuOverTcp(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
        );
        assert!(server_listen(Some(ip), Some(PathBuf::from("/dev/ttyUSB0")), false).is_err());
        assert!(server_listen(None, Some(PathBuf::from("/dev/ttyUSB0")), true).is_err());
    }

    #[tokio::test]
    async fn test_idle_timeout_closes_silent_stream() {
        let (_client, server) = tokio::io::duplex(16);
        let mut timed = IdleTimeoutStream::new(server, Duration::from_millis(30));
        let mut buf = [0u8; 4];
        let n = timed.read(&mut buf).await.unwrap();
        assert_eq!(n, 0);
    }

    #[tokio::test]
    async fn broadcast_write_updates_memory_without_a_response() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
            4, 1, 4, 1, 3,
        )));
        let service = service(data.clone(), &[1, 2]);
        let written = service
            .call(SlaveRequest {
                slave: 0,
                request: Request::WriteSingleRegister(1, 77),
            })
            .await
            .unwrap();
        assert!(written.is_none());
        assert_eq!(data.read().await.holding_registers[1], 77);

        let before = data.read().await.holding_registers[0];
        let read = service
            .call(SlaveRequest {
                slave: 0,
                request: Request::ReadHoldingRegisters(0, 1),
            })
            .await
            .unwrap();
        assert!(read.is_none());
        assert_eq!(data.read().await.holding_registers[0], before);
    }

    #[tokio::test]
    async fn stations_share_one_image_and_unknown_ids_stay_silent() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
            4, 1, 4, 1, 4,
        )));
        let service = service(data.clone(), &[1, 2]);
        service
            .call(SlaveRequest {
                slave: 2,
                request: Request::WriteSingleCoil(0, true),
            })
            .await
            .unwrap();
        let seen = service
            .call(SlaveRequest {
                slave: 1,
                request: Request::ReadCoils(0, 1),
            })
            .await
            .unwrap()
            .unwrap();
        assert_eq!(seen, Response::ReadCoils(vec![true]));
        let ignored = service
            .call(SlaveRequest {
                slave: 3,
                request: Request::ReadCoils(0, 1),
            })
            .await
            .unwrap();
        assert!(ignored.is_none());
    }

    #[tokio::test]
    async fn forced_exception_does_not_write_and_other_functions_do() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
            4, 1, 4, 1, 5,
        )));
        let original = data.read().await.holding_registers[1];
        let mut policy = runtime(&[1]);
        policy.forced = Some((0x06, ExceptionCode::IllegalDataAddress));
        let service = ModbusService::new(data.clone(), &policy);
        let denied = service
            .call(SlaveRequest {
                slave: 1,
                request: Request::WriteSingleRegister(1, 9),
            })
            .await;
        assert_eq!(denied.unwrap_err(), ExceptionCode::IllegalDataAddress);
        assert_eq!(data.read().await.holding_registers[1], original);

        service
            .call(SlaveRequest {
                slave: 1,
                request: Request::WriteSingleCoil(0, true),
            })
            .await
            .unwrap()
            .unwrap();
        assert!(data.read().await.coils[0]);
    }

    #[tokio::test]
    async fn silence_skips_a_write() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
            4, 1, 4, 1, 6,
        )));
        let original = data.read().await.holding_registers[0];
        let mut policy = runtime(&[1]);
        policy.silence_writes = true;
        let service = ModbusService::new(data.clone(), &policy);
        let response = service
            .call(SlaveRequest {
                slave: 1,
                request: Request::WriteSingleRegister(0, 5),
            })
            .await
            .unwrap();
        assert!(response.is_none());
        assert_eq!(data.read().await.holding_registers[0], original);
    }

    #[tokio::test]
    async fn delay_waits_before_a_normal_response() {
        let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
            2, 1, 2, 1, 7,
        )));
        let mut policy = runtime(&[1]);
        policy.delay = Duration::from_millis(40);
        let service = ModbusService::new(data, &policy);
        let started = std::time::Instant::now();
        service
            .call(SlaveRequest {
                slave: 1,
                request: Request::ReadHoldingRegisters(0, 1),
            })
            .await
            .unwrap()
            .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(30));
    }
}
