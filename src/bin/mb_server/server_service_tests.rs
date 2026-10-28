//! In-memory service, listener selection, and the TCP MEI rewrite.

use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::runtime::{
    IdleTimeoutStream, ModbusData, ModbusService, ServerListen, ServerRuntime, adjust_mei_frame,
    pop_mbap_frame, server_listen,
};
use tokio::io::AsyncReadExt;
use tokio_modbus::prelude::{ExceptionCode, Request, Response, SlaveRequest};
use tokio_modbus::server::Service;

fn runtime(units: &[u8]) -> ServerRuntime {
    ServerRuntime {
        units: units.to_vec(),
        verbose: false,
        timeout_secs: 5,
        delay: Duration::ZERO,
        forced: None,
        silence_writes: false,
    }
}

fn service(data: Arc<tokio::sync::RwLock<ModbusData>>, units: &[u8]) -> ModbusService {
    ModbusService::new(data, &runtime(units))
}

#[test]
fn test_modbus_data_new() {
    let data = ModbusData::new(100, 200, 300, 400);

    assert_eq!(data.coils.len(), 100);
    assert_eq!(data.discrete_inputs.len(), 200);
    assert_eq!(data.holding_registers.len(), 300);
    assert_eq!(data.input_registers.len(), 400);
    assert!(data.discrete_inputs.iter().all(|bit| !bit));
}

#[test]
fn an_unsupported_mei_type_becomes_a_device_id_read() {
    let flag = std::sync::atomic::AtomicBool::new(false);
    let coil = std::sync::atomic::AtomicBool::new(false);
    let holding = [
        0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x01,
    ];
    assert_eq!(adjust_mei_frame(&holding, &flag, &coil), holding);
    assert!(!flag.load(std::sync::atomic::Ordering::SeqCst));

    let mei14 = [
        0x00, 0x02, 0x00, 0x00, 0x00, 0x05, 0x01, 0x2B, 0x0E, 0x01, 0x00,
    ];
    assert_eq!(adjust_mei_frame(&mei14, &flag, &coil), mei14);
    assert!(!flag.load(std::sync::atomic::Ordering::SeqCst));

    let mei13 = [0x00, 0x11, 0x00, 0x00, 0x00, 0x04, 0x01, 0x2B, 0x0D, 0x00];
    let adjusted = adjust_mei_frame(&mei13, &flag, &coil);
    assert!(flag.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!coil.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(
        adjusted,
        [
            0x00, 0x11, 0x00, 0x00, 0x00, 0x05, 0x01, 0x2B, 0x0E, 0x04, 0x00
        ]
    );

    let mut buffered = mei13.to_vec();
    buffered.extend_from_slice(&holding);
    let first = pop_mbap_frame(&mut buffered).expect("mei frame");
    assert_eq!(first, mei13);
    let second = pop_mbap_frame(&mut buffered).expect("holding frame");
    assert_eq!(second, holding);
    assert!(buffered.is_empty());
}

#[test]
fn an_illegal_coil_value_becomes_off_and_sets_the_flag() {
    let mei = std::sync::atomic::AtomicBool::new(false);
    let coil = std::sync::atomic::AtomicBool::new(false);
    let on = [
        0x00, 0x20, 0x00, 0x00, 0x00, 0x06, 0x01, 0x05, 0x00, 0xAC, 0xFF, 0x00,
    ];
    assert_eq!(adjust_mei_frame(&on, &mei, &coil), on);
    assert!(!coil.load(std::sync::atomic::Ordering::SeqCst));

    let illegal = [
        0x00, 0x21, 0x00, 0x00, 0x00, 0x06, 0x01, 0x05, 0x00, 0xAC, 0x00, 0x01,
    ];
    let adjusted = adjust_mei_frame(&illegal, &mei, &coil);
    assert!(coil.load(std::sync::atomic::Ordering::SeqCst));
    assert!(!mei.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(
        adjusted,
        [
            0x00, 0x21, 0x00, 0x00, 0x00, 0x06, 0x01, 0x05, 0x00, 0xAC, 0x00, 0x00
        ]
    );
}

#[test]
fn startup_draw_is_stable_for_one_seed_and_changes_with_another() {
    let first = ModbusData::from_seed(8, 4, 8, 8, 1);
    let again = ModbusData::from_seed(8, 4, 8, 8, 1);
    let other = ModbusData::from_seed(8, 4, 8, 8, 2);
    assert_eq!(first.coils, again.coils);
    assert_eq!(first.holding_registers, again.holding_registers);
    assert_eq!(first.input_registers, again.input_registers);
    assert!(first.discrete_inputs.iter().all(|bit| !bit));
    assert_ne!(first.holding_registers, other.holding_registers);
}

#[test]
fn test_modbus_data_small_sizes() {
    let data = ModbusData::new(1, 2, 3, 4);

    assert_eq!(data.coils.len(), 1);
    assert_eq!(data.discrete_inputs.len(), 2);
    assert_eq!(data.holding_registers.len(), 3);
    assert_eq!(data.input_registers.len(), 4);
    assert!(data.discrete_inputs.iter().all(|bit| !bit));
}

#[test]
fn test_modbus_data_zero_sizes() {
    let data = ModbusData::new(0, 0, 0, 0);

    assert!(data.coils.is_empty());
    assert!(data.discrete_inputs.is_empty());
    assert!(data.holding_registers.is_empty());
    assert!(data.input_registers.is_empty());
}

#[tokio::test]
async fn test_modbus_service_read_coils_valid() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(10, 10, 10, 10)));

    // Set some coil values for testing
    {
        let mut data_lock = data.write().await;
        data_lock.coils[0] = true;
        data_lock.coils[1] = false;
        data_lock.coils[2] = true;
    }

    let service = service(data, &[1]);
    let request = Request::ReadCoils(0, 3);

    let result = service.call(SlaveRequest { slave: 1, request }).await;
    assert!(result.is_ok());

    if let Ok(Some(Response::ReadCoils(coils))) = result {
        assert_eq!(coils.len(), 3);
        assert!(coils[0]);
        assert!(!coils[1]);
        assert!(coils[2]);
    } else {
        panic!("Expected ReadCoils response");
    }
}

