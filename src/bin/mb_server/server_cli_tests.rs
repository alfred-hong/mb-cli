//! Command-line parsing for `mb_server`.

use crate::cli::{Cli, forced_fault, served_stations};
use clap::Parser;

#[test]
fn parses_unit_timeout_and_verbose() {
    let cli = Cli::try_parse_from([
        "mb_server",
        "--ip",
        "127.0.0.1",
        "--modbus_address",
        "7",
        "--timeout",
        "12",
        "-v",
    ])
    .unwrap();

    assert_eq!(cli.units, vec![7]);
    assert_eq!(cli.timeout, 12);
    assert!(cli.verbose);
    assert_eq!(cli.ip.unwrap().to_string(), "127.0.0.1");
    assert!(cli.device.is_none());
}

#[test]
fn repeats_station_ids_and_rejects_broadcast() {
    let cli = Cli::try_parse_from(["mb_server", "--modbus_address", "1", "--addr", "2"]).unwrap();
    assert_eq!(served_stations(&cli.units).unwrap(), vec![1, 2]);
    assert_eq!(served_stations(&[]).unwrap(), vec![1]);
    assert!(served_stations(&[0]).is_err());
}

#[test]
fn exception_code_must_be_2_3_or_4() {
    assert!(forced_fault(Some(6), Some(2)).is_ok());
    assert!(forced_fault(Some(6), None).is_err());
    assert!(forced_fault(None, Some(2)).is_err());
    assert!(forced_fault(Some(6), Some(10)).is_err());
}
