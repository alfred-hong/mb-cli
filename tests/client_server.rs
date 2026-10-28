//! `mb_client` against a live `mb_server`.
//!
//! Each test starts its own TCP server on 127.0.0.1. The snapshot makes the
//! register values known, because a bare server draws them at startup.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const SNAPSHOT: &str = "\
area,address,value
coil,0,1
coil,1,0
holding,0,1234
holding,1,7
input,0,42
input,1,8
";

fn startup_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct Server {
    child: Child,
    port: u16,
    csv: PathBuf,
    lines: std::sync::Arc<Mutex<Vec<String>>>,
}

impl Server {
    fn start(extra: &[&str]) -> Self {
        let _guard = startup_lock().lock().expect("server startup lock");
        let port = free_port();
        let csv = std::env::temp_dir().join(format!("mb-rs-{}-{port}.csv", std::process::id()));
        std::fs::write(&csv, SNAPSHOT).expect("write snapshot");

        let port_text = port.to_string();
        let csv_text = csv.to_string_lossy().into_owned();
        let mut args = vec![
            "--ip".to_string(),
            "127.0.0.1".to_string(),
            "--port".to_string(),
            port_text,
        ];
        let supplied = |name: &str| extra.contains(&name);
        if !supplied("--num-coils") {
            args.extend(["--num-coils".into(), "2".into()]);
        }
        if !supplied("--num-discrete") {
            args.extend(["--num-discrete".into(), "2".into()]);
        }
        if !supplied("--num-holding") {
            args.extend(["--num-holding".into(), "2".into()]);
        }
        if !supplied("--num-input") {
            args.extend(["--num-input".into(), "2".into()]);
        }
        args.push("--csv".into());
        args.push(csv_text);
        args.extend(extra.iter().map(|arg| (*arg).to_string()));
        let mut command = Command::new(env!("CARGO_BIN_EXE_mb_server"));
        command
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().expect("spawn mb_server");
        let stdout = child.stdout.take().expect("server stdout");
        let lines = std::sync::Arc::new(Mutex::new(Vec::new()));
        let recorded = lines.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if line.contains("listening") {
                    let _ = sender.send(());
                }
                recorded.lock().expect("server log").push(line);
            }
        });
        if receiver.recv_timeout(Duration::from_secs(5)).is_err() {
            let _ = child.kill();
            let status = child.wait();
            let stderr = child.stderr.take();
            let mut detail = String::new();
            if let Some(stderr) = stderr {
                let _ = std::io::Read::read_to_string(
                    &mut std::io::BufReader::new(stderr),
                    &mut detail,
                );
            }
            panic!("mb_server did not listen on {port}: {status:?} {detail}");
        }
        Self {
            child,
            port,
            csv,
            lines,
        }
    }

    fn stop_and_keep_snapshot(&mut self) {
        let pid = self.child.id().to_string();
        let _ = Command::new("kill").args(["-INT", &pid]).status();
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("mb_server did not exit after SIGINT");
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.csv);
    }
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind a free port");
    listener.local_addr().expect("port").port()
}