#[tokio::test]
async fn test_modbus_service_read_coils_out_of_bounds() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(5, 5, 5, 5)));
    let service = service(data, &[1]);

    // Try to read beyond available coils
    let request = Request::ReadCoils(3, 5); // starts at 3, wants 5 coils = addresses 3,4,5,6,7 but only 0-4 exist

    let result = service.call(SlaveRequest { slave: 1, request }).await;
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), ExceptionCode::IllegalDataAddress);
}

#[tokio::test]
async fn test_modbus_service_read_holding_registers() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(10, 10, 10, 10)));
    {
        let mut guard = data.write().await;
        guard.holding_registers[5] = 5;
        guard.holding_registers[6] = 6;
        guard.holding_registers[7] = 7;
    }
    let service = service(data, &[1]);

    let request = Request::ReadHoldingRegisters(5, 3);

    let result = service.call(SlaveRequest { slave: 1, request }).await;
    assert!(result.is_ok());

    if let Ok(Some(Response::ReadHoldingRegisters(registers))) = result {
        assert_eq!(registers.len(), 3);
        assert_eq!(registers[0], 5); // address 5 = value 5
        assert_eq!(registers[1], 6); // address 6 = value 6
        assert_eq!(registers[2], 7); // address 7 = value 7
    } else {
        panic!("Expected ReadHoldingRegisters response");
    }
}

#[tokio::test]
async fn test_modbus_service_write_single_coil() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(10, 10, 10, 10)));
    let service = service(data.clone(), &[1]);

    let request = Request::WriteSingleCoil(3, true);

    let result = service.call(SlaveRequest { slave: 1, request }).await;
    assert!(result.is_ok());

    if let Ok(Some(Response::WriteSingleCoil(addr, value))) = result {
        assert_eq!(addr, 3);
        assert!(value);
    } else {
        panic!("Expected WriteSingleCoil response");
    }

    // Verify the coil was actually set
    let data_lock = data.read().await;
    assert!(data_lock.coils[3]);
}

#[tokio::test]
async fn test_modbus_service_write_single_register() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(10, 10, 10, 10)));
    let service = service(data.clone(), &[1]);

    let request = Request::WriteSingleRegister(2, 12345);

    let result = service.call(SlaveRequest { slave: 1, request }).await;
    assert!(result.is_ok());

    if let Ok(Some(Response::WriteSingleRegister(addr, value))) = result {
        assert_eq!(addr, 2);
        assert_eq!(value, 12345);
    } else {
        panic!("Expected WriteSingleRegister response");
    }

    // Verify the register was actually set
    let data_lock = data.read().await;
    assert_eq!(data_lock.holding_registers[2], 12345);
}

#[tokio::test]
async fn test_modbus_service_write_multiple_coils() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::new(10, 10, 10, 10)));
    let service = service(data.clone(), &[1]);

    let values = [true, false, true];
    let request = Request::WriteMultipleCoils(1, values.to_vec().into());

    let result = service.call(SlaveRequest { slave: 1, request }).await;
    assert!(result.is_ok());

    if let Ok(Some(Response::WriteMultipleCoils(addr, qty))) = result {
        assert_eq!(addr, 1);
        assert_eq!(qty, 3);
    } else {
        panic!("Expected WriteMultipleCoils response");
    }

    // Verify the coils were actually set
    let data_lock = data.read().await;
    assert!(data_lock.coils[1]);
    assert!(!data_lock.coils[2]);
    assert!(data_lock.coils[3]);
}

#[tokio::test]
async fn test_modbus_service_ignores_other_unit_ids() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
        10, 10, 10, 10, 11,
    )));
    data.write().await.coils[0] = false;
    let service = service(data.clone(), &[1]);
    let request = Request::WriteSingleCoil(0, true);

    let result = service
        .call(SlaveRequest { slave: 2, request })
        .await
        .unwrap();
    assert!(result.is_none());

    let data_lock = data.read().await;
    assert!(!data_lock.coils[0]);
}

