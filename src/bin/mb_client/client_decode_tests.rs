//! Numeric views of register words.

use crate::cli::{DecodeFormat, RegisterOrder};
use crate::decode::{apply_byte_swap, decode_rows};

fn row(words: &[u16], format: DecodeFormat, order: RegisterOrder, swap: bool) -> String {
    let prepared = apply_byte_swap(words, swap);
    decode_rows(&prepared, 0, format, order).0[0].value.clone()
}

#[test]
fn sixteen_bit_views_and_byte_swap() {
    assert_eq!(
        row(&[0x4142], DecodeFormat::Ansi, RegisterOrder::Msrf, false),
        "AB"
    );
    assert_eq!(
        row(&[0x4142], DecodeFormat::Ansi, RegisterOrder::Msrf, true),
        "BA"
    );
    assert_eq!(
        row(&[0x00FF], DecodeFormat::Hex, RegisterOrder::Msrf, true),
        "0xFF00"
    );
    assert_eq!(
        row(&[0x0001], DecodeFormat::Bin, RegisterOrder::Msrf, false),
        "0000_0000_0000_0001"
    );
    assert_eq!(
        row(&[0xFFFF], DecodeFormat::I16, RegisterOrder::Msrf, false),
        "-1"
    );
    assert_eq!(
        row(&[0xFFFF], DecodeFormat::U16, RegisterOrder::Msrf, false),
        "65535"
    );
}

#[test]
fn register_order_and_byte_swap_combine() {
    let words = [0x1234, 0x5678];
    assert_eq!(
        row(&words, DecodeFormat::U32, RegisterOrder::Msrf, false),
        "305419896"
    );
    assert_eq!(
        row(&words, DecodeFormat::Hex, RegisterOrder::Lsrf, false),
        "0x1234"
    );
    assert_eq!(
        row(&words, DecodeFormat::U32, RegisterOrder::Lsrf, false),
        "1450709556"
    );
    assert_eq!(
        row(&words, DecodeFormat::U32, RegisterOrder::Msrf, true),
        "873625686"
    );
    assert_eq!(
        row(&words, DecodeFormat::U32, RegisterOrder::Lsrf, true),
        "2018915346"
    );
    assert_eq!(
        row(
            &[0xFFFF, 0xFFFF],
            DecodeFormat::I32,
            RegisterOrder::Msrf,
            false
        ),
        "-1"
    );
}

#[test]
fn floats_use_ieee754() {
    let pi = row(
        &[0x4049, 0x0FDB],
        DecodeFormat::F32,
        RegisterOrder::Msrf,
        false,
    );
    assert!(pi.starts_with("3.14159"), "{pi}");
    let (rows, leftover) = decode_rows(&[0x4049], 10, DecodeFormat::F32, RegisterOrder::Msrf);
    assert!(rows.is_empty());
    assert_eq!(leftover, 1);
    let swapped = row(
        &[0x1234, 0x5678, 0x9ABC, 0xDEF0],
        DecodeFormat::U64,
        RegisterOrder::Lsrf,
        true,
    );
    let expected = u64::from_be_bytes([0xF0, 0xDE, 0xBC, 0x9A, 0x78, 0x56, 0x34, 0x12]);
    assert_eq!(swapped, expected.to_string());
}