fn expect_server_rejects(args: &[&str], needle: &str) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_mb_server"))
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mb_server");
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.try_wait().expect("server status") {
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = std::io::Read::read_to_string(&mut stderr, &mut err);
            }
            let _ = child.kill();
            let _ = child.wait();
            assert!(!status.success(), "server accepted {:?}\n{err}", args);
            assert!(err.contains(needle), "expected {needle} in:\n{err}");
            return;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("mb_server kept running; expected it to reject {args:?}");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn run_client(port: u16, args: &[&str]) -> Output {
    let port = port.to_string();
    Command::new(env!("CARGO_BIN_EXE_mb_client"))
        .args(args)
        .args(["--ip", "127.0.0.1", "--port", &port])
        .output()
        .expect("spawn mb_client")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn expect_no_response(output: &Output) {
    assert!(
        !output.status.success(),
        "client should fail when the server stays silent:\n{}",
        stdout(output)
    );
    let err = stderr(output);
    assert!(
        err.contains("timed out") || err.contains("Operation now in progress"),
        "{err}"
    );
}

fn expect_ok(output: &Output) -> String {
    let text = stdout(output);
    assert!(
        output.status.success(),
        "client failed: {}\n{text}",
        stderr(output)
    );
    text
}

fn client_ok(port: u16, args: &[&str]) -> String {
    let output = run_client(port, args);
    expect_ok(&output)
}

fn assert_client_ok(port: u16, args: &[&str], needle: &str) {
    let text = client_ok(port, args);
    assert!(
        text.contains(needle),
        "expected {needle} in:\n{text}\nargs: {args:?}"
    );
}

fn assert_client_err(port: u16, args: &[&str], needle: &str) {
    let output = run_client(port, args);
    assert!(
        !output.status.success(),
        "client accepted {:?}\n{}",
        args,
        stdout(&output)
    );
    let err = format!("{}{}", stderr(&output), stdout(&output));
    assert!(err.contains(needle), "expected {needle} in:\n{err}");
}

fn read_holding(port: u16, station: u8, address: u16, qty: u16) -> String {
    let station = station.to_string();
    let address = address.to_string();
    let qty = qty.to_string();
    let output = run_client(
        port,
        &[
            "read", "holding", "--addr", &station, "--unit", &address, "--qty", &qty,
        ],
    );
    expect_ok(&output)
}

#[test]
fn client_reads_the_server_snapshot() {
    let server = Server::start(&[]);

    let holding = read_holding(server.port, 1, 0, 2);
    assert!(holding.contains("0        1234"));
    assert!(holding.contains("1        7"));

    let decoded = run_client(
        server.port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "1", "--decode", "hex",
        ],
    );
    let decoded = expect_ok(&decoded);
    assert!(decoded.contains("0x04D2"), "{decoded}");

    let coils = run_client(
        server.port,
        &["read", "coil", "--addr", "1", "--unit", "0", "--qty", "2"],
    );
    let coils = expect_ok(&coils);
    assert!(coils.contains("0        ON"), "{coils}");
    assert!(coils.contains("1        OFF"), "{coils}");

    let inputs = run_client(
        server.port,
        &["read", "input", "--addr", "1", "--unit", "0", "--qty", "1"],
    );
    let inputs = expect_ok(&inputs);
    assert!(inputs.contains("0        42"), "{inputs}");

    let discrete = run_client(
        server.port,
        &[
            "read", "discrete", "--addr", "1", "--unit", "0", "--qty", "2",
        ],
    );
    let discrete = expect_ok(&discrete);
    assert!(discrete.contains("0        OFF"), "{discrete}");
    assert!(discrete.contains("1        OFF"), "{discrete}");
}

#[test]
fn client_write_is_visible_from_the_other_station() {
    let server = Server::start(&["--modbus_address", "1", "--modbus_address", "2"]);

    let written = run_client(
        server.port,
        &[
            "write", "holding", "--addr", "2", "--unit", "0", "--value", "99",
        ],
    );
    let written = expect_ok(&written);
    assert!(written.contains("Wrote holding register at address 0 with value 99"));

    let seen = read_holding(server.port, 1, 0, 1);
    assert!(seen.contains("0        99"), "{seen}");

    let coil = run_client(
        server.port,
        &[
            "write", "coil", "--addr", "1", "--unit", "1", "--value", "1",
        ],
    );
    expect_ok(&coil);
    let coils = run_client(
        server.port,
        &["read", "coil", "--addr", "2", "--unit", "1", "--qty", "1"],
    );
    let coils = expect_ok(&coils);
    assert!(coils.contains("1        ON"), "{coils}");
}

