use crate::serial::{DataBits, Parity, StopBits};
use clap::{Parser, Subcommand, ValueEnum};
use std::{net::IpAddr, path::PathBuf};

// Custom validation functions for Modbus specification limits
pub fn validate_coil_qty(s: &str) -> Result<u16, String> {
    parse_quantity(s, 1, 2000, "FC 01/02")
}

pub fn validate_register_qty(s: &str) -> Result<u16, String> {
    parse_quantity(s, 1, 125, "FC 03/04/23")
}

/// Decimal quantity inside an inclusive Modbus limit.
fn parse_quantity(text: &str, min: u16, max: u16, function: &str) -> Result<u16, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "quantity '{text}' must be a decimal number in {min}-{max} ({function})"
        ));
    }
    let qty: u16 = text.parse().map_err(|_| {
        format!("quantity '{text}' must be a decimal number in {min}-{max} ({function})")
    })?;
    if !(min..=max).contains(&qty) {
        Err(format!(
            "quantity {qty} is outside {min}-{max} ({function})"
        ))
    } else {
        Ok(qty)
    }
}

/// Milliseconds between scan samples. `0` means no extra wait.
fn validate_interval(text: &str) -> Result<u64, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "interval '{text}' must be a decimal number of milliseconds"
        ));
    }
    text.parse()
        .map_err(|_| format!("interval '{text}' must be a decimal number of milliseconds"))
}

/// Client station id. `0` is the broadcast address.
fn parse_station(text: &str) -> Result<u8, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "station address '{text}' must be a decimal number 0-255"
        ));
    }
    text.parse::<u8>()
        .map_err(|_| format!("station address '{text}' must be 0-255"))
}

/// Device-identification object id.
fn parse_object_id(text: &str) -> Result<u8, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("object id '{text}' must be a decimal number 0-255"));
    }
    text.parse::<u8>()
        .map_err(|_| format!("object id '{text}' must be 0-255"))
}

/// Extra attempts after the first try. `0` sends the request once.
fn parse_retries(text: &str) -> Result<u32, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "retries '{text}' must be a decimal number of extra attempts"
        ));
    }
    text.parse()
        .map_err(|_| format!("retries '{text}' must be a decimal number of extra attempts"))
}

fn parse_function_code(text: &str) -> Result<u8, String> {
    let value = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u8::from_str_radix(hex, 16)
    } else {
        text.parse()
    }
    .map_err(|_| format!("function code '{text}' must be 1-127"))?;
    if (1..128).contains(&value) {
        Ok(value)
    } else {
        Err(format!("function code '{text}' must be 1-127"))
    }
}

/// Bytes after a function code. Empty text is an empty payload.
pub fn parse_hex_payload(text: &str) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    for token in text.split(|character: char| character.is_whitespace() || character == ',') {
        if token.is_empty() {
            continue;
        }
        let hex = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
            .unwrap_or(token);
        if hex.is_empty() || hex.len() > 2 {
            anyhow::bail!("PDU byte '{token}' must be one hex byte");
        }
        let value = u8::from_str_radix(hex, 16)
            .map_err(|_| anyhow::anyhow!("PDU byte '{token}' is not hex"))?;
        bytes.push(value);
    }
    Ok(bytes)
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
        value_parser = parse_station,
        allow_hyphen_values = true,
        display_order = 8
    )]
    pub unit: u8,

    /// Seconds to wait for the connection and for each response. This is separate from `--timeout`.
    #[arg(
        long = "response-timeout",
        default_value_t = crate::session::REQUEST_TIMEOUT_SECS,
        value_name = "SECONDS",
        display_order = 9
    )]
    pub response_timeout: u64,

    /// Extra attempts after a timeout or a broken connection. The default is 2, then the command stops.
    /// A Modbus exception is not retried. Station 0 is not retried, because that address gets no response.
    #[arg(
        long,
        default_value_t = crate::session::DEFAULT_RETRIES,
        value_name = "COUNT",
        value_parser = parse_retries,
        allow_hyphen_values = true,
        display_order = 10
    )]
    pub retries: u32,

    /// Address column. `pdu` prints the protocol address. `modicon` prints 00001, 10001, 30001, or 40001.
    #[arg(long, value_enum, default_value = "pdu", display_order = 10)]
    pub notation: AddressNotation,

    /// Verbose output. Also prints each transmitted and received frame in hex.
    #[arg(long, short, display_order = 11)]
    pub verbose: bool,
}

/// How addresses are printed. Input still accepts decimal, `0x`, and area references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Default)]
pub enum AddressNotation {
    /// Protocol address, starting at 0.
    #[default]
    #[value(name = "pdu")]
    Pdu,
    /// Classic area reference: coils 0XXXX, discrete inputs 1XXXX, input registers 3XXXX, holding registers 4XXXX.
    #[value(name = "modicon")]
    Modicon,
}

