use crate::address::{FileReadGroup, FileWriteGroup};
use crate::cli::{Common, DecodeArgs, DeviceIdCode};
use crate::client::{
    connect_to_modbus, each_read, modbus_operation_with_timeout, REQUEST_TIMEOUT_SECS,
};
use crate::decode;
use crate::table::print_register_table;
use std::borrow::Cow;
use std::fmt::Write as _;
use tokio_modbus::client::Context;
use tokio_modbus::prelude::*;

/// Function 15 accepts 1..=1968 coils. A single value uses function 05.
pub fn ensure_write_coil_count(count: usize) -> anyhow::Result<()> {
    if count > 0x07B0 {
        anyhow::bail!("Modbus function 15 writes at most 1968 coils (0x07B0); got {count}");
    }
    Ok(())
}

/// Function 16 accepts 1..=123 registers. A single value uses function 06.
pub fn ensure_write_register_count(count: usize) -> anyhow::Result<()> {
    if count > 123 {
        anyhow::bail!("Modbus function 16 writes at most 123 registers (0x007B); got {count}");
    }
    Ok(())
}

/// Function 23 writes 1..=121 registers.
pub fn ensure_read_write_count(count: usize) -> anyhow::Result<()> {
    if !(1..=121).contains(&count) {
        anyhow::bail!("Modbus function 23 writes 1-121 registers (0x0079); got {count}");
    }
    Ok(())
}

pub fn reject_readonly(area: &str, command: &str) -> anyhow::Error {
    anyhow::anyhow!("{area} are read-only. Read them with `{command}`.")
}

pub async fn read_exception_status(common: &Common, timeout: Option<i64>) -> anyhow::Result<()> {
    let mut client = connect_to_modbus(common).await?;
    each_read(timeout, async || {
        let response = call_custom(
            &mut client,
            0x07,
            &[],
            "read exception status",
            REQUEST_TIMEOUT_SECS,
        )
        .await?;
        let Response::Custom(_, bytes) = response else {
            anyhow::bail!("unexpected exception-status response");
        };
        let status = *bytes
            .first()
            .ok_or_else(|| anyhow::anyhow!("empty exception status"))?;
        let mut frame =
            format!("Exception status: {status:#04X} (coil 0 is the least significant bit)\n");
        for bit in 0..8 {
            let on = status & (1 << bit) != 0;
            let _ = writeln!(frame, "  coil {bit}: {}", if on { "ON" } else { "OFF" });
        }
        Ok(frame)
    })
    .await
}

pub async fn read_event_counter(common: &Common, timeout: Option<i64>) -> anyhow::Result<()> {
    let mut client = connect_to_modbus(common).await?;
    each_read(timeout, async || {
        let response = call_custom(
            &mut client,
            0x0B,
            &[],
            "read event counter",
            REQUEST_TIMEOUT_SECS,
        )
        .await?;
        let Response::Custom(_, bytes) = response else {
            anyhow::bail!("unexpected event-counter response");
        };
        let status = word_at(&bytes, 0)?;
        let count = word_at(&bytes, 2)?;
        Ok(format!(
            "Status: {status:#06X} ({})\nEvent count: {count}\n",
            if status == 0 { "ready" } else { "busy" }
        ))
    })
    .await
}

pub async fn read_event_log(common: &Common, timeout: Option<i64>) -> anyhow::Result<()> {
    let mut client = connect_to_modbus(common).await?;
    each_read(timeout, async || {
        let response = call_custom(
            &mut client,
            0x0C,
            &[],
            "read event log",
            REQUEST_TIMEOUT_SECS,
        )
        .await?;
        let Response::Custom(_, bytes) = response else {
            anyhow::bail!("unexpected event-log response");
        };
        if bytes.len() < 7 {
            anyhow::bail!("event log response is too short");
        }
        let status = word_at(&bytes, 1)?;
        let events = word_at(&bytes, 3)?;
        let messages = word_at(&bytes, 5)?;
        let mut frame = format!(
            "Byte count: {}\nStatus: {status:#06X} ({})\nEvent count: {events}\nMessage count: {messages}\nEvents, newest first:\n",
            bytes[0],
            if status == 0 { "ready" } else { "busy" }
        );
        if bytes.len() == 7 {
            frame.push_str("  (none)\n");
        }
        for (index, event) in bytes[7..].iter().enumerate() {
            let _ = writeln!(frame, "  {index}: {event:#04X} ({})", event_name(*event));
        }
        Ok(frame)
    })
    .await
}

