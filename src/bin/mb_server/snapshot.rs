//! CSV snapshot of coils, holding registers, and input registers.
//!
//! Each row is `area,address,value`. A later row for the same address wins.
//! Discrete inputs are not stored in the file.

use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context};
use mb_cli::address::{parse_coil_address, parse_holding_address, parse_input_address};

use crate::server::ModbusData;

/// Load `path` and let its rows replace values already in `data`.
pub fn load_file(data: &mut ModbusData, path: &Path) -> anyhow::Result<()> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    apply(&text, data).map_err(|err| anyhow!(err))
}

/// Rewrite `path` with every coil, holding register, and input register.
pub fn save_file(data: &ModbusData, path: &Path) -> anyhow::Result<()> {
    fs::write(path, render(data)).with_context(|| format!("writing {}", path.display()))
}

/// Apply CSV text. The header row is required.
pub fn apply(text: &str, data: &mut ModbusData) -> Result<(), String> {
    let mut rows = text
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty());
    let Some((header_line, header)) = rows.next() else {
        return Err("CSV header is required: area,address,value".to_string());
    };
    let header_fields: Vec<String> = split_row(header)
        .into_iter()
        .map(|field| field.to_ascii_lowercase())
        .collect();
    if header_fields != ["area", "address", "value"] {
        return Err(format!(
            "line {}: header must be area,address,value",
            header_line + 1
        ));
    }
    for (idx, line) in rows {
        let fields = split_row(line);
        if fields.len() != 3 {
            return Err(format!("line {}: expected area,address,value", idx + 1));
        }
        apply_row(idx + 1, fields[0], fields[1], fields[2], data)?;
    }
    Ok(())
}

/// Render the three areas. Addresses are decimal PDU addresses.
pub fn render(data: &ModbusData) -> String {
    let mut out = String::from("area,address,value\n");
    for (addr, value) in data.coils.iter().enumerate() {
        out.push_str(&format!("coil,{addr},{}\n", u8::from(*value)));
    }
    for (addr, value) in data.holding_registers.iter().enumerate() {
        out.push_str(&format!("holding,{addr},{value}\n"));
    }
    for (addr, value) in data.input_registers.iter().enumerate() {
        out.push_str(&format!("input,{addr},{value}\n"));
    }
    out
}

fn split_row(line: &str) -> Vec<&str> {
    line.split(',').map(str::trim).collect()
}

fn apply_row(
    line: usize,
    area: &str,
    address: &str,
    value: &str,
    data: &mut ModbusData,
) -> Result<(), String> {
    match area.to_ascii_lowercase().as_str() {
        "coil" => {
            let addr = parse_coil_address(address).map_err(|err| format!("line {line}: {err}"))?;
            let bit = parse_coil_value(value).map_err(|err| format!("line {line}: {err}"))?;
            let slot = slot(line, "coil", addr, data.coils.len())?;
            data.coils[slot] = bit;
        }
        "holding" => {
            let addr =
                parse_holding_address(address).map_err(|err| format!("line {line}: {err}"))?;
            let word = parse_register_value(value).map_err(|err| format!("line {line}: {err}"))?;
            let slot = slot(line, "holding", addr, data.holding_registers.len())?;
            data.holding_registers[slot] = word;
        }
        "input" => {
            let addr = parse_input_address(address).map_err(|err| format!("line {line}: {err}"))?;
            let word = parse_register_value(value).map_err(|err| format!("line {line}: {err}"))?;
            let slot = slot(line, "input", addr, data.input_registers.len())?;
            data.input_registers[slot] = word;
        }
        "discrete" => {
            return Err(format!(
                "line {line}: discrete inputs are not stored in the CSV"
            ));
        }
        other => {
            return Err(format!(
                "line {line}: area '{other}' must be coil, holding, or input"
            ));
        }
    }
    Ok(())
}

fn slot(line: usize, area: &str, addr: u16, len: usize) -> Result<usize, String> {
    let index = usize::from(addr);
    if index < len {
        Ok(index)
    } else {
        Err(format!(
            "line {line}: {area} address {addr} is outside 0..{len}"
        ))
    }
}

fn parse_coil_value(text: &str) -> Result<bool, String> {
    match text {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(format!("coil value '{text}' must be 0 or 1")),
    }
}

fn parse_register_value(text: &str) -> Result<u16, String> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "register value '{text}' must be a decimal integer 0-65535"
        ));
    }
    text.parse::<u16>()
        .map_err(|_| format!("register value '{text}' must be a decimal integer 0-65535"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