/// CLI entry point
#[derive(Parser, Debug)]
#[command(
    name = "mb_client",
    author,
    version,
    about = "Modbus TCP and RTU client",
    after_help = "EXAMPLES:\n    mb_client read holding --ip 127.0.0.1 --port 502 --unit 1\n    mb_client read holding --ip 127.0.0.1 --unit 40001 --addr 1\n    mb_client read coil --ip 192.168.1.100 --unit 0x10 --qty 8\n    mb_client write holding --ip 127.0.0.1 --unit 100 --value 0x2A\n    mb_client write coil --ip 127.0.0.1 --unit 0 --value 1,0,1,1"
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
    /// Seconds to keep reading. Omit to read once. A positive value repeats until that many seconds have passed. `0` or a negative value repeats until interrupted. Each sample replaces the previous one. A failed sample stays on screen and the scan continues.
    #[arg(
        long,
        value_name = "SECONDS",
        allow_hyphen_values = true,
        display_order = 12
    )]
    pub timeout: Option<i64>,

    /// Milliseconds from the start of one sample to the start of the next. `0` sends the next request as soon as the previous one finishes.
    #[arg(
        long = "interval",
        default_value_t = 1000,
        value_name = "MILLISECONDS",
        value_parser = validate_interval,
        allow_hyphen_values = true,
        display_order = 13
    )]
    pub interval_ms: u64,

    /// Append one line per sample. An error is written in place of the values.
    #[arg(long, value_name = "FILE", display_order = 14)]
    pub log: Option<PathBuf>,
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
        /// Sub-function name or number. See `mb_client diagnostics --help`.
        #[arg(long, value_parser = crate::address::parse_diagnostic_sub, allow_hyphen_values = true)]
        sub: u16,
        /// Data word, decimal or 0x hex. Restart uses 0xFF00 to clear counters.
        #[arg(long, default_value = "0", value_parser = crate::address::parse_word, allow_hyphen_values = true)]
        data: u16,
        #[command(flatten)]
        common: Common,
    },

    /// Send one function code and print the response. ModScan calls this a user command.
    Raw {
        /// Function code, 1-127.
        #[arg(long, value_parser = parse_function_code, allow_hyphen_values = true)]
        function: u8,
        /// Bytes after the function code. Hex bytes separated by commas or spaces, such as `00,00,00,01`.
        #[arg(long, default_value = "")]
        data: String,
        #[command(flatten)]
        common: Common,
    },

    /// Write holding registers, then read holding registers (FC 23)
    ReadWrite {
        /// Register to read: decimal PDU, 0x hex, or a 4XXXX holding reference
        #[arg(long = "read-unit", value_parser = crate::address::parse_holding_address, allow_hyphen_values = true)]
        read_addr: u16,
        /// Read quantity (1-125)
        #[arg(long = "read-qty", default_value_t = 1, value_parser = validate_register_qty, allow_hyphen_values = true)]
        read_qty: u16,
        /// Register to write: decimal PDU, 0x hex, or a 4XXXX holding reference
        #[arg(long = "unit", value_parser = crate::address::parse_holding_address, allow_hyphen_values = true)]
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

    /// Print a shell completion script to stdout.
    Completions {
        /// bash, elvish, fish, powershell, or zsh
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReadArea {
    /// Read coils / discrete outputs (FC 01). Area 0XXXX, read/write bits.
    #[command(visible_alias = "discrete-output", visible_alias = "0xxxx")]
    Coil {
        /// PDU address (decimal or 0x hex) or a 0XXXX reference. 00001 is PDU 0.
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_coil_address, allow_hyphen_values = true)]
        start: u16,
        /// Quantity (default 1, max 2000)
        #[arg(long = "qty", default_value_t = 1, value_parser = validate_coil_qty, allow_hyphen_values = true, display_order = 6)]
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
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_discrete_address, allow_hyphen_values = true)]
        start: u16,
        /// Quantity (default 1, max 2000)
        #[arg(long = "qty", default_value_t = 1, value_parser = validate_coil_qty, allow_hyphen_values = true, display_order = 6)]
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
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_holding_address, allow_hyphen_values = true)]
        start: u16,
        /// Quantity (default 1, max 125)
        #[arg(long = "qty", default_value_t = 1, value_parser = validate_register_qty, allow_hyphen_values = true, display_order = 6)]
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
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_input_address, allow_hyphen_values = true)]
        start: u16,
        /// Quantity (default 1, max 125)
        #[arg(long = "qty", default_value_t = 1, value_parser = validate_register_qty, allow_hyphen_values = true, display_order = 6)]
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
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_holding_address, allow_hyphen_values = true)]
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
        #[arg(long = "object", default_value_t = 0, value_parser = parse_object_id, allow_hyphen_values = true)]
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
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_coil_address, allow_hyphen_values = true)]
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
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_holding_address, allow_hyphen_values = true)]
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
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_holding_address, allow_hyphen_values = true)]
        start: u16,
        /// AND mask, decimal or 0x hex
        #[arg(long = "and", value_parser = crate::address::parse_word, allow_hyphen_values = true)]
        and_mask: u16,
        /// OR mask, decimal or 0x hex
        #[arg(long = "or", value_parser = crate::address::parse_word, allow_hyphen_values = true)]
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
        /// PDU address or a 1XXXX reference. The write is rejected either way.
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_discrete_address, allow_hyphen_values = true)]
        start: Option<u16>,
        /// Values are not written. Discrete inputs stay read-only.
        #[arg(long = "value")]
        values: Option<String>,
        #[command(flatten)]
        common: Common,
    },
    /// Input registers are read-only (FC 04, area 3XXXX)
    #[command(visible_alias = "input-register", visible_alias = "3xxxx")]
    Input {
        /// PDU address or a 3XXXX reference. The write is rejected either way.
        #[arg(long = "unit", value_name = "UNIT", value_parser = crate::address::parse_input_address, allow_hyphen_values = true)]
        start: Option<u16>,
        /// Values are not written. Input registers stay read-only.
        #[arg(long = "value")]
        values: Option<String>,
        #[command(flatten)]
        common: Common,
    },
}
