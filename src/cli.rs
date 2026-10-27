use clap::{Parser, Subcommand, ValueEnum};
use std::{net::IpAddr, path::PathBuf};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Parity {
    None,
    Even,
    Odd,
}

impl From<Parity> for tokio_serial::Parity {
    fn from(parity: Parity) -> Self {
        match parity {
            Parity::None => tokio_serial::Parity::None,
            Parity::Even => tokio_serial::Parity::Even,
            Parity::Odd => tokio_serial::Parity::Odd,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StopBits {
    #[value(name = "1")]
    One,
    #[value(name = "2")]
    Two,
}

impl From<StopBits> for tokio_serial::StopBits {
    fn from(stop_bits: StopBits) -> Self {
        match stop_bits {
            StopBits::One => tokio_serial::StopBits::One,
            StopBits::Two => tokio_serial::StopBits::Two,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DataBits {
    #[value(name = "5")]
    Five,
    #[value(name = "6")]
    Six,
    #[value(name = "7")]
    Seven,
    #[value(name = "8")]
    Eight,
}

impl From<DataBits> for tokio_serial::DataBits {
    fn from(data_bits: DataBits) -> Self {
        match data_bits {
            DataBits::Five => tokio_serial::DataBits::Five,
            DataBits::Six => tokio_serial::DataBits::Six,
            DataBits::Seven => tokio_serial::DataBits::Seven,
            DataBits::Eight => tokio_serial::DataBits::Eight,
        }
    }
}

// Custom validation functions for Modbus specification limits
fn validate_coil_qty(s: &str) -> Result<u16, String> {
    let qty: u16 = s
        .parse()
        .map_err(|_| format!("Invalid quantity '{s}': must be a number"))?;

    if !(1..=2000).contains(&qty) {
        Err(format!(
            "Invalid quantity {qty}: Modbus specification limits coil and discrete reads to 1-2000 per request (FC 01/02)"
        ))
    } else {
        Ok(qty)
    }
}

fn validate_register_qty(s: &str) -> Result<u16, String> {
    let qty: u16 = s
        .parse()
        .map_err(|_| format!("Invalid quantity '{s}': must be a number"))?;

    if !(1..=125).contains(&qty) {
        Err(format!(
            "Invalid quantity {qty}: Modbus specification limits register operations to 1-125 registers per request (FC 03/04/06/16)"
        ))
    } else {
        Ok(qty)
    }
}

/// Flags common to every subcommand
#[derive(Debug, clap::Args)]
pub struct Common {
    /// Modbus TCP server IP address (TCP only)
    #[arg(long, value_parser = clap::value_parser!(IpAddr), conflicts_with = "device", display_order = 1)]
    pub ip: Option<IpAddr>,

    /// Modbus TCP server port (TCP only)
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

    /// Modbus station address. `--addr` is the same flag.
    #[arg(
        long = "modbus_address",
        visible_alias = "addr",
        value_name = "ADDRESS",
        default_value_t = 0,
        display_order = 8
    )]
    pub unit: u8,

    /// Verbose output
    #[arg(long, short, display_order = 10)]
    pub verbose: bool,
}

/// CLI entry point
#[derive(Parser, Debug)]
#[command(
    name = "mb",
    author,
    version,
    about = "Modbus TCP client and server",
    after_help = "EXAMPLES:\n    mb read holding --ip 127.0.0.1 --port 502 --unit 1\n    mb read holding --ip 127.0.0.1 --unit 40001 --addr 1\n    mb read coil --ip 192.168.1.100 --unit 0x10 --qty 8\n    mb write holding --ip 127.0.0.1 --unit 100 --value 0x2A\n    mb write coil --ip 127.0.0.1 --unit 0 --value 1,0,1,1\n    mb server --ip 0.0.0.0 --port 502"
)]
pub struct Cli {
    #[command(subcommand)]
    pub cmd: Command,
}

/// Read Device Identification access type (FC 43 / MEI 14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DeviceIdCode {
    /// Basic stream, objects 0x00-0x02.
    Basic,
    /// Regular stream, objects 0x00-0x06.
    Regular,
    /// Extended stream. This server answers at its regular conformity level.
    Extended,
    /// One object selected with `--object`.
    Specific,
}

/// How to interpret registers after the raw value table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum DecodeFormat {
    /// 16-bit binary.
    #[value(name = "bin")]
    Bin,
    /// 16-bit hexadecimal.
    #[value(name = "hex")]
    Hex,
    /// Two ANSI characters per register, high byte first.
    #[value(name = "ansi")]
    Ansi,
    /// Unsigned 16-bit integer.
    #[value(name = "u16")]
    U16,
    /// Signed 16-bit integer.
    #[value(name = "i16")]
    I16,
    /// Unsigned 32-bit integer. Uses two registers.
    #[value(name = "u32")]
    U32,
    /// Signed 32-bit integer. Uses two registers.
    #[value(name = "i32")]
    I32,
    /// Unsigned 64-bit integer. Uses four registers.
    #[value(name = "u64")]
    U64,
    /// Signed 64-bit integer. Uses four registers.
    #[value(name = "i64")]
    I64,
    /// IEEE 754 binary32 decimal. Uses two registers. The sign bit is part of the format.
    #[value(name = "f32")]
    F32,
    /// IEEE 754 binary64 decimal. Uses four registers. The sign bit is part of the format.
    #[value(name = "f64")]
    F64,
}

