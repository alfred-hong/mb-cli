//! CSV snapshot of coils, holding registers, and input registers.

use crate::runtime::ModbusData;
use crate::snapshot::{apply, render};

fn sample() -> ModbusData {
    ModbusData::from_seed(4, 2, 4, 3, 1)
}

#[test]
fn csv_row_overrides_one_address_and_last_row_wins() {
    let mut data = sample();
    let before = data.holding_registers[1];
    apply(
        "area,address,value\nholding,1,9\nholding,40002,8\ncoil,0,1\ninput,0,42\n",
        &mut data,
    )
    .unwrap();
    assert_eq!(data.holding_registers[1], 8);
    assert_ne!(before, 8);
    assert!(data.coils[0]);
    assert_eq!(data.input_registers[0], 42);
    assert!(data.discrete_inputs.iter().all(|bit| !bit));
}

#[test]
fn csv_rejects_discrete_and_addresses_outside_the_window() {
    let mut data = sample();
    let discrete = apply("area,address,value\ndiscrete,0,1\n", &mut data);
    assert!(discrete.unwrap_err().contains("discrete"));
    let outside = apply("area,address,value\nholding,9,1\n", &mut data);
    assert!(outside.unwrap_err().contains("outside"));
}

#[test]
fn render_round_trip_keeps_values() {
    let mut data = sample();
    data.coils[1] = true;
    data.holding_registers[2] = 99;
    data.input_registers[1] = 7;
    let mut loaded = ModbusData::from_seed(4, 2, 4, 3, 99);
    apply(&render(&data), &mut loaded).unwrap();
    assert_eq!(loaded.coils, data.coils);
    assert_eq!(loaded.holding_registers, data.holding_registers);
    assert_eq!(loaded.input_registers, data.input_registers);
    assert!(loaded.discrete_inputs.iter().all(|bit| !bit));
}
