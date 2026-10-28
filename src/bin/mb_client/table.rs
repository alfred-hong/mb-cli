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
