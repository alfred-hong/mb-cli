use std::fmt::Write as _;

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

pub fn write_register_table(out: &mut String, registers: &[u16], start_addr: u16, verbose: bool) {
    if registers.is_empty() {
        return;
    }

    if verbose {
        write_table_header(out, &["Address", "Value", "Hex"]);
    } else {
        write_table_header(out, &["Address", "Value"]);
    }

    for (i, &value) in registers.iter().enumerate() {
        let addr = start_addr + i as u16;
        if verbose {
            let _ = writeln!(out, "{addr:<8} {value:<6} 0x{value:04X}");
        } else {
            let _ = writeln!(out, "{addr:<8} {value:<6}");
        }
    }
}

pub fn write_coil_table(out: &mut String, coils: &[bool], start_addr: u16) {
    if coils.is_empty() {
        return;
    }

    write_table_header(out, &["Address", "Value"]);

    for (i, &value) in coils.iter().enumerate() {
        let addr = start_addr + i as u16;
        let _ = writeln!(out, "{:<8} {:<6}", addr, if value { "ON" } else { "OFF" });
    }
}

pub fn print_register_table(registers: &[u16], start_addr: u16, verbose: bool) {
    let mut out = String::new();
    write_register_table(&mut out, registers, start_addr, verbose);
    print!("{out}");
}

pub fn print_coil_table(coils: &[bool], start_addr: u16) {
    let mut out = String::new();
    write_coil_table(&mut out, coils, start_addr);
    print!("{out}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_register_table_empty() {
        let registers: &[u16] = &[];
        print_register_table(registers, 0, false);
        print_register_table(registers, 0, true);
    }

    #[test]
    fn test_print_register_table_single() {
        let registers = [42u16];
        print_register_table(&registers, 100, false);
        print_register_table(&registers, 100, true);
    }

    #[test]
    fn test_print_register_table_multiple() {
        let registers = [1u16, 2, 3];
        print_register_table(&registers, 0, false);
        print_register_table(&registers, 1000, true);
    }

    #[test]
    fn write_register_table_builds_one_sample() {
        let mut out = String::new();
        write_register_table(&mut out, &[7, 8], 0, false);
        assert!(out.contains("Address"));
        assert!(out.contains("0        7"));
        assert!(out.contains("1        8"));
        assert_eq!(out.lines().count(), 4);
    }
}
