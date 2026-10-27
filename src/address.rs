/// One of the four Modbus data tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Area {
    /// Coils / discrete outputs, references `0XXXX`.
    Coil,
    /// Discrete inputs, references `1XXXX`.
    Discrete,
    /// Input registers, references `3XXXX`.
    Input,
    /// Holding registers, references `4XXXX`.
    Holding,
}

impl Area {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Coil => "coil (0XXXX)",
            Self::Discrete => "discrete input (1XXXX)",
            Self::Input => "input register (3XXXX)",
            Self::Holding => "holding register (4XXXX)",
        }
    }

    fn from_prefix(prefix: u8) -> Option<Self> {
        match prefix {
            b'0' => Some(Self::Coil),
            b'1' => Some(Self::Discrete),
            b'3' => Some(Self::Input),
            b'4' => Some(Self::Holding),
            _ => None,
        }
    }
}

/// Parse a PDU address.
///
/// Accepted forms:
/// - decimal PDU address, such as `0` or `100`
/// - hexadecimal PDU address with an `0x` prefix, such as `0x64`
/// - 5-digit area reference `00001`–`09999`, `10001`–`19999`, `30001`–`39999`, `40001`–`49999`
/// - 6-digit area reference `000001`–`065536`, `100001`–`165536`, `300001`–`365536`, `400001`–`465536`
///
/// Area references are 1-based inside the area. PDU addresses are 0-based.
pub fn parse_address(text: &str, area: Area) -> Result<u16, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("address is empty".to_string());
    }
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return parse_hex_u16(hex, text);
    }
    if let Some(pdu) = parse_area_reference(text, area)? {
        return Ok(pdu);
    }
    parse_decimal_u16(text)
}

/// Parse a 16-bit word as decimal or `0x` hexadecimal.
pub fn parse_word(text: &str) -> Result<u16, String> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return parse_hex_u16(hex, text);
    }
    parse_decimal_u16(text)
}

pub fn parse_coil_address(text: &str) -> Result<u16, String> {
    parse_address(text, Area::Coil)
}

pub fn parse_discrete_address(text: &str) -> Result<u16, String> {
    parse_address(text, Area::Discrete)
}

pub fn parse_input_address(text: &str) -> Result<u16, String> {
    parse_address(text, Area::Input)
}

pub fn parse_holding_address(text: &str) -> Result<u16, String> {
    parse_address(text, Area::Holding)
}

/// One `FILE:RECORD:LENGTH` group for function 20.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReadGroup {
    pub file: u16,
    pub record: u16,
    pub length: u16,
}

/// One `FILE:RECORD:v1,v2,...` group for function 21.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileWriteGroup {
    pub file: u16,
    pub record: u16,
    pub values: Vec<u16>,
}

/// Parse a diagnostics sub-function name or a decimal/`0x` number.
pub fn parse_diagnostic_sub(text: &str) -> Result<u16, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "return-query" | "echo" => Ok(0x0000),
        "restart" => Ok(0x0001),
        "diagnostic-register" => Ok(0x0002),
        "ascii-delimiter" => Ok(0x0003),
        "listen-only" => Ok(0x0004),
        "clear-counters" => Ok(0x000A),
        "bus-messages" => Ok(0x000B),
        "bus-comm-errors" => Ok(0x000C),
        "bus-exceptions" => Ok(0x000D),
        "server-messages" => Ok(0x000E),
        "server-no-response" => Ok(0x000F),
        "server-nak" => Ok(0x0010),
        "server-busy" => Ok(0x0011),
        "bus-overrun" => Ok(0x0012),
        "clear-overrun" => Ok(0x0014),
        _ => parse_word(text),
    }
}

/// Parse `FILE:RECORD:LENGTH`.
pub fn parse_file_read_group(text: &str) -> Result<FileReadGroup, String> {
    let [file, record, length] = split_group(text)?;
    let length = parse_word(length)?;
    if length == 0 {
        return Err("file record length must be at least 1".to_string());
    }
    Ok(FileReadGroup {
        file: parse_word(file)?,
        record: parse_word(record)?,
        length,
    })
}