pub async fn read_server_id(common: &Common, timeout: Option<i64>) -> anyhow::Result<()> {
    let mut client = connect_to_modbus(common).await?;
    each_read(timeout, async || {
        let response = modbus_operation_with_timeout(
            || client.call(Request::ReportServerId),
            "report server id",
            REQUEST_TIMEOUT_SECS,
        )
        .await?;
        let Response::ReportServerId(id, running, extra) = response else {
            anyhow::bail!("unexpected server-id response");
        };
        let mut frame = format!(
            "Server ID: {id}\nRun status: {}\n",
            if running { "ON" } else { "OFF" }
        );
        match std::str::from_utf8(&extra) {
            Ok(text) if !text.is_empty() => {
                let _ = writeln!(frame, "Additional data: {text}");
            }
            _ => {
                let _ = writeln!(frame, "Additional data: {extra:02X?}");
            }
        }
        Ok(frame)
    })
    .await
}

pub async fn read_fifo(
    common: &Common,
    addr: u16,
    decode_options: &DecodeArgs,
    timeout: Option<i64>,
) -> anyhow::Result<()> {
    let mut client = connect_to_modbus(common).await?;
    each_read(timeout, async || {
        let response = call_custom(
            &mut client,
            0x18,
            &addr.to_be_bytes(),
            "read fifo",
            REQUEST_TIMEOUT_SECS,
        )
        .await?;
        let Response::Custom(_, bytes) = response else {
            anyhow::bail!("unexpected FIFO response");
        };
        let byte_count = word_at(&bytes, 0)?;
        let count = word_at(&bytes, 2)?;
        let mut values = Vec::new();
        let mut offset = 4;
        while offset + 1 < bytes.len() {
            values.push(word_at(&bytes, offset)?);
            offset += 2;
        }
        let mut frame = format!("FIFO at {addr}: count {count}, byte count {byte_count}\n");
        let data_addr = addr.saturating_add(1);
        decode::write_words(
            &mut frame,
            &values,
            data_addr,
            common.verbose,
            decode_options,
        )?;
        Ok(frame)
    })
    .await
}

pub async fn read_file(
    common: &Common,
    groups: &[FileReadGroup],
    decode_options: &DecodeArgs,
    timeout: Option<i64>,
) -> anyhow::Result<()> {
    let payload = encode_file_read(groups)?;
    let mut client = connect_to_modbus(common).await?;
    each_read(timeout, async || {
        let response = call_custom(
            &mut client,
            0x14,
            &payload,
            "read file record",
            REQUEST_TIMEOUT_SECS,
        )
        .await?;
        let Response::Custom(_, bytes) = response else {
            anyhow::bail!("unexpected file-read response");
        };
        let mut frame = String::new();
        write_file_read(&mut frame, &bytes, groups, common.verbose, decode_options)?;
        Ok(frame)
    })
    .await
}

pub async fn read_device_id(
    common: &Common,
    code: DeviceIdCode,
    object: u8,
    timeout: Option<i64>,
) -> anyhow::Result<()> {
    let mut client = connect_to_modbus(common).await?;
    each_read(timeout, async || {
        let response = modbus_operation_with_timeout(
            || client.read_device_identification(code.read_code(), object),
            "read device identification",
            REQUEST_TIMEOUT_SECS,
        )
        .await?;
        let mut frame = format!(
            "Read code: {} ({:#04X})\nConformity: {:#04X}\nMore follows: {}\nNext object: {}\n",
            code_name(response.read_code),
            response.read_code.value(),
            response.conformity_level.value(),
            response.more_follows,
            response.next_object_id
        );
        for object in &response.device_id_objects {
            let text = object.value_as_str().unwrap_or("");
            let _ = writeln!(
                frame,
                "  {:>3} {:#04X} {}: {text}",
                object.id,
                object.id,
                object_name(object.id)
            );
        }
        Ok(frame)
    })
    .await
}