#[test]
fn address_outside_the_window_is_exception_02() {
    let server = Server::start(&[]);
    let output = run_client(
        server.port,
        &[
            "read", "holding", "--addr", "1", "--unit", "2", "--qty", "1",
        ],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("IllegalDataAddress"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn forced_exception_does_not_change_the_register() {
    let server = Server::start(&["--exception-function", "6", "--exception-code", "2"]);
    let output = run_client(
        server.port,
        &[
            "write", "holding", "--addr", "1", "--unit", "0", "--value", "55",
        ],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("IllegalDataAddress"),
        "{}",
        stderr(&output)
    );

    let holding = read_holding(server.port, 1, 0, 1);
    assert!(holding.contains("0        1234"), "{holding}");
}

#[test]
fn unknown_station_gets_no_response() {
    let server = Server::start(&[]);
    let output = run_client(
        server.port,
        &[
            "read",
            "holding",
            "--addr",
            "9",
            "--unit",
            "0",
            "--qty",
            "1",
            "--response-timeout",
            "1",
        ],
    );
    assert!(!output.status.success());
    expect_no_response(&output);
}

#[test]
fn silence_discards_a_client_write() {
    let server = Server::start(&["--silence-writes"]);
    let output = run_client(
        server.port,
        &[
            "write",
            "holding",
            "--addr",
            "1",
            "--unit",
            "1",
            "--value",
            "55",
            "--response-timeout",
            "1",
        ],
    );
    assert!(!output.status.success());
    expect_no_response(&output);

    let holding = read_holding(server.port, 1, 1, 1);
    assert!(holding.contains("1        7"), "{holding}");
}

#[test]
fn clean_stop_rewrites_the_snapshot() {
    let mut server = Server::start(&[]);
    let written = run_client(
        server.port,
        &[
            "write", "holding", "--addr", "1", "--unit", "0", "--value", "99",
        ],
    );
    expect_ok(&written);
    let csv = server.csv.clone();
    server.stop_and_keep_snapshot();
    let text = std::fs::read_to_string(&csv).expect("read snapshot");
    assert!(text.contains("holding,0,99"), "{text}");
    assert_eq!(text.lines().next(), Some("area,address,value"));
}

#[test]
fn verbose_log_names_the_request_and_the_peer() {
    let server = Server::start(&["-v"]);
    let holding = read_holding(server.port, 1, 0, 1);
    assert!(holding.contains("0        1234"), "{holding}");
    let log = server.lines.lock().expect("log").join("\n");
    assert!(log.contains("station=1"), "{log}");
    assert!(log.contains("fc=0x03"), "{log}");
    assert!(log.contains("addr=0"), "{log}");
    assert!(log.contains("qty=1"), "{log}");
    assert!(log.contains("result=ok"), "{log}");
    assert!(log.contains("from 127.0.0.1"), "{log}");
}

#[test]
fn response_delay_is_visible_to_the_client() {
    let server = Server::start(&["--delay-ms", "400"]);
    let started = Instant::now();
    let holding = read_holding(server.port, 1, 0, 1);
    assert!(holding.contains("0        1234"), "{holding}");
    assert!(started.elapsed() >= Duration::from_millis(300));
}

#[test]
fn broadcast_write_updates_the_shared_image() {
    let server = Server::start(&[]);
    let output = run_client(
        server.port,
        &[
            "write",
            "holding",
            "--addr",
            "0",
            "--unit",
            "0",
            "--value",
            "77",
            "--response-timeout",
            "1",
        ],
    );
    expect_no_response(&output);
    let holding = read_holding(server.port, 1, 0, 1);
    assert!(holding.contains("0        77"), "{holding}");
}

#[test]
fn device_failure_exception_does_not_return_the_register() {
    let server = Server::start(&["--exception-function", "3", "--exception-code", "4"]);
    let output = run_client(
        server.port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "1",
        ],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("ServerDeviceFailure"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn server_rejects_bad_arguments_and_snapshots() {
    expect_server_rejects(&["--modbus_address", "0"], "broadcast");
    expect_server_rejects(
        &["--ip", "127.0.0.1", "--device", "/dev/null"],
        "cannot be used with",
    );
    expect_server_rejects(
        &["--device", "/dev/null", "--rtu-over-tcp"],
        "cannot be used with",
    );
    expect_server_rejects(
        &["--exception-function", "6"],
        "--exception-function requires --exception-code",
    );
    expect_server_rejects(
        &["--exception-function", "6", "--exception-code", "9"],
        "must be 2, 3, or 4",
    );

    let dir = std::env::temp_dir();
    let discrete = dir.join(format!("mb-rs-discrete-{}.csv", std::process::id()));
    std::fs::write(&discrete, "area,address,value\ndiscrete,0,1\n").unwrap();
    expect_server_rejects(
        &[
            "--ip",
            "127.0.0.1",
            "--num-discrete",
            "2",
            "--csv",
            discrete.to_str().unwrap(),
        ],
        "discrete",
    );
    let _ = std::fs::remove_file(&discrete);

    let outside = dir.join(format!("mb-rs-outside-{}.csv", std::process::id()));
    std::fs::write(&outside, "area,address,value\nholding,5,1\n").unwrap();
    expect_server_rejects(
        &[
            "--ip",
            "127.0.0.1",
            "--num-holding",
            "2",
            "--csv",
            outside.to_str().unwrap(),
        ],
        "outside",
    );
    let _ = std::fs::remove_file(&outside);

    let missing = dir.join(format!("mb-rs-missing-{}.csv", std::process::id()));
    let _ = std::fs::remove_file(&missing);
    expect_server_rejects(
        &["--ip", "127.0.0.1", "--csv", missing.to_str().unwrap()],
        "reading",
    );
}

#[test]
fn client_reports_a_closed_port() {
    let port = free_port();
    let output = run_client(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "1",
        ],
    );
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(
        err.contains("Failed to connect") || err.contains("Connection refused"),
        "{err}"
    );
}

#[test]
fn server_reports_a_port_already_in_use() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("hold a port");
    let port = listener.local_addr().expect("port").port().to_string();
    expect_server_rejects(
        &["--ip", "127.0.0.1", "--port", &port],
        "Address already in use",
    );
}

#[test]
fn idle_client_is_disconnected() {
    let server = Server::start(&["--timeout", "1"]);
    let addr = format!("127.0.0.1:{}", server.port);
    let mut stream = std::net::TcpStream::connect(addr).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut buf = [0u8; 8];
    let n = std::io::Read::read(&mut stream, &mut buf).expect("read until close");
    assert_eq!(n, 0);
}

#[test]
fn scan_interval_is_shorter_than_one_second() {
    let server = Server::start(&["-v"]);
    let output = run_client(
        server.port,
        &[
            "read",
            "holding",
            "--addr",
            "1",
            "--unit",
            "0",
            "--qty",
            "1",
            "--timeout",
            "2",
            "--interval",
            "400",
        ],
    );
    expect_ok(&output);
    let text = stdout(&output);
    assert!(text.contains("requests="), "{text}");
    let log = server.lines.lock().expect("log").join("\n");
    let samples = log.matches("fc=0x03").count();
    assert!(samples >= 3, "samples={samples}\n{log}");
}

#[test]
fn a_failed_scan_stays_up_and_counts_the_miss() {
    let server = Server::start(&["--exception-function", "3", "--exception-code", "2"]);
    let output = run_client(
        server.port,
        &[
            "read",
            "holding",
            "--addr",
            "1",
            "--unit",
            "0",
            "--qty",
            "1",
            "--timeout",
            "2",
            "--interval",
            "400",
        ],
    );
    expect_ok(&output);
    let text = format!("{}{}", stdout(&output), stderr(&output));
    assert!(text.contains("valid=0"), "{text}");
    assert!(text.contains("IllegalDataAddress"), "{text}");
}

#[test]
fn scan_log_records_each_sample() {
    let server = Server::start(&[]);
    let path = std::env::temp_dir().join(format!("mb-rs-scan-{}.log", std::process::id()));
    let log = path.to_str().expect("log path");
    let output = run_client(
        server.port,
        &[
            "read",
            "holding",
            "--addr",
            "1",
            "--unit",
            "0",
            "--qty",
            "1",
            "--timeout",
            "2",
            "--interval",
            "400",
            "--log",
            log,
        ],
    );
    expect_ok(&output);
    let text = std::fs::read_to_string(&path).expect("read scan log");
    let _ = std::fs::remove_file(&path);
    assert!(text.lines().count() >= 2, "{text}");
    assert!(text.contains("1234"), "{text}");
    assert!(text.contains('T') && text.contains('Z'), "{text}");
}

#[test]
fn modicon_notation_prints_the_area_reference() {
    let server = Server::start(&[]);
    let holding = run_client(
        server.port,
        &[
            "read",
            "holding",
            "--addr",
            "1",
            "--unit",
            "0",
            "--qty",
            "1",
            "--notation",
            "modicon",
        ],
    );
    expect_ok(&holding);
    let text = stdout(&holding);
    assert!(text.contains("40001"), "{text}");
    assert!(text.contains("1234"), "{text}");
}

#[test]
fn verbose_client_prints_frame_bytes() {
    let server = Server::start(&[]);
    let output = run_client(
        server.port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "1", "-v",
        ],
    );
    expect_ok(&output);
    let err = stderr(&output);
    assert!(err.contains("tx "), "{err}");
    assert!(err.contains("rx "), "{err}");
}

#[test]
fn response_timeout_ends_a_silent_station_sooner() {
    let server = Server::start(&[]);
    let started = Instant::now();
    let output = run_client(
        server.port,
        &[
            "read",
            "holding",
            "--addr",
            "9",
            "--unit",
            "0",
            "--qty",
            "1",
            "--response-timeout",
            "1",
        ],
    );
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_secs(2) && elapsed < Duration::from_secs(5),
        "three one-second attempts took {elapsed:?}"
    );
    expect_no_response(&output);
}

#[test]
fn mei_type_13_returns_illegal_function_on_the_same_connection() {
    let server = Server::start(&[]);
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout");
    let mei13 = [0x00, 0x11, 0x00, 0x00, 0x00, 0x04, 0x01, 0x2B, 0x0D, 0x00];
    std::io::Write::write_all(&mut stream, &mei13).expect("write mei 13");
    let mut buf = [0u8; 16];
    let n = std::io::Read::read(&mut stream, &mut buf).expect("read exception");
    assert_eq!(
        &buf[..n],
        &[0x00, 0x11, 0x00, 0x00, 0x00, 0x03, 0x01, 0xAB, 0x01]
    );
    let read = [
        0x00, 0x12, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x01,
    ];
    std::io::Write::write_all(&mut stream, &read).expect("write holding read");
    let n = std::io::Read::read(&mut stream, &mut buf).expect("read holding");
    assert!(n >= 11, "short response {n} {buf:?}");
    assert_eq!(buf[7], 0x03, "function {buf:?}");
    assert_eq!(&buf[9..11], &1234u16.to_be_bytes());
}

#[test]
fn illegal_coil_value_returns_exception_03_and_leaves_the_coil() {
    let server = Server::start(&[]);
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", server.port)).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .expect("read timeout");
    let illegal = [
        0x00, 0x21, 0x00, 0x00, 0x00, 0x06, 0x01, 0x05, 0x00, 0x00, 0x00, 0x01,
    ];
    std::io::Write::write_all(&mut stream, &illegal).expect("write illegal coil");
    let mut buf = [0u8; 16];
    let n = std::io::Read::read(&mut stream, &mut buf).expect("read exception");
    assert_eq!(
        &buf[..n],
        &[0x00, 0x21, 0x00, 0x00, 0x00, 0x03, 0x01, 0x85, 0x03]
    );
    let read = [
        0x00, 0x22, 0x00, 0x00, 0x00, 0x06, 0x01, 0x01, 0x00, 0x00, 0x00, 0x01,
    ];
    std::io::Write::write_all(&mut stream, &read).expect("write coil read");
    let n = std::io::Read::read(&mut stream, &mut buf).expect("read coil");
    assert!(n >= 10, "short response {n} {buf:?}");
    assert_eq!(buf[7], 0x01, "function {buf:?}");
    assert_eq!(buf[9], 0x01, "coil 0 changed: {buf:?}");
}

#[test]
fn raw_function_reads_the_same_holding_register() {
    let server = Server::start(&[]);
    let output = run_client(
        server.port,
        &[
            "raw",
            "--addr",
            "1",
            "--function",
            "3",
            "--data",
            "00,00,00,01",
        ],
    );
    expect_ok(&output);
    assert!(stdout(&output).contains("1234"), "{}", stdout(&output));
}

#[test]
fn every_read_area_address_form_and_decode() {
    let server = Server::start(&[]);
    let port = server.port;

    assert_client_ok(
        port,
        &["read", "coil", "--addr", "1", "--unit", "0", "--qty", "2"],
        "0        ON",
    );
    assert_client_ok(
        port,
        &[
            "read", "0xxxx", "--addr", "1", "--unit", "00001", "--qty", "2",
        ],
        "1        OFF",
    );
    assert_client_ok(
        port,
        &[
            "read",
            "discrete-input",
            "--addr",
            "1",
            "--unit",
            "10001",
            "--qty",
            "2",
        ],
        "0        OFF",
    );
    assert_client_ok(
        port,
        &["read", "input-register", "--addr", "1", "--unit", "30001"],
        "0        42",
    );
    assert_client_ok(
        port,
        &[
            "read",
            "holding-register",
            "--addr",
            "1",
            "--unit",
            "40001",
            "--qty",
            "2",
        ],
        "1        7",
    );
    assert_client_ok(
        port,
        &["read", "4xxxx", "--addr", "1", "--unit", "0x0"],
        "0        1234",
    );

    assert_client_ok(
        port,
        &[
            "read",
            "coil",
            "--addr",
            "1",
            "--unit",
            "0",
            "--notation",
            "modicon",
        ],
        "00001",
    );
    assert_client_ok(
        port,
        &[
            "read",
            "discrete",
            "--addr",
            "1",
            "--unit",
            "0",
            "--notation",
            "modicon",
        ],
        "10001",
    );
    assert_client_ok(
        port,
        &[
            "read",
            "input",
            "--addr",
            "1",
            "--unit",
            "0",
            "--notation",
            "modicon",
        ],
        "30001",
    );

    assert_client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--decode", "hex",
        ],
        "0x04D2",
    );
    assert_client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--decode", "bin",
        ],
        "0000_0100_1101_0010",
    );
    assert_client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--decode", "i16",
        ],
        "1234",
    );
    assert_client_ok(
        port,
        &[
            "read",
            "holding",
            "--addr",
            "1",
            "--unit",
            "0",
            "--decode",
            "u16",
            "--swap-bytes",
        ],
        "53764",
    );
    assert_client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "2", "--decode", "u32",
        ],
        "80871431",
    );
    assert_client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "2", "--decode", "u32",
            "--order", "lsrf",
        ],
        "459986",
    );
    assert_client_ok(
        port,
        &[
            "read", "input", "--addr", "1", "--unit", "0", "--qty", "2", "--decode", "i32",
        ],
        "2752520",
    );
    assert_client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "2", "--decode", "f32",
        ],
        "4.9370788e-36",
    );
    let ansi = client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--decode", "ansi",
        ],
    );
    assert!(ansi.contains(".\u{d2}"), "{ansi}");
    let wide = client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "2", "--decode", "u64",
        ],
    );
    assert!(wide.contains("needs 4 registers"), "{wide}");
}