impl DecodeFormat {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bin => "bin",
            Self::Hex => "hex",
            Self::Ansi => "ansi",
            Self::U16 => "u16",
            Self::I16 => "i16",
            Self::U32 => "u32",
            Self::I32 => "i32",
            Self::U64 => "u64",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
        }
    }
}

/// Order of registers inside a 32-bit or 64-bit value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum RegisterOrder {
    /// Most significant register first. The first register holds the high bits.
    #[default]
    #[value(name = "msrf")]
    Msrf,
    /// Least significant register first. The registers are reversed.
    #[value(name = "lsrf")]
    Lsrf,
}

/// How long a read command keeps requesting.
#[derive(Debug, Clone, clap::Args)]
pub struct ReadTiming {
    /// Seconds to keep reading. Omit to read once. A positive value repeats until that many seconds have passed. `0` or a negative value repeats until interrupted. Each sample replaces the previous one.
    #[arg(
        long,
        value_name = "SECONDS",
        allow_hyphen_values = true,
        display_order = 9
    )]
    pub timeout: Option<i64>,
}

/// Options that reinterpret a register read.
#[derive(Debug, Clone, clap::Args)]
pub struct DecodeArgs {
    /// Replace the raw register table with this interpretation. Repeat to show more than one format.
    #[arg(long = "decode", value_enum)]
    pub formats: Vec<DecodeFormat>,

    /// Swap the two bytes inside every 16-bit register before decoding.
    #[arg(long = "swap-bytes")]
    pub swap_bytes: bool,

    /// Register order for 32-bit and 64-bit values.
    #[arg(long = "order", value_enum, default_value = "msrf")]
    pub order: RegisterOrder,
}

impl DeviceIdCode {
    pub const fn read_code(self) -> tokio_modbus::prelude::ReadCode {
        match self {
            Self::Basic => tokio_modbus::prelude::ReadCode::Basic,
            Self::Regular => tokio_modbus::prelude::ReadCode::Regular,
            Self::Extended => tokio_modbus::prelude::ReadCode::Extended,
            Self::Specific => tokio_modbus::prelude::ReadCode::Specific,
        }
    }
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Read coils, discrete inputs, registers, files, or device data
    Read {
        #[command(subcommand)]
        area: ReadArea,
    },

    /// Write coils, holding registers, masks, or file records
    Write {
        #[command(subcommand)]
        area: WriteArea,
    },

    /// Diagnostics (FC 08). Defined for serial lines; this server also answers it on TCP.
    Diagnostics {
        /// Sub-function name or number. See `mb diagnostics --help`.
        #[arg(long, value_parser = crate::address::parse_diagnostic_sub)]
        sub: u16,
        /// Data word, decimal or 0x hex. Restart uses 0xFF00 to clear counters.
        #[arg(long, default_value = "0", value_parser = crate::address::parse_word)]
        data: u16,
        #[command(flatten)]
        common: Common,
    },

