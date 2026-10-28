//! Register and coil tables.

use crate::table::{AddressStyle, format_address, print_register_table, write_register_table};

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