pub async fn diagnostics(common: &Common, sub: u16, data: u16) -> anyhow::Result<()> {
    if sub == 0x0004 {
        println!(
            "Listen-only mode sends no response. A timeout means the server entered that mode."
        );
    }
    let mut payload = Vec::with_capacity(4);
    payload.extend(sub.to_be_bytes());
    payload.extend(data.to_be_bytes());
    let response = custom(common, 0x08, &payload, "diagnostics").await?;
    let Response::Custom(_, bytes) = response else {
        anyhow::bail!("unexpected diagnostics response");
    };
    let response_sub = word_at(&bytes, 0)?;
    let response_data = word_at(&bytes, 2)?;
    println!(
        "Diagnostics sub {response_sub:#06X} ({}) data {response_data} ({response_data:#06X})",
        diagnostic_name(response_sub)
    );
    Ok(())
}

pub async fn write_mask(
    common: &Common,
    addr: u16,
    and_mask: u16,
    or_mask: u16,
) -> anyhow::Result<()> {
    let mut client = connect_to_modbus(common).await?;
    modbus_operation_with_timeout(
        || client.masked_write_register(addr, and_mask, or_mask),
        "mask write register",
        REQUEST_TIMEOUT_SECS,
    )
    .await?;
    println!(
        "Mask-wrote holding register {addr}: AND {and_mask:#06X}, OR {or_mask:#06X} (Modbus address: {})",
        common.unit
    );
    Ok(())
}

pub async fn write_file(common: &Common, groups: &[FileWriteGroup]) -> anyhow::Result<()> {
    let payload = encode_file_write(groups)?;
    let response = custom(common, 0x15, &payload, "write file record").await?;
    let Response::Custom(_, bytes) = response else {
        anyhow::bail!("unexpected file-write response");
    };
    if bytes.as_ref() != payload {
        anyhow::bail!("file write response did not echo the request");
    }
    println!(
        "Wrote {} file group(s) (Modbus address: {})",
        groups.len(),
        common.unit
    );
    for group in groups {
        println!(
            "File {} record {} ({} value(s))",
            group.file,
            group.record,
            group.values.len()
        );
        print_register_table(&group.values, group.record, common.verbose);
    }
    Ok(())
}

pub async fn read_write(
    common: &Common,
    read_addr: u16,
    read_qty: u16,
    write_addr: u16,
    values: &[u16],
    decode_options: &DecodeArgs,
) -> anyhow::Result<()> {
    ensure_read_write_count(values.len())?;
    let mut client = connect_to_modbus(common).await?;
    let registers = modbus_operation_with_timeout(
        || client.read_write_multiple_registers(read_addr, read_qty, write_addr, values),
        "read/write registers",
        REQUEST_TIMEOUT_SECS,
    )
    .await?;
    println!(
        "Wrote {} holding register(s) at {write_addr}, then read {} at {read_addr} (Modbus address: {})",
        values.len(),
        registers.len(),
        common.unit
    );
    decode::print_words(&registers, read_addr, common.verbose, decode_options)?;
    Ok(())
}

async fn custom(common: &Common, code: u8, bytes: &[u8], name: &str) -> anyhow::Result<Response> {
    let mut client = connect_to_modbus(common).await?;
    call_custom(&mut client, code, bytes, name, REQUEST_TIMEOUT_SECS).await
}

async fn call_custom(
    client: &mut Context,
    code: u8,
    bytes: &[u8],
    name: &str,
    timeout_secs: u64,
) -> anyhow::Result<Response> {
    modbus_operation_with_timeout(
        || client.call(Request::Custom(code, Cow::Borrowed(bytes))),
        name,
        timeout_secs,
    )
    .await
}

fn encode_file_read(groups: &[FileReadGroup]) -> anyhow::Result<Vec<u8>> {
    let mut body = Vec::new();
    for group in groups {
        body.push(6);
        body.extend(group.file.to_be_bytes());
        body.extend(group.record.to_be_bytes());
        body.extend(group.length.to_be_bytes());
    }
    checked_body(body, 0x07, 0xF5, "file read")
}

