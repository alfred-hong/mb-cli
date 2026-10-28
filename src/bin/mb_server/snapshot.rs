//! CSV snapshot of coils, holding registers, and input registers.
//!
//! Each row is `area,address,value`. A later row for the same address wins.
//! Discrete inputs are not stored in the file.

use std::fs;
use std::path::Path;

use crate::address::{parse_coil_address, parse_holding_address, parse_input_address};
use anyhow::{Context, anyhow};

use crate::runtime::ModbusData;

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
