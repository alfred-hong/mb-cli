//! Request handling against the in-memory image.

use std::borrow::Cow;

use crate::handle::{device_identification_payload, handle_request, summarize};
use crate::runtime::ModbusData;
use tokio_modbus::prelude::{ExceptionCode, Request, Response};

fn data() -> ModbusData {
    ModbusData::new(16, 16, 16, 16)
}

#[test]
fn quantity_zero_is_illegal_data_value() {
    let mut data = data();
    let error = handle_request(&mut data, Request::ReadCoils(0, 0), 1).unwrap_err();
    assert_eq!(error, ExceptionCode::IllegalDataValue);
}

#[test]
fn mask_write_updates_selected_bits() {
    let mut data = data();
    data.holding_registers[4] = 0x0012;
    let response = handle_request(&mut data, Request::MaskWriteRegister(4, 0x00F2, 0x0025), 1)
        .unwrap()
        .unwrap();
    assert_eq!(response, Response::MaskWriteRegister(4, 0x00F2, 0x0025));
    assert_eq!(data.holding_registers[4], 0x0017);
}

#[test]
fn read_write_writes_before_reading() {
    let mut data = data();
    let response = handle_request(
        &mut data,
        Request::ReadWriteMultipleRegisters(0, 2, 0, Cow::Owned(vec![9, 8])),
        1,
    )
    .unwrap()
    .unwrap();
    assert_eq!(response, Response::ReadWriteMultipleRegisters(vec![9, 8]));
}

#[test]
fn fifo_count_above_31_is_illegal_data_value() {
    let mut data = data();
    data.holding_registers[1] = 32;
    data.holding_registers[2] = 9;
    let error = handle_request(
        &mut data,
        Request::Custom(0x18, Cow::Owned(vec![0x00, 0x01])),
        1,
    )
    .unwrap_err();
    assert_eq!(error, ExceptionCode::IllegalDataValue);
    assert_eq!(data.holding_registers[1], 32);
    assert_eq!(data.holding_registers[2], 9);
}

#[test]
fn fifo_reads_count_and_following_registers() {
    let mut data = data();
    data.holding_registers[1] = 2;
    data.holding_registers[2] = 0x01B8;
    data.holding_registers[3] = 0x1284;
    let response = handle_request(
        &mut data,
        Request::Custom(0x18, Cow::Owned(vec![0x00, 0x01])),
        1,
    )
    .unwrap()
    .unwrap();
    match response {
        Response::Custom(0x18, bytes) => {
            assert_eq!(
                bytes.as_ref(),
                [0x00, 0x06, 0x00, 0x02, 0x01, 0xB8, 0x12, 0x84]
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn file_round_trip_and_device_identity() {
    let mut data = data();
    let write = vec![0x09, 0x06, 0x00, 0x04, 0x00, 0x07, 0x00, 0x01, 0x12, 0x34];
    handle_request(
        &mut data,
        Request::Custom(0x15, Cow::Owned(write.clone())),
        1,
    )
    .unwrap()
    .unwrap();
    assert_eq!(data.files.get(&(4, 7)), Some(&0x1234));
    let read = vec![0x07, 0x06, 0x00, 0x04, 0x00, 0x07, 0x00, 0x01];
    let response = handle_request(&mut data, Request::Custom(0x14, Cow::Owned(read)), 1)
        .unwrap()
        .unwrap();
    match response {
        Response::Custom(0x14, bytes) => {
            assert_eq!(bytes.as_ref(), [0x04, 0x03, 0x06, 0x12, 0x34]);
        }
        other => panic!("unexpected {other:?}"),
    }

    let payload = device_identification_payload(0x01, 0).unwrap();
    assert_eq!(payload[0], 0x0E);
    assert_eq!(payload[2], 0x82);
    assert!(payload.windows(5).any(|window| window == b"mb-rs"));
    let restarted = device_identification_payload(0x01, 99).unwrap();
    assert_eq!(restarted, payload);
    assert_eq!(
        device_identification_payload(0x04, 99).unwrap_err(),
        ExceptionCode::IllegalDataAddress
    );
    assert_eq!(
        device_identification_payload(0x09, 0).unwrap_err(),
        ExceptionCode::IllegalDataValue
    );

    let response = handle_request(&mut data, Request::ReportServerId, 7)
        .unwrap()
        .unwrap();
    match response {
        Response::Custom(0x11, bytes) => {
            assert_eq!(
                bytes.as_ref(),
                [0x07, 0x07, 0xFF, b'm', b'b', b'-', b'r', b's']
            );
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn diagnostics_echo_and_listen_only() {
    let mut data = data();
    let echo = handle_request(
        &mut data,
        Request::Custom(0x08, Cow::Owned(vec![0x00, 0x00, 0x12, 0x34])),
        1,
    )
    .unwrap()
    .unwrap();
    assert!(matches!(echo, Response::Custom(0x08, _)));

    let silent = handle_request(
        &mut data,
        Request::Custom(0x08, Cow::Owned(vec![0x00, 0x04, 0x00, 0x00])),
        1,
    )
    .unwrap();
    assert!(silent.is_none());
    assert!(data.diagnostics.listen_only);

    let ignored = handle_request(&mut data, Request::ReadCoils(0, 1), 1).unwrap();
    assert!(ignored.is_none());
}

#[test]
fn summary_includes_function_address_quantity_and_outcome() {
    let request = Request::ReadHoldingRegisters(5, 3);
    let ok = summarize(
        1,
        &request,
        &Ok(Some(Response::ReadHoldingRegisters(vec![1, 2, 3]))),
    );
    assert_eq!(ok, "station=1 fc=0x03 addr=5 qty=3 result=ok");
    let denied = summarize(4, &request, &Err(ExceptionCode::IllegalDataAddress));
    assert_eq!(
        denied,
        "station=4 fc=0x03 addr=5 qty=3 result=exception 0x02"
    );
    let silent = summarize(0, &Request::WriteSingleRegister(2, 9), &Ok(None));
    assert_eq!(silent, "station=0 fc=0x06 addr=2 qty=1 result=no-response");
}