#[test]
fn every_write_file_fifo_identity_and_diagnostic() {
    let server = Server::start(&["--num-holding", "4"]);
    let port = server.port;

    assert_client_ok(
        port,
        &[
            "write", "coil", "--addr", "1", "--unit", "0", "--value", "0,1",
        ],
        "Wrote 2 coil",
    );
    assert_client_ok(
        port,
        &["read", "coil", "--addr", "1", "--unit", "1"],
        "1        ON",
    );

    assert_client_ok(
        port,
        &[
            "write", "holding", "--addr", "1", "--unit", "0", "--value", "11,22",
        ],
        "Wrote 2 holding",
    );
    assert_client_ok(
        port,
        &["read", "holding", "--addr", "1", "--unit", "1"],
        "1        22",
    );

    assert_client_ok(
        port,
        &[
            "write", "mask", "--addr", "1", "--unit", "1", "--and", "0", "--or", "0x00FF",
        ],
        "Mask-wrote",
    );
    assert_client_ok(
        port,
        &["read", "holding", "--addr", "1", "--unit", "1"],
        "1        255",
    );

    assert_client_ok(
        port,
        &[
            "read-write",
            "--addr",
            "1",
            "--read-unit",
            "0",
            "--read-qty",
            "2",
            "--unit",
            "0",
            "--value",
            "9,8",
            "--decode",
            "u16",
        ],
        "9",
    );
    assert_client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "2",
        ],
        "0        9",
    );

    assert_client_ok(
        port,
        &["write", "file", "--addr", "1", "--group", "4:7:0x1234,2"],
        "File 4 record 7",
    );
    let file = client_ok(
        port,
        &[
            "read", "file", "--addr", "1", "--group", "4:7:2", "--decode", "u16",
        ],
    );
    assert!(file.contains("4660"), "{file}");
    assert!(file.contains("2"), "{file}");

    assert_client_ok(
        port,
        &[
            "write", "holding", "--addr", "1", "--unit", "0", "--value", "1",
        ],
        "Wrote holding",
    );
    let fifo = client_ok(port, &["read", "fifo", "--addr", "1", "--unit", "0"]);
    assert!(fifo.contains("count 1"), "{fifo}");
    assert!(fifo.contains("1        8"), "{fifo}");

    assert_client_ok(
        port,
        &[
            "write", "holding", "--addr", "1", "--unit", "0", "--value", "1,2,3,4",
        ],
        "Wrote 4 holding",
    );
    assert_client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "4", "--decode", "u64",
        ],
        "281483566841860",
    );
    assert_client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "4", "--decode", "i64",
        ],
        "281483566841860",
    );
    assert_client_ok(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "4", "--decode", "f64",
        ],
        "1.390713602454213e-309",
    );

    let status = client_ok(port, &["read", "exception-status", "--addr", "1"]);
    assert!(status.contains("coil 0: OFF"), "{status}");
    assert!(status.contains("coil 1: ON"), "{status}");

    let counter = client_ok(port, &["read", "event-counter", "--addr", "1"]);
    assert!(counter.contains("ready"), "{counter}");
    assert!(counter.contains("Event count:"), "{counter}");

    let log = client_ok(port, &["read", "event-log", "--addr", "1"]);
    assert!(log.contains("receive"), "{log}");
    assert!(log.contains("send"), "{log}");

    let identity = client_ok(port, &["read", "server-id", "--addr", "1"]);
    assert!(identity.contains("Server ID: 1"), "{identity}");
    assert!(identity.contains("ON"), "{identity}");
    assert!(identity.contains("mb-rs"), "{identity}");

    let basic = client_ok(
        port,
        &["read", "device-id", "--addr", "1", "--code", "basic"],
    );
    assert!(basic.contains("VendorName"), "{basic}");
    assert!(basic.contains("mb-rs"), "{basic}");
    assert!(basic.contains("0x82"), "{basic}");
    let regular = client_ok(
        port,
        &["read", "device-id", "--addr", "1", "--code", "regular"],
    );
    assert!(regular.contains("VendorUrl"), "{regular}");
    let extended = client_ok(
        port,
        &["read", "device-id", "--addr", "1", "--code", "extended"],
    );
    assert!(extended.contains("0x82"), "{extended}");
    assert!(extended.contains("ModelName"), "{extended}");
    let specific = client_ok(
        port,
        &[
            "read",
            "device-id",
            "--addr",
            "1",
            "--code",
            "specific",
            "--object",
            "1",
        ],
    );
    assert!(specific.contains("ProductCode"), "{specific}");
    assert_client_err(
        port,
        &[
            "read",
            "device-id",
            "--addr",
            "1",
            "--code",
            "specific",
            "--object",
            "99",
        ],
        "IllegalDataAddress",
    );

    assert_client_ok(
        port,
        &[
            "diagnostics",
            "--addr",
            "1",
            "--sub",
            "return-query",
            "--data",
            "0x1234",
        ],
        "0x1234",
    );
    assert_client_ok(
        port,
        &["diagnostics", "--addr", "1", "--sub", "echo", "--data", "7"],
        "return query data",
    );
    assert_client_ok(
        port,
        &[
            "diagnostics",
            "--addr",
            "1",
            "--sub",
            "diagnostic-register",
            "--data",
            "0",
        ],
        "0x0000",
    );
    assert_client_ok(
        port,
        &[
            "diagnostics",
            "--addr",
            "1",
            "--sub",
            "ascii-delimiter",
            "--data",
            "0x0A00",
        ],
        "0x0A00",
    );
    for sub in [
        "clear-counters",
        "bus-messages",
        "bus-comm-errors",
        "bus-exceptions",
        "server-messages",
        "server-no-response",
        "server-nak",
        "server-busy",
        "bus-overrun",
        "clear-overrun",
        "restart",
    ] {
        assert_client_ok(
            port,
            &["diagnostics", "--addr", "1", "--sub", sub],
            "Diagnostics",
        );
    }

    let silent = run_client(
        port,
        &[
            "diagnostics",
            "--addr",
            "1",
            "--sub",
            "listen-only",
            "--response-timeout",
            "1",
        ],
    );
    expect_no_response(&silent);
    let hidden = run_client(
        port,
        &[
            "read",
            "holding",
            "--addr",
            "1",
            "--unit",
            "0",
            "--response-timeout",
            "1",
        ],
    );
    expect_no_response(&hidden);
    assert_client_ok(
        port,
        &[
            "diagnostics",
            "--addr",
            "1",
            "--sub",
            "restart",
            "--data",
            "0xFF00",
        ],
        "restart communications",
    );
    assert_client_ok(
        port,
        &["read", "holding", "--addr", "1", "--unit", "0"],
        "0        1",
    );

    assert_client_err(
        port,
        &["raw", "--addr", "1", "--function", "65"],
        "IllegalFunction",
    );
    assert_client_err(
        port,
        &[
            "raw",
            "--addr",
            "1",
            "--function",
            "3",
            "--data",
            "00,00,00,00",
        ],
        "IllegalDataValue",
    );
    assert_client_err(
        port,
        &[
            "write", "discrete", "--addr", "1", "--unit", "0", "--value", "1",
        ],
        "read-only",
    );
    assert_client_err(
        port,
        &[
            "write", "input", "--addr", "1", "--unit", "0", "--value", "1",
        ],
        "read-only",
    );
    assert_client_err(
        port,
        &[
            "read", "holding", "--addr", "1", "--unit", "0", "--qty", "0",
        ],
        "1-125",
    );
}