/// Parse `FILE:RECORD:v1,v2,...`.
pub fn parse_file_write_group(text: &str) -> Result<FileWriteGroup, String> {
    let [file, record, values] = split_group(text)?;
    if values.is_empty() {
        return Err(format!("file write group '{text}' has no values"));
    }
    let values = values
        .split(',')
        .map(parse_word)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FileWriteGroup {
        file: parse_word(file)?,
        record: parse_word(record)?,
        values,
    })
}

fn split_group(text: &str) -> Result<[&str; 3], String> {
    let parts: Vec<&str> = text.trim().splitn(3, ':').collect();
    let [file, record, rest] = parts.as_slice() else {
        return Err(format!(
            "invalid file group '{text}': use FILE:RECORD:LENGTH or FILE:RECORD:v1,v2"
        ));
    };
    Ok([file, record, rest])
}

fn parse_area_reference(text: &str, expected: Area) -> Result<Option<u16>, String> {
    if text.len() != 5 && text.len() != 6 {
        return Ok(None);
    }
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(None);
    }
    let Some(reference_area) = Area::from_prefix(text.as_bytes()[0]) else {
        return Ok(None);
    };
    let offset: u32 = text[1..]
        .parse()
        .map_err(|_| format!("invalid address '{text}'"))?;
    let max = if text.len() == 5 { 9999 } else { 65536 };
    if offset == 0 || offset > max {
        return Ok(None);
    }
    if reference_area != expected {
        return Err(format!(
            "address {text} is a {} reference; this command uses {}",
            reference_area.name(),
            expected.name()
        ));
    }
    Ok(Some((offset - 1) as u16))
}

fn parse_decimal_u16(text: &str) -> Result<u16, String> {
    if !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "invalid number '{text}': use a decimal value or a 0x hexadecimal value"
        ));
    }
    text.parse::<u16>()
        .map_err(|_| format!("invalid number '{text}': must fit in 0-65535"))
}

fn parse_hex_u16(hex: &str, original: &str) -> Result<u16, String> {
    if hex.is_empty() || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(format!("invalid hexadecimal address '{original}'"));
    }
    u16::from_str_radix(hex, 16).map_err(|_| format!("invalid hexadecimal address '{original}'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_and_hex_are_pdu_addresses() {
        assert_eq!(parse_address("0", Area::Holding).unwrap(), 0);
        assert_eq!(parse_address("100", Area::Holding).unwrap(), 100);
        assert_eq!(parse_address("0x10", Area::Coil).unwrap(), 16);
        assert_eq!(parse_address("0X00FF", Area::Input).unwrap(), 255);
        assert_eq!(parse_word("0x2A").unwrap(), 42);
    }

    #[test]
    fn five_and_six_digit_references_are_one_based() {
        assert_eq!(parse_address("00001", Area::Coil).unwrap(), 0);
        assert_eq!(parse_address("10001", Area::Discrete).unwrap(), 0);
        assert_eq!(parse_address("30001", Area::Input).unwrap(), 0);
        assert_eq!(parse_address("40001", Area::Holding).unwrap(), 0);
        assert_eq!(parse_address("40100", Area::Holding).unwrap(), 99);
        assert_eq!(parse_address("400001", Area::Holding).unwrap(), 0);
        assert_eq!(parse_address("465536", Area::Holding).unwrap(), 65535);
    }

    #[test]
    fn reference_for_another_area_is_rejected() {
        let error = parse_address("40001", Area::Coil).unwrap_err();
        assert!(error.contains("holding register"));
        assert!(error.contains("coil"));
    }

    #[test]
    fn zero_offset_falls_through_to_a_decimal_pdu() {
        assert_eq!(parse_address("40000", Area::Holding).unwrap(), 40000);
        assert_eq!(parse_address("10000", Area::Discrete).unwrap(), 10000);
    }

    #[test]
    fn file_groups_and_diagnostic_names() {
        let read = parse_file_read_group("4:0x0001:2").unwrap();
        assert_eq!(read.file, 4);
        assert_eq!(read.record, 1);
        assert_eq!(read.length, 2);
        let write = parse_file_write_group("4:7:0x1234,2").unwrap();
        assert_eq!(write.values, vec![0x1234, 2]);
        assert_eq!(parse_diagnostic_sub("listen-only").unwrap(), 4);
        assert_eq!(parse_diagnostic_sub("0x0A").unwrap(), 10);
    }
}
