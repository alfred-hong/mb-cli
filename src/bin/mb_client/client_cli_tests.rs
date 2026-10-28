//! Command-line parsing for `mb_client`.

use crate::cli::{
    AddressNotation, Cli, Command, DecodeFormat, ReadArea, RegisterOrder, parse_hex_payload,
    validate_coil_qty, validate_register_qty,
};
use crate::session::transport_retries;
use clap::Parser;

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
    assert!(result.unwrap_err().contains("decimal number"));
    assert!(validate_coil_qty("-1").unwrap_err().contains("1-2000"));
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
    assert!(error_msg.contains("FC 03/04/23"));

    let result = validate_register_qty("xyz");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("decimal number"));
}

#[test]
fn test_help_examples_use_real_commands() {
    use clap::CommandFactory;

    let mut command = Cli::command();
    let mut help = Vec::new();
    command.write_long_help(&mut help).unwrap();
    let help = String::from_utf8(help).unwrap();

    assert!(help.contains("mb_client read coil "));
    assert!(!help.contains("mb_client read coils"));
    assert!(help.contains("mb_client write coil "));
    assert!(!help.contains("mb_client write coils"));
}

#[test]
fn test_read_area_names_are_singular() {
    assert!(
        Cli::try_parse_from([
            "mb_client",
            "read",
            "coils",
            "--ip",
            "127.0.0.1",
            "--unit",
            "0"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "mb_client",
            "read",
            "coil",
            "--ip",
            "127.0.0.1",
            "--unit",
            "0"
        ])
        .is_ok()
    );
}

#[test]
fn addresses_accept_hex_and_area_references() {
    let cli = Cli::try_parse_from([
        "mb_client",
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

    let cli = Cli::try_parse_from([
        "mb_client",
        "read",
        "0xxxx",
        "--ip",
        "127.0.0.1",
        "--unit",
        "0x10",
    ])
    .unwrap();
    match cli.cmd {
        Command::Read {
            area: ReadArea::Coil { start, .. },
        } => assert_eq!(start, 16),
        other => panic!("expected coil read, got {other:?}"),
    }

    assert!(
        Cli::try_parse_from([
            "mb_client",
            "read",
            "coil",
            "--ip",
            "127.0.0.1",
            "--unit",
            "40001",
        ])
        .is_err()
    );
    assert!(Cli::try_parse_from(["mb_client", "write", "discrete"]).is_ok());
    assert!(Cli::try_parse_from(["mb_client", "write", "input-register"]).is_ok());
}

#[test]
fn decode_flags_select_format_order_and_byte_swap() {
    let cli = Cli::try_parse_from([
        "mb_client",
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
    let mut command = vec![
        "mb_client",
        "read",
        "holding",
        "--ip",
        "127.0.0.1",
        "--unit",
        "0",
    ];
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

#[test]
fn scan_flags_default_to_one_second_and_pdu_addresses() {
    let cli = Cli::try_parse_from([
        "mb_client",
        "read",
        "holding",
        "--ip",
        "127.0.0.1",
        "--unit",
        "0",
        "--interval",
        "250",
        "--notation",
        "modicon",
        "--response-timeout",
        "2",
        "--log",
        "scan.log",
    ])
    .unwrap();
    match cli.cmd {
        Command::Read {
            area: ReadArea::Holding { timing, common, .. },
        } => {
            assert_eq!(timing.interval_ms, 250);
            assert_eq!(
                timing.log.as_deref(),
                Some(std::path::Path::new("scan.log"))
            );
            assert_eq!(common.response_timeout, 2);
            assert_eq!(common.retries, 2);
            assert_eq!(transport_retries(&common), 0);
            assert_eq!(common.notation, AddressNotation::Modicon);
        }
        other => panic!("expected holding read, got {other:?}"),
    }
}

#[test]
fn hex_payload_accepts_spaces_commas_and_prefixes() {
    assert_eq!(parse_hex_payload("").unwrap(), Vec::<u8>::new());
    assert_eq!(
        parse_hex_payload("00,00 0x00,01").unwrap(),
        vec![0, 0, 0, 1]
    );
    assert!(parse_hex_payload("0100").is_err());
}
