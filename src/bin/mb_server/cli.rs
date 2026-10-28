use crate::serial::{DataBits, Parity, StopBits};
use clap::Parser;
use std::{net::IpAddr, path::PathBuf};

/// Modbus TCP and RTU memory server.
#[derive(Parser, Debug)]
#[command(
    name = "mb_server",
    author,
    version,
    about = "Modbus TCP and RTU memory server",
    after_help = "EXAMPLES:\n    mb_server\n    mb_server --port 5020 --modbus_address 1 --modbus_address 2\n    mb_server --rtu-over-tcp --port 5020\n    mb_server --device /dev/ttyUSB0 --baud 9600"
)]
pub struct Cli {
    /// IP address to bind to (TCP only)
    #[arg(long, value_parser = clap::value_parser!(IpAddr), conflicts_with = "device", display_order = 1)]
    pub ip: Option<IpAddr>,

    /// Port to listen on (TCP only)
    #[arg(long, default_value_t = 502, display_order = 2)]
    pub port: u16,

    /// Serial device path (RTU only)
    #[arg(long, conflicts_with = "ip", display_order = 3)]
    pub device: Option<PathBuf>,

    /// Baud rate for serial communication (RTU only)
    #[arg(long, default_value_t = 9600, display_order = 4)]
    pub baud: u32,

    /// Parity for serial communication (RTU only)
    #[arg(long, value_enum, default_value = "none", display_order = 5)]
    pub parity: Parity,

    /// Stop bits for serial communication (RTU only)
    #[arg(long, value_enum, default_value = "1", display_order = 6)]
    pub stop_bits: StopBits,

    /// Data bits for serial communication (RTU only)
    #[arg(long, value_enum, default_value = "8", display_order = 7)]
    pub data_bits: DataBits,

    /// Modbus station address. Repeat the flag for more than one station. `--addr` is the same flag.
    /// Station 0 is rejected. Omitting the flag serves station 1.
    #[arg(
        long = "modbus_address",
        visible_alias = "addr",
        value_name = "ADDRESS",
        action = clap::ArgAction::Append,
        display_order = 8
    )]
    pub units: Vec<u8>,

    /// Timeout for binding, opening the serial port, and idle TCP connections, in seconds
    #[arg(long, default_value_t = 5, display_order = 9)]
    pub timeout: u64,

    /// Number of coils (0-65535)
    #[arg(long, default_value_t = 10000, display_order = 10)]
    pub num_coils: u16,

    /// Number of discrete inputs (0-65535)
    #[arg(long, default_value_t = 10000, display_order = 11)]
    pub num_discrete: u16,

    /// Number of holding registers (0-65535)
    #[arg(long, default_value_t = 10000, display_order = 12)]
    pub num_holding: u16,

    /// Number of input registers (0-65535)
    #[arg(long, default_value_t = 10000, display_order = 13)]
    pub num_input: u16,

    /// Verbose logging
    #[arg(long, short, display_order = 14)]
    pub verbose: bool,

    /// Print a shell completion script and exit.
    #[arg(long, value_enum, value_name = "SHELL", display_order = 20)]
    pub completions: Option<clap_complete::Shell>,

    /// CSV file of `area,address,value` rows for coils, holding registers, and input registers
    #[arg(long, value_name = "FILE", display_order = 15)]
    pub csv: Option<PathBuf>,

    /// Wait this many milliseconds after a normal request is applied and before the response is sent
    #[arg(long = "delay-ms", value_name = "MILLISECONDS", display_order = 16)]
    pub delay_ms: Option<u64>,

    /// Function code that returns `--exception-code` and does not change memory
    #[arg(long = "exception-function", value_name = "CODE", display_order = 17)]
    pub exception_function: Option<u8>,

    /// Exception returned for `--exception-function`: 2, 3, or 4
    #[arg(long = "exception-code", value_name = "CODE", display_order = 18)]
    pub exception_code: Option<u8>,

    /// Accept write requests without storing them and without sending a response
    #[arg(long = "silence-writes", display_order = 19)]
    pub silence_writes: bool,

    /// Serve Modbus RTU frames on the TCP port instead of Modbus TCP
    #[arg(long = "rtu-over-tcp", conflicts_with = "device", display_order = 20)]
    pub rtu_over_tcp: bool,
}

/// Station ids fixed for this process. An empty list means station 1.
pub fn served_stations(units: &[u8]) -> anyhow::Result<Vec<u8>> {
    let stations = if units.is_empty() {
        vec![1]
    } else {
        units.to_vec()
    };
    if stations.contains(&0) {
        return Err(anyhow::anyhow!(
            "station 0 is the broadcast address and cannot be selected with --modbus_address"
        ));
    }
    Ok(stations)
}

/// Pair `--exception-function` with exception 2, 3, or 4.
pub fn forced_fault(
    function: Option<u8>,
    code: Option<u8>,
) -> anyhow::Result<Option<(u8, tokio_modbus::prelude::ExceptionCode)>> {
    use tokio_modbus::prelude::ExceptionCode;
    match (function, code) {
        (None, None) => Ok(None),
        (Some(function), Some(2)) => Ok(Some((function, ExceptionCode::IllegalDataAddress))),
        (Some(function), Some(3)) => Ok(Some((function, ExceptionCode::IllegalDataValue))),
        (Some(function), Some(4)) => Ok(Some((function, ExceptionCode::ServerDeviceFailure))),
        (Some(_), Some(other)) => Err(anyhow::anyhow!(
            "exception code must be 2, 3, or 4 (got {other})"
        )),
        (Some(_), None) => Err(anyhow::anyhow!(
            "--exception-function requires --exception-code"
        )),
        (None, Some(_)) => Err(anyhow::anyhow!(
            "--exception-code requires --exception-function"
        )),
    }
}
