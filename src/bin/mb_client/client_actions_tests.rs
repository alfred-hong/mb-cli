//! Raw function codes mapped onto typed Modbus requests.

use crate::actions::request_from_raw;
use tokio_modbus::prelude::Request;

#[test]
fn raw_holding_read_uses_the_typed_request() {
    let request = request_from_raw(3, &[0x00, 0x00, 0x00, 0x01]).unwrap();
    assert!(matches!(request, Request::ReadHoldingRegisters(0, 1)));
}