#[test]
fn test_server_listen_defaults_to_all_interfaces() {
    let ip = "127.0.0.1".parse().unwrap();
    assert_eq!(
        server_listen(Some(ip), None, false).unwrap(),
        ServerListen::Tcp(ip)
    );
    assert_eq!(
        server_listen(None, Some(PathBuf::from("/dev/ttyUSB0")), false).unwrap(),
        ServerListen::Rtu(PathBuf::from("/dev/ttyUSB0"))
    );
    assert_eq!(
        server_listen(None, None, false).unwrap(),
        ServerListen::Tcp(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    );
    assert_eq!(
        server_listen(None, None, true).unwrap(),
        ServerListen::RtuOverTcp(IpAddr::V4(Ipv4Addr::UNSPECIFIED))
    );
    assert!(server_listen(Some(ip), Some(PathBuf::from("/dev/ttyUSB0")), false).is_err());
    assert!(server_listen(None, Some(PathBuf::from("/dev/ttyUSB0")), true).is_err());
}

#[tokio::test]
async fn test_idle_timeout_closes_silent_stream() {
    let (_client, server) = tokio::io::duplex(16);
    let mut timed = IdleTimeoutStream::new(server, Duration::from_millis(30));
    let mut buf = [0u8; 4];
    let n = timed.read(&mut buf).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn broadcast_write_updates_memory_without_a_response() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
        4, 1, 4, 1, 3,
    )));
    let service = service(data.clone(), &[1, 2]);
    let written = service
        .call(SlaveRequest {
            slave: 0,
            request: Request::WriteSingleRegister(1, 77),
        })
        .await
        .unwrap();
    assert!(written.is_none());
    assert_eq!(data.read().await.holding_registers[1], 77);

    let before = data.read().await.holding_registers[0];
    let read = service
        .call(SlaveRequest {
            slave: 0,
            request: Request::ReadHoldingRegisters(0, 1),
        })
        .await
        .unwrap();
    assert!(read.is_none());
    assert_eq!(data.read().await.holding_registers[0], before);
}

#[tokio::test]
async fn stations_share_one_image_and_unknown_ids_stay_silent() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
        4, 1, 4, 1, 4,
    )));
    let service = service(data.clone(), &[1, 2]);
    service
        .call(SlaveRequest {
            slave: 2,
            request: Request::WriteSingleCoil(0, true),
        })
        .await
        .unwrap();
    let seen = service
        .call(SlaveRequest {
            slave: 1,
            request: Request::ReadCoils(0, 1),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(seen, Response::ReadCoils(vec![true]));
    let ignored = service
        .call(SlaveRequest {
            slave: 3,
            request: Request::ReadCoils(0, 1),
        })
        .await
        .unwrap();
    assert!(ignored.is_none());
}

#[tokio::test]
async fn forced_exception_does_not_write_and_other_functions_do() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
        4, 1, 4, 1, 5,
    )));
    let original = data.read().await.holding_registers[1];
    let mut policy = runtime(&[1]);
    policy.forced = Some((0x06, ExceptionCode::IllegalDataAddress));
    let service = ModbusService::new(data.clone(), &policy);
    let denied = service
        .call(SlaveRequest {
            slave: 1,
            request: Request::WriteSingleRegister(1, 9),
        })
        .await;
    assert_eq!(denied.unwrap_err(), ExceptionCode::IllegalDataAddress);
    assert_eq!(data.read().await.holding_registers[1], original);

    service
        .call(SlaveRequest {
            slave: 1,
            request: Request::WriteSingleCoil(0, true),
        })
        .await
        .unwrap()
        .unwrap();
    assert!(data.read().await.coils[0]);
}

#[tokio::test]
async fn silence_skips_a_write() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
        4, 1, 4, 1, 6,
    )));
    let original = data.read().await.holding_registers[0];
    let mut policy = runtime(&[1]);
    policy.silence_writes = true;
    let service = ModbusService::new(data.clone(), &policy);
    let response = service
        .call(SlaveRequest {
            slave: 1,
            request: Request::WriteSingleRegister(0, 5),
        })
        .await
        .unwrap();
    assert!(response.is_none());
    assert_eq!(data.read().await.holding_registers[0], original);
}

#[tokio::test]
async fn delay_waits_before_a_normal_response() {
    let data = Arc::new(tokio::sync::RwLock::new(ModbusData::from_seed(
        2, 1, 2, 1, 7,
    )));
    let mut policy = runtime(&[1]);
    policy.delay = Duration::from_millis(40);
    let service = ModbusService::new(data, &policy);
    let started = std::time::Instant::now();
    service
        .call(SlaveRequest {
            slave: 1,
            request: Request::ReadHoldingRegisters(0, 1),
        })
        .await
        .unwrap()
        .unwrap();
    assert!(started.elapsed() >= Duration::from_millis(30));
}
