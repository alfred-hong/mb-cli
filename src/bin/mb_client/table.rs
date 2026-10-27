use std::fmt::Write as _;

/// How a PDU address is shown. `Modicon` uses the 0/1/3/4 area reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AddressStyle {
    /// The numeric protocol address, starting at 0.
    #[default]
    Pdu,
    /// Five digits through offset 9999, then six digits. `prefix` is 0, 1, 3, or 4.
    Modicon(u8),
}

/// Format one PDU address for a table cell.
pub fn format_address(style: AddressStyle, pdu: u32) -> String {
    let Ok(point) = u16::try_from(pdu) else {
        return pdu.to_string();
    };
    match style {
        AddressStyle::Pdu => point.to_string(),
        AddressStyle::Modicon(prefix) => {
            let offset = u32::from(point) + 1;
            if offset <= 9_999 {
                format!("{:05}", u32::from(prefix) * 10_000 + offset)
            } else {
                format!("{:06}", u32::from(prefix) * 100_000 + offset)
            }
        }
    }
}

/// Write a table header into `out`.
pub fn write_table_header(out: &mut String, columns: &[&str]) {
    for (i, col) in columns.iter().enumerate() {
        if i == 0 {
            let _ = write!(out, "{col:<8}");
        } else {
            let _ = write!(out, " {col:<6}");
        }
    }
    out.push('\n');

    for (i, _) in columns.iter().enumerate() {
        if i == 0 {
            let _ = write!(out, "{:-<8}", "");
        } else {
            let _ = write!(out, " {:-<6}", "");
        }
    }
    out.push('\n');
}

pub fn write_register_table(
    out: &mut String,
    registers: &[u16],
    start_addr: u16,
    verbose: bool,
    style: AddressStyle,
) {
    if registers.is_empty() {
        return;
    }

    if verbose {
        write_table_header(out, &["Address", "Value", "Hex"]);
    } else {
        write_table_header(out, &["Address", "Value"]);
    }

    for (i, &value) in registers.iter().enumerate() {
        let addr = format_address(style, u32::from(start_addr) + i as u32);
        if verbose {
            let _ = writeln!(out, "{addr:<8} {value:<6} 0x{value:04X}");
        } else {
            let _ = writeln!(out, "{addr:<8} {value:<6}");
        }
    }
}

pub fn write_coil_table(out: &mut String, coils: &[bool], start_addr: u16, style: AddressStyle) {
    if coils.is_empty() {
        return;
    }

    write_table_header(out, &["Address", "Value"]);

    for (i, &value) in coils.iter().enumerate() {
        let addr = format_address(style, u32::from(start_addr) + i as u32);
        let _ = writeln!(out, "{addr:<8} {:<6}", if value { "ON" } else { "OFF" });
    }
}

pub fn print_register_table(
    registers: &[u16],
    start_addr: u16,
    verbose: bool,
    style: AddressStyle,
) {
    let mut out = String::new();
    write_register_table(&mut out, registers, start_addr, verbose, style);
    print!("{out}");
}

pub fn print_coil_table(coils: &[bool], start_addr: u16, style: AddressStyle) {
    let mut out = String::new();
    write_coil_table(&mut out, coils, start_addr, style);
    print!("{out}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_register_table_empty() {
        let registers: &[u16] = &[];
        print_register_table(registers, 0, false, AddressStyle::Pdu);
        print_register_table(registers, 0, true, AddressStyle::Pdu);
    }

    #[test]
    fn test_print_register_table_single() {
        let registers = [42u16];
        print_register_table(&registers, 100, false, AddressStyle::Pdu);
        print_register_table(&registers, 100, true, AddressStyle::Pdu);
    }

    #[test]
    fn test_print_register_table_multiple() {
        let registers = [1u16, 2, 3];
        print_register_table(&registers, 0, false, AddressStyle::Pdu);
        print_register_table(&registers, 1000, true, AddressStyle::Pdu);
    }

    #[test]
    fn write_register_table_builds_one_sample() {
        let mut out = String::new();
        write_register_table(&mut out, &[7, 8], 0, false, AddressStyle::Pdu);
        assert!(out.contains("Address"));
        assert!(out.contains("0        7"));
        assert!(out.contains("1        8"));
        assert_eq!(out.lines().count(), 4);
    }

    #[test]
    fn modicon_addresses_use_five_digits_then_six() {
        assert_eq!(format_address(AddressStyle::Pdu, 0), "0");
        assert_eq!(format_address(AddressStyle::Modicon(4), 0), "40001");
        assert_eq!(format_address(AddressStyle::Modicon(4), 1), "40002");
        assert_eq!(format_address(AddressStyle::Modicon(3), 0), "30001");
        assert_eq!(format_address(AddressStyle::Modicon(1), 0), "10001");
        assert_eq!(format_address(AddressStyle::Modicon(0), 0), "00001");
        assert_eq!(format_address(AddressStyle::Modicon(4), 9998), "49999");
        assert_eq!(format_address(AddressStyle::Modicon(4), 9999), "410000");
    }
}
