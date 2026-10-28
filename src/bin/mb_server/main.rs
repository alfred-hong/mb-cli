use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

mod address;
mod cli;
mod handle;
mod runtime;
mod serial;
mod snapshot;

use anyhow::Context;
use clap::{CommandFactory, Parser};
use cli::{Cli, forced_fault, served_stations};
use runtime::{
    ModbusData, ServerListen, ServerRuntime, run_rtu_over_tcp_server, run_rtu_server,
    run_tcp_server, server_listen,
};
use snapshot::{load_file, save_file};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if let Some(shell) = cli.completions {
        let mut command = Cli::command();
        write_completions(shell, &mut command, "mb_server")?;
        return Ok(());
    }
    let stations = served_stations(&cli.units)?;
    let forced = forced_fault(cli.exception_function, cli.exception_code)?;
    let listen = server_listen(cli.ip, cli.device.clone(), cli.rtu_over_tcp)?;
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(
        cli.num_coils,
        cli.num_discrete,
        cli.num_holding,
        cli.num_input,
    )));
    if let Some(path) = &cli.csv {
        let mut guard = data.write().await;
        load_file(&mut guard, path)?;
    }

    print_config(&cli, &stations);
    let runtime = ServerRuntime {
        units: stations,
        verbose: cli.verbose,
        timeout_secs: cli.timeout,
        delay: Duration::from_millis(cli.delay_ms.unwrap_or(0)),
        forced,
        silence_writes: cli.silence_writes,
    };
    let served = data.clone();
    let result = match listen {
        ServerListen::Tcp(ip_addr) => {
            println!("Starting Modbus TCP server on {ip_addr}:{}", cli.port);
            run_tcp_server(ip_addr, cli.port, runtime, served).await
        }
        ServerListen::RtuOverTcp(ip_addr) => {
            println!(
                "Starting Modbus RTU over TCP server on {ip_addr}:{}",
                cli.port
            );
            run_rtu_over_tcp_server(ip_addr, cli.port, runtime, served).await
        }
        ServerListen::Rtu(device_path) => {
            println!("Starting Modbus RTU server on {}", device_path.display());
            run_rtu_server(
                &device_path,
                cli.baud,
                &cli.parity,
                &cli.stop_bits,
                &cli.data_bits,
                runtime,
                served,
            )
            .await
        }
    };
    if result.is_ok() {
        if let Some(path) = &cli.csv {
            let guard = data.read().await;
            save_file(&guard, path).context("saving the CSV snapshot")?;
        }
    }
    result
}

/// Write a completion script. A closed pipe is a normal end, not a panic.
fn write_completions(
    shell: clap_complete::Shell,
    command: &mut clap::Command,
    bin_name: &str,
) -> anyhow::Result<()> {
    let mut script = Vec::new();
    clap_complete::generate(shell, command, bin_name, &mut script);
    let mut out = std::io::stdout().lock();
    match std::io::Write::write_all(&mut out, &script).and_then(|()| out.flush()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn print_config(cli: &Cli, stations: &[u8]) {
    let station_list = stations
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    println!("Configuration:");
    println!("  Modbus address: {station_list}");
    println!("  Timeout: {}s", cli.timeout);
    if let Some(delay) = cli.delay_ms {
        println!("  Response delay: {delay}ms");
    }
    if let (Some(function), Some(code)) = (cli.exception_function, cli.exception_code) {
        println!("  Forced exception: function {function} returns {code} and does not write");
    }
    if cli.silence_writes {
        println!("  Write requests are accepted with no response and no stored change");
    }
    if let Some(path) = &cli.csv {
        println!("  CSV snapshot: {}", path.display());
    }
    println!(
        "  Coils: {} (addresses 0-{}), initial value random 0 or 1",
        cli.num_coils,
        cli.num_coils.saturating_sub(1)
    );
    println!(
        "  Discrete Inputs: {} (addresses 0-{}), initial value OFF",
        cli.num_discrete,
        cli.num_discrete.saturating_sub(1)
    );
    println!(
        "  Holding Registers: {} (addresses 0-{}), initial value random 0-65535",
        cli.num_holding,
        cli.num_holding.saturating_sub(1)
    );
    println!(
        "  Input Registers: {} (addresses 0-{}), initial value random 0-65535",
        cli.num_input,
        cli.num_input.saturating_sub(1)
    );
    println!();
}

#[cfg(test)]
#[path = "server_cli_tests.rs"]
mod server_cli_tests;

#[cfg(test)]
#[path = "server_handle_tests.rs"]
mod server_handle_tests;

#[cfg(test)]
#[path = "server_service_tests.rs"]
mod server_service_tests;

#[cfg(test)]
#[path = "server_snapshot_tests.rs"]
mod server_snapshot_tests;