    /// Write holding registers, then read holding registers (FC 23)
    ReadWrite {
        /// Register to read: decimal PDU, 0x hex, or a 4XXXX holding reference
        #[arg(long = "read-unit", value_parser = crate::address::parse_holding_address)]
        read_addr: u16,
        /// Read quantity (1-125)
        #[arg(long = "read-qty", default_value_t = 1, value_parser = validate_register_qty)]
        read_qty: u16,
        /// Register to write: decimal PDU, 0x hex, or a 4XXXX holding reference
        #[arg(long = "unit", value_parser = crate::address::parse_holding_address)]
        start: u16,
        /// Values to write before the read (1-121 words, decimal or 0x hex)
        #[arg(
            long = "value",
            value_delimiter = ',',
            num_args = 1..,
            required = true,
            value_parser = crate::address::parse_word
        )]
        values: Vec<u16>,
        #[command(flatten)]
        decode: DecodeArgs,
        #[command(flatten)]
        common: Common,
    },

    /// Run a Modbus server
    Server {
        /// IP address to bind to (TCP only)
        #[arg(long, value_parser = clap::value_parser!(IpAddr), conflicts_with = "device", display_order = 1)]
        ip: Option<IpAddr>,

        /// Port to listen on (TCP only)
        #[arg(long, default_value_t = 502, display_order = 2)]
        port: u16,

        /// Serial device path (RTU only)
        #[arg(long, conflicts_with = "ip", display_order = 3)]
        device: Option<PathBuf>,

        /// Baud rate for serial communication (RTU only)
        #[arg(long, default_value_t = 9600, display_order = 4)]
        baud: u32,

        /// Parity for serial communication (RTU only)
        #[arg(long, value_enum, default_value = "none", display_order = 5)]
        parity: Parity,

        /// Stop bits for serial communication (RTU only)
        #[arg(long, value_enum, default_value = "1", display_order = 6)]
        stop_bits: StopBits,

        /// Data bits for serial communication (RTU only)
        #[arg(long, value_enum, default_value = "8", display_order = 7)]
        data_bits: DataBits,

        /// Modbus station address. The server answers only this id. `--addr` is the same flag.
        #[arg(
            long = "modbus_address",
            visible_alias = "addr",
            value_name = "ADDRESS",
            default_value_t = 1,
            display_order = 8
        )]
        unit: u8,

        /// Timeout for binding, opening the serial port, and idle TCP connections, in seconds
        #[arg(long, default_value_t = 5, display_order = 9)]
        timeout: u64,

        /// Number of coils (0-65535)
        #[arg(long, default_value_t = 10000, display_order = 10)]
        num_coils: u16,

        /// Number of discrete inputs (0-65535)
        #[arg(long, default_value_t = 10000, display_order = 11)]
        num_discrete: u16,

        /// Number of holding registers (0-65535)
        #[arg(long, default_value_t = 10000, display_order = 12)]
        num_holding: u16,

        /// Number of input registers (0-65535)
        #[arg(long, default_value_t = 10000, display_order = 13)]
        num_input: u16,

        /// Verbose logging
        #[arg(long, short, display_order = 14)]
        verbose: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReadArea {
    /// Read coils / discrete outputs (FC 01). Area 0XXXX, read/write bits.
    #[command(visible_alias = "discrete-output", visible_alias = "0xxxx")]
    Coil {
        /// PDU address (decimal or 0x hex) or a 0XXXX reference. 00001 is PDU 0.
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_coil_address)]
        start: u16,
        /// Quantity (default 1, max 2000)
        #[arg(long = "qty", default_value_t = 1, value_parser = validate_coil_qty, display_order = 6)]
        qty: u16,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
    /// Read discrete inputs (FC 02). Area 1XXXX, read-only bits.
    #[command(visible_alias = "discrete-input", visible_alias = "1xxxx")]
    Discrete {
        /// PDU address (decimal or 0x hex) or a 1XXXX reference. 10001 is PDU 0.
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_discrete_address)]
        start: u16,
        /// Quantity (default 1, max 2000)
        #[arg(long = "qty", default_value_t = 1, value_parser = validate_coil_qty, display_order = 6)]
        qty: u16,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
    /// Read holding registers (FC 03). Area 4XXXX, read/write 16-bit registers.
    #[command(visible_alias = "holding-register", visible_alias = "4xxxx")]
    Holding {
        /// PDU address (decimal or 0x hex) or a 4XXXX reference. 40001 is PDU 0.
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_holding_address)]
        start: u16,
        /// Quantity (default 1, max 125)
        #[arg(long = "qty", default_value_t = 1, value_parser = validate_register_qty, display_order = 6)]
        qty: u16,
        #[command(flatten)]
        decode: DecodeArgs,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
    /// Read input registers (FC 04). Area 3XXXX, read-only 16-bit registers.
    #[command(visible_alias = "input-register", visible_alias = "3xxxx")]
    Input {
        /// PDU address (decimal or 0x hex) or a 3XXXX reference. 30001 is PDU 0.
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_input_address)]
        start: u16,
        /// Quantity (default 1, max 125)
        #[arg(long = "qty", default_value_t = 1, value_parser = validate_register_qty, display_order = 6)]
        qty: u16,
        #[command(flatten)]
        decode: DecodeArgs,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
    /// Read exception status (FC 07). Serial-line function; also answered on TCP.
    ExceptionStatus {
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
    /// Get the communication event counter (FC 11)
    EventCounter {
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
    /// Get the communication event log (FC 12)
    EventLog {
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
    /// Report the server id (FC 17)
    ServerId {
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
    /// Read a FIFO queue (FC 24). The holding register at `--unit` holds the count.
    Fifo {
        /// FIFO pointer: decimal PDU, 0x hex, or a 4XXXX holding reference
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_holding_address)]
        start: u16,
        #[command(flatten)]
        decode: DecodeArgs,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
    /// Read file records (FC 20)
    File {
        /// Repeat for each group: FILE:RECORD:LENGTH
        #[arg(
            long = "group",
            value_name = "FILE:RECORD:LENGTH",
            num_args = 1..,
            required = true,
            value_parser = crate::address::parse_file_read_group
        )]
        groups: Vec<crate::address::FileReadGroup>,
        #[command(flatten)]
        decode: DecodeArgs,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
    /// Read device identification (FC 43/14). CANopen (FC 43/13) is not implemented.
    DeviceId {
        /// Identification stream: basic, regular, extended, or specific
        #[arg(long, value_enum, default_value_t = DeviceIdCode::Basic)]
        code: DeviceIdCode,
        /// Object id for `--code specific`, or the first object of a stream
        #[arg(long, default_value_t = 0)]
        object: u8,
        #[command(flatten)]
        common: Common,
        #[command(flatten)]
        timing: ReadTiming,
    },
}

#[derive(Subcommand, Debug)]
pub enum WriteArea {
    /// Write one coil (FC 05) or several coils (FC 15). Area 0XXXX.
    #[command(visible_alias = "discrete-output", visible_alias = "0xxxx")]
    Coil {
        /// PDU address (decimal or 0x hex) or a 0XXXX reference. 00001 is PDU 0.
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_coil_address)]
        start: u16,
        /// Value(s): 0=OFF, non-zero=ON. Decimal or 0x hex, comma-separated.
        #[arg(
            long = "value",
            value_delimiter = ',',
            num_args = 1..,
            required = true,
            value_parser = crate::address::parse_word
        )]
        values: Vec<u16>,
        #[command(flatten)]
        common: Common,
    },
    /// Write one holding register (FC 06) or several (FC 16). Area 4XXXX.
    #[command(visible_alias = "holding-register", visible_alias = "4xxxx")]
    Holding {
        /// PDU address (decimal or 0x hex) or a 4XXXX reference. 40001 is PDU 0.
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_holding_address)]
        start: u16,
        /// Value(s), decimal or 0x hex, comma-separated. At most 123 words.
        #[arg(
            long = "value",
            value_delimiter = ',',
            num_args = 1..,
            required = true,
            value_parser = crate::address::parse_word
        )]
        values: Vec<u16>,
        #[command(flatten)]
        common: Common,
    },
    /// Mask-write one holding register (FC 22): `(current AND and) OR (or AND NOT and)`
    Mask {
        /// PDU address (decimal or 0x hex) or a 4XXXX reference
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_holding_address)]
        start: u16,
        /// AND mask, decimal or 0x hex
        #[arg(long = "and", value_parser = crate::address::parse_word)]
        and_mask: u16,
        /// OR mask, decimal or 0x hex
        #[arg(long = "or", value_parser = crate::address::parse_word)]
        or_mask: u16,
        #[command(flatten)]
        common: Common,
    },
    /// Write file records (FC 21)
    File {
        /// Repeat for each group: FILE:RECORD:v1,v2,...
        #[arg(
            long = "group",
            value_name = "FILE:RECORD:VALUES",
            num_args = 1..,
            required = true,
            value_parser = crate::address::parse_file_write_group
        )]
        groups: Vec<crate::address::FileWriteGroup>,
        #[command(flatten)]
        common: Common,
    },
    /// Discrete inputs are read-only (FC 02, area 1XXXX)
    #[command(visible_alias = "discrete-input", visible_alias = "1xxxx")]
    Discrete {
        #[command(flatten)]
        common: Common,
    },
    /// Input registers are read-only (FC 04, area 3XXXX)
    #[command(visible_alias = "input-register", visible_alias = "3xxxx")]
    Input {
        #[command(flatten)]
        common: Common,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_coil_qty_valid() {
        assert_eq!(validate_coil_qty("1"), Ok(1));
        assert_eq!(validate_coil_qty("1000"), Ok(1000));
        assert_eq!(validate_coil_qty("2000"), Ok(2000));
    }

    #[test]
    fn test_validate_coil_qty_invalid() {
        assert!(validate_coil_qty("0").is_err());
        assert!(validate_coil_qty("2001").is_err());
        assert!(validate_coil_qty("abc").is_err());
        assert!(validate_coil_qty("").is_err());
    }

    #[test]
    fn test_validate_coil_qty_error_messages() {
        let result = validate_coil_qty("0");
        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("1-2000"));
        assert!(error_msg.contains("FC 01/02"));

        let result = validate_coil_qty("abc");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be a number"));
    }

    #[test]
    fn test_validate_register_qty_valid() {
        assert_eq!(validate_register_qty("1"), Ok(1));
        assert_eq!(validate_register_qty("100"), Ok(100));
        assert_eq!(validate_register_qty("125"), Ok(125));
    }

    #[test]
    fn test_validate_register_qty_invalid() {
        assert!(validate_register_qty("0").is_err());
        assert!(validate_register_qty("126").is_err());
        assert!(validate_register_qty("xyz").is_err());
        assert!(validate_register_qty("").is_err());
    }

    #[test]
    fn test_validate_register_qty_error_messages() {
        let result = validate_register_qty("0");
        assert!(result.is_err());
        let error_msg = result.unwrap_err();
        assert!(error_msg.contains("1-125"));
        assert!(error_msg.contains("FC 03/04/06/16"));

        let result = validate_register_qty("xyz");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be a number"));
    }

    #[test]
    fn test_help_examples_use_real_commands() {
        use clap::CommandFactory;

        let mut command = Cli::command();
        let mut help = Vec::new();
        command.write_long_help(&mut help).unwrap();
        let help = String::from_utf8(help).unwrap();

        assert!(help.contains("mb read coil "));
        assert!(!help.contains("mb read coils"));
        assert!(help.contains("mb write coil "));
        assert!(!help.contains("mb write coils"));
    }

    #[test]
    fn test_read_area_names_are_singular() {
        assert!(
            Cli::try_parse_from(["mb", "read", "coils", "--ip", "127.0.0.1", "--unit", "0"])
                .is_err()
        );
        assert!(
            Cli::try_parse_from(["mb", "read", "coil", "--ip", "127.0.0.1", "--unit", "0"]).is_ok()
        );
    }

    #[test]
    fn test_server_parses_unit_timeout_and_verbose() {
        let cli = Cli::try_parse_from([
            "mb",
            "server",
            "--ip",
            "127.0.0.1",
            "--modbus_address",
            "7",
            "--timeout",
            "12",
            "-v",
        ])
        .unwrap();

        match cli.cmd {
            Command::Server {
                unit,
                timeout,
                verbose,
                ip,
                device,
                ..
            } => {
                assert_eq!(unit, 7);
                assert_eq!(timeout, 12);
                assert!(verbose);
                assert_eq!(ip.unwrap().to_string(), "127.0.0.1");
                assert!(device.is_none());
            }
            other => panic!("expected server command, got {other:?}"),
        }
    }

    #[test]
    fn addresses_accept_hex_and_area_references() {
        let cli = Cli::try_parse_from([
            "mb",
            "read",
            "holding-register",
            "--ip",
            "127.0.0.1",
            "--unit",
            "40100",
        ])
        .unwrap();
        match cli.cmd {
            Command::Read {
                area: ReadArea::Holding { start, .. },
            } => assert_eq!(start, 99),
            other => panic!("expected holding read, got {other:?}"),
        }

        let cli =
            Cli::try_parse_from(["mb", "read", "0xxxx", "--ip", "127.0.0.1", "--unit", "0x10"])
                .unwrap();
        match cli.cmd {
            Command::Read {
                area: ReadArea::Coil { start, .. },
            } => assert_eq!(start, 16),
            other => panic!("expected coil read, got {other:?}"),
        }

        assert!(Cli::try_parse_from([
            "mb",
            "read",
            "coil",
            "--ip",
            "127.0.0.1",
            "--unit",
            "40001",
        ])
        .is_err());
        assert!(Cli::try_parse_from(["mb", "write", "discrete"]).is_ok());
        assert!(Cli::try_parse_from(["mb", "write", "input-register"]).is_ok());
    }

    #[test]
    fn decode_flags_select_format_order_and_byte_swap() {
        let cli = Cli::try_parse_from([
            "mb",
            "read",
            "input",
            "--ip",
            "127.0.0.1",
            "--modbus_address",
            "3",
            "--unit",
            "0",
            "--qty",
            "4",
            "--decode",
            "i32",
            "--decode",
            "f32",
            "--order",
            "lsrf",
            "--swap-bytes",
        ])
        .unwrap();
        match cli.cmd {
            Command::Read {
                area:
                    ReadArea::Input {
                        decode,
                        qty,
                        common,
                        ..
                    },
            } => {
                assert_eq!(qty, 4);
                assert_eq!(common.unit, 3);
                assert_eq!(decode.formats, vec![DecodeFormat::I32, DecodeFormat::F32]);
                assert_eq!(decode.order, RegisterOrder::Lsrf);
                assert!(decode.swap_bytes);
            }
            other => panic!("expected input read, got {other:?}"),
        }
    }

    fn read_timeout(args: &[&str]) -> Option<i64> {
        let mut command = vec!["mb", "read", "holding", "--ip", "127.0.0.1", "--unit", "0"];
        command.extend_from_slice(args);
        let cli = Cli::try_parse_from(command).unwrap();
        match cli.cmd {
            Command::Read {
                area: ReadArea::Holding { timing, .. },
            } => timing.timeout,
            other => panic!("expected holding read, got {other:?}"),
        }
    }

    #[test]
    fn read_timeout_is_seconds_and_defaults_to_one_read() {
        assert_eq!(read_timeout(&[]), None);
        assert_eq!(read_timeout(&["--timeout", "10"]), Some(10));
        assert_eq!(read_timeout(&["--timeout", "0"]), Some(0));
        assert_eq!(read_timeout(&["--timeout", "-1"]), Some(-1));
    }
}