#[test]
fn completions_survive_a_closed_pipe() {
    for (bin, args) in [
        (
            env!("CARGO_BIN_EXE_mb_client"),
            &["completions", "bash"][..],
        ),
        (env!("CARGO_BIN_EXE_mb_server"), &["completions", "zsh"][..]),
    ] {
        let mut child = Command::new(bin)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn completions");
        let mut stdout = child.stdout.take().expect("stdout");
        let mut sample = [0u8; 64];
        let _ = std::io::Read::read(&mut stdout, &mut sample);
        drop(stdout);
        let status = child.wait().expect("wait");
        let mut err = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut err);
        }
        assert!(status.success(), "{bin} {args:?} failed: {err}");
        assert!(!err.contains("panicked"), "{bin} panicked:\n{err}");
    }
}

fn usage_text(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_mb_client"))
        .args(args)
        .output()
        .expect("spawn mb_client");
    assert!(
        !output.status.success(),
        "{args:?} was accepted\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    )
}

#[test]
fn validation_errors_name_the_limit_before_connecting() {
    let cases: &[(&[&str], &str)] = &[
        (&["read", "holding", "--qty", "0"], "1-125"),
        (&["read", "holding", "--qty", "-1"], "1-125"),
        (&["read", "coil", "--qty", "2001"], "1-2000"),
        (&["read", "holding", "--unit", "30001"], "an input register"),
        (&["read", "holding", "--unit", "465537"], "1-65536"),
        (&["read", "holding", "--unit", "-1"], "0-65535"),
        (
            &["write", "holding", "--unit", "0", "--value", "0xGG"],
            "hexadecimal value",
        ),
        (&["diagnostics", "--sub", "nope"], "sub-function"),
        (&["read", "file", "--group", "0:0:1"], "1-65535"),
        (&["read", "file", "--group", "1:10000:1"], "0x270F"),
        (&["read", "holding", "--interval", "-1"], "milliseconds"),
        (&["read", "holding", "--addr", "-1"], "0-255"),
        (&["read", "holding", "--addr", "256"], "0-255"),
        (
            &["write", "discrete", "--unit", "0", "--value", "1"],
            "read-only",
        ),
        (
            &["write", "input", "--unit", "0", "--value", "1"],
            "read-only",
        ),
        (
            &[
                "read",
                "holding",
                "--ip",
                "127.0.0.1",
                "--unit",
                "0",
                "--order",
                "lsrf",
                "--response-timeout",
                "1",
            ],
            "--order lsrf requires --decode",
        ),
        (
            &[
                "read",
                "holding",
                "--ip",
                "127.0.0.1",
                "--unit",
                "0",
                "--swap-bytes",
                "--response-timeout",
                "1",
            ],
            "--swap-bytes requires --decode",
        ),
        (
            &["--ip", "127.0.0.1", "read", "holding", "--unit", "0"],
            "after the command",
        ),
    ];
    for (args, needle) in cases {
        let text = usage_text(args);
        assert!(
            text.contains(needle),
            "expected {needle} in:\n{text}\nargs: {args:?}"
        );
        assert!(
            !text.contains("Connection refused"),
            "validation opened a connection for {args:?}:\n{text}"
        );
    }
}