fn encode_file_write(groups: &[FileWriteGroup]) -> anyhow::Result<Vec<u8>> {
    let mut body = Vec::new();
    for group in groups {
        if group.values.is_empty() || group.values.len() > usize::from(u16::MAX) {
            anyhow::bail!("each file write group needs 1-65535 values");
        }
        body.push(6);
        body.extend(group.file.to_be_bytes());
        body.extend(group.record.to_be_bytes());
        body.extend((group.values.len() as u16).to_be_bytes());
        for value in &group.values {
            body.extend(value.to_be_bytes());
        }
    }
    checked_body(body, 0x09, 0xFB, "file write")
}

fn checked_body(body: Vec<u8>, min: usize, max: usize, name: &str) -> anyhow::Result<Vec<u8>> {
    if !(min..=max).contains(&body.len()) {
        anyhow::bail!("{name} data must be {min}-{max} bytes; got {}", body.len());
    }
    let mut request = Vec::with_capacity(1 + body.len());
    request.push(u8::try_from(body.len()).expect("body length fits in one byte"));
    request.extend(body);
    Ok(request)
}

fn write_file_read(
    out: &mut String,
    bytes: &[u8],
    groups: &[FileReadGroup],
    verbose: bool,
    decode_options: &DecodeArgs,
) -> anyhow::Result<()> {
    if bytes.is_empty() {
        anyhow::bail!("empty file-read response");
    }
    let _ = writeln!(out, "File response length: {}", bytes[0]);
    let mut offset = 1;
    let mut index = 0;
    while offset < bytes.len() {
        let sub_len = usize::from(bytes[offset]);
        if offset + 1 + sub_len > bytes.len() || sub_len < 1 {
            anyhow::bail!("truncated file-read group");
        }
        let reference = bytes[offset + 1];
        let data = &bytes[offset + 2..offset + 1 + sub_len];
        if data.len() % 2 != 0 {
            anyhow::bail!("file record data is not a whole number of registers");
        }
        let values: Vec<u16> = data
            .chunks(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .collect();
        let (file, record) = groups
            .get(index)
            .map(|group| (group.file, group.record))
            .unwrap_or((0, 0));
        let _ = writeln!(
            out,
            "Group {}: file {file}, record {record}, reference type {reference}",
            index + 1
        );
        decode::write_words(out, &values, record, verbose, decode_options)?;
        offset += 1 + sub_len;
        index += 1;
    }
    Ok(())
}

fn word_at(bytes: &[u8], offset: usize) -> anyhow::Result<u16> {
    let pair = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| anyhow::anyhow!("response is shorter than the function requires"))?;
    Ok(u16::from_be_bytes([pair[0], pair[1]]))
}

fn event_name(event: u8) -> &'static str {
    match event {
        0x04 => "listen only",
        0x00 => "restart",
        0x80 => "receive",
        0x40 => "send",
        _ => "event",
    }
}

fn diagnostic_name(sub: u16) -> &'static str {
    match sub {
        0x0000 => "return query data",
        0x0001 => "restart communications",
        0x0002 => "return diagnostic register",
        0x0003 => "change ASCII delimiter",
        0x0004 => "force listen only",
        0x000A => "clear counters",
        0x000B => "bus message count",
        0x000C => "bus communication error count",
        0x000D => "bus exception count",
        0x000E => "server message count",
        0x000F => "server no response count",
        0x0010 => "server NAK count",
        0x0011 => "server busy count",
        0x0012 => "bus character overrun count",
        0x0014 => "clear overrun counter",
        _ => "sub-function",
    }
}

fn code_name(code: ReadCode) -> &'static str {
    match code {
        ReadCode::Basic => "basic",
        ReadCode::Regular => "regular",
        ReadCode::Extended => "extended",
        ReadCode::Specific => "specific",
    }
}

fn object_name(id: u8) -> &'static str {
    match id {
        0x00 => "VendorName",
        0x01 => "ProductCode",
        0x02 => "MajorMinorRevision",
        0x03 => "VendorUrl",
        0x04 => "ProductName",
        0x05 => "ModelName",
        0x06 => "UserApplicationName",
        _ => "Object",
    }
}
