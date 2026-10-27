use crate::server::ModbusData;
use tokio_modbus::bytes::Bytes;
use tokio_modbus::prelude::*;

const MAX_FILE_RECORD: u16 = 0x270F;
const FILE_REFERENCE_TYPE: u8 = 6;

/// Apply one decoded request to the in-memory server.
pub(crate) fn handle_request(
    data: &mut ModbusData,
    request: Request<'static>,
    unit: u8,
) -> Result<Option<Response>, ExceptionCode> {
    if data.diagnostics.listen_only && !is_restart(&request) {
        data.diagnostics.no_response_count = data.diagnostics.no_response_count.saturating_add(1);
        return Ok(None);
    }

    if is_restart(&request) {
        return restart_communications(data, &request);
    }

    data.diagnostics.message_count = data.diagnostics.message_count.saturating_add(1);
    data.diagnostics.server_message_count = data.diagnostics.server_message_count.saturating_add(1);
    push_event(data, 0x80);

    let outcome = dispatch(data, &request, unit);
    match &outcome {
        Ok(Some(_)) => {
            if counts_event(&request) {
                data.diagnostics.event_count = data.diagnostics.event_count.saturating_add(1);
            }
            push_event(data, 0x40);
        }
        Ok(None) => {
            data.diagnostics.no_response_count =
                data.diagnostics.no_response_count.saturating_add(1);
        }
        Err(_) => {
            data.diagnostics.exception_count = data.diagnostics.exception_count.saturating_add(1);
        }
    }
    outcome
}

fn dispatch(
    data: &mut ModbusData,
    request: &Request<'static>,
    unit: u8,
) -> Result<Option<Response>, ExceptionCode> {
    let response = match request {
        Request::ReadCoils(addr, qty) => {
            check_qty(*qty, 1, 2000)?;
            let range = span(*addr, *qty, data.coils.len())?;
            Response::ReadCoils(data.coils[range].to_vec())
        }
        Request::ReadDiscreteInputs(addr, qty) => {
            check_qty(*qty, 1, 2000)?;
            let range = span(*addr, *qty, data.discrete_inputs.len())?;
            Response::ReadDiscreteInputs(data.discrete_inputs[range].to_vec())
        }
        Request::ReadHoldingRegisters(addr, qty) => {
            check_qty(*qty, 1, 125)?;
            let range = span(*addr, *qty, data.holding_registers.len())?;
            Response::ReadHoldingRegisters(data.holding_registers[range].to_vec())
        }
        Request::ReadInputRegisters(addr, qty) => {
            check_qty(*qty, 1, 125)?;
            let range = span(*addr, *qty, data.input_registers.len())?;
            Response::ReadInputRegisters(data.input_registers[range].to_vec())
        }
        Request::WriteSingleCoil(addr, value) => {
            let index = single_index(*addr, data.coils.len())?;
            data.coils[index] = *value;
            Response::WriteSingleCoil(*addr, *value)
        }
        Request::WriteSingleRegister(addr, value) => {
            let index = single_index(*addr, data.holding_registers.len())?;
            data.holding_registers[index] = *value;
            Response::WriteSingleRegister(*addr, *value)
        }
        Request::WriteMultipleCoils(addr, values) => {
            let qty = u16::try_from(values.len()).map_err(|_| ExceptionCode::IllegalDataValue)?;
            check_qty(qty, 1, 0x07B0)?;
            let range = span(*addr, qty, data.coils.len())?;
            for (slot, value) in data.coils[range].iter_mut().zip(values.iter()) {
                *slot = *value;
            }
            Response::WriteMultipleCoils(*addr, qty)
        }
        Request::WriteMultipleRegisters(addr, values) => {
            let qty = u16::try_from(values.len()).map_err(|_| ExceptionCode::IllegalDataValue)?;
            check_qty(qty, 1, 123)?;
            let range = span(*addr, qty, data.holding_registers.len())?;
            for (slot, value) in data.holding_registers[range].iter_mut().zip(values.iter()) {
                *slot = *value;
            }
            Response::WriteMultipleRegisters(*addr, qty)
        }
        Request::MaskWriteRegister(addr, and_mask, or_mask) => {
            let index = single_index(*addr, data.holding_registers.len())?;
            let current = data.holding_registers[index];
            data.holding_registers[index] = (current & and_mask) | (or_mask & !and_mask);
            Response::MaskWriteRegister(*addr, *and_mask, *or_mask)
        }
        Request::ReadWriteMultipleRegisters(read_addr, read_qty, write_addr, words) => {
            let write_qty =
                u16::try_from(words.len()).map_err(|_| ExceptionCode::IllegalDataValue)?;
            check_qty(*read_qty, 1, 125)?;
            check_qty(write_qty, 1, 0x0079)?;
            let read_range = span(*read_addr, *read_qty, data.holding_registers.len())?;
            let write_range = span(*write_addr, write_qty, data.holding_registers.len())?;
            for (slot, value) in data.holding_registers[write_range]
                .iter_mut()
                .zip(words.iter())
            {
                *slot = *value;
            }
            Response::ReadWriteMultipleRegisters(data.holding_registers[read_range].to_vec())
        }
        Request::ReportServerId => {
            // tokio-modbus sizes this typed response one byte short of what it encodes,
            // so the TCP length field would truncate the last byte. A custom PDU keeps
            // the length and the encoded bytes in agreement.
            let name = b"mb-cli";
            let mut payload = Vec::with_capacity(3 + name.len());
            let byte_count = u8::try_from(2 + name.len()).expect("server id fits in the PDU");
            payload.push(byte_count);
            payload.push(unit);
            payload.push(0xFF);
            payload.extend_from_slice(name);
            Response::Custom(0x11, Bytes::from(payload))
        }
        Request::ReadDeviceIdentification(code, object_id) => {
            let code_value = code.value();
            let payload = device_identification_payload(code_value, *object_id)?;
            Response::Custom(0x2B, Bytes::from(payload))
        }
        Request::Custom(0x07, bytes) if bytes.is_empty() => {
            Response::Custom(0x07, Bytes::from(vec![exception_status(data)]))
        }
        Request::Custom(0x08, bytes) => {
            let (sub, word) = words(bytes)?;
            let response_word = diagnostics(data, sub, word)?;
            if data.diagnostics.listen_only {
                return Ok(None);
            }
            Response::Custom(0x08, Bytes::from(word_pair(sub, response_word)))
        }
        Request::Custom(0x0B, bytes) if bytes.is_empty() => Response::Custom(
            0x0B,
            Bytes::from(word_pair(0x0000, data.diagnostics.event_count)),
        ),
        Request::Custom(0x0C, bytes) if bytes.is_empty() => {
            Response::Custom(0x0C, Bytes::from(event_log(data)))
        }
        Request::Custom(0x14, bytes) => {
            Response::Custom(0x14, Bytes::from(read_file(data, bytes)?))
        }
        Request::Custom(0x15, bytes) => {
            Response::Custom(0x15, Bytes::from(write_file(data, bytes)?))
        }
        Request::Custom(0x18, bytes) => {
            Response::Custom(0x18, Bytes::from(read_fifo(data, bytes)?))
        }
        Request::Custom(_, _) => return Err(ExceptionCode::IllegalFunction),
    };
    Ok(Some(response))
}

fn check_qty(qty: u16, min: u16, max: u16) -> Result<(), ExceptionCode> {
    if (min..=max).contains(&qty) {
        Ok(())
    } else {
        Err(ExceptionCode::IllegalDataValue)
    }
}

fn span(start: u16, qty: u16, len: usize) -> Result<std::ops::Range<usize>, ExceptionCode> {
    let end = u32::from(start)
        .checked_add(u32::from(qty))
        .ok_or(ExceptionCode::IllegalDataAddress)?;
    if end > 65536 || end > u32::try_from(len).unwrap_or(u32::MAX) {
        return Err(ExceptionCode::IllegalDataAddress);
    }
    Ok(usize::from(start)..end as usize)
}

fn single_index(addr: u16, len: usize) -> Result<usize, ExceptionCode> {
    let index = usize::from(addr);
    if index < len {
        Ok(index)
    } else {
        Err(ExceptionCode::IllegalDataAddress)
    }
}

fn exception_status(data: &ModbusData) -> u8 {
    let mut status = 0u8;
    for (bit, coil) in data.coils.iter().take(8).enumerate() {
        if *coil {
            status |= 1 << bit;
        }
    }
    status
}

fn words(bytes: &[u8]) -> Result<(u16, u16), ExceptionCode> {
    if bytes.len() != 4 {
        return Err(ExceptionCode::IllegalDataValue);
    }
    Ok((
        u16::from_be_bytes([bytes[0], bytes[1]]),
        u16::from_be_bytes([bytes[2], bytes[3]]),
    ))
}

fn word_pair(first: u16, second: u16) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(4);
    bytes.extend(first.to_be_bytes());
    bytes.extend(second.to_be_bytes());
    bytes
}

fn diagnostics(data: &mut ModbusData, sub: u16, word: u16) -> Result<u16, ExceptionCode> {
    match sub {
        0x0000 => Ok(word),
        0x0002 => Ok(data.diagnostics.register),
        0x0003 => {
            data.diagnostics.delimiter = (word >> 8) as u8;
            Ok(word)
        }
        0x0004 => {
            data.diagnostics.listen_only = true;
            push_event(data, 0x04);
            Ok(word)
        }
        0x000A => {
            clear_counters(data);
            Ok(word)
        }
        0x000B => Ok(data.diagnostics.message_count),
        0x000C => Ok(data.diagnostics.comm_error_count),
        0x000D => Ok(data.diagnostics.exception_count),
        0x000E => Ok(data.diagnostics.server_message_count),
        0x000F => Ok(data.diagnostics.no_response_count),
        0x0010 => Ok(data.diagnostics.nak_count),
        0x0011 => Ok(data.diagnostics.busy_count),
        0x0012 => Ok(data.diagnostics.overrun_count),
        0x0014 => {
            data.diagnostics.overrun_count = 0;
            Ok(word)
        }
        _ => Err(ExceptionCode::IllegalFunction),
    }
}

fn restart_communications(
    data: &mut ModbusData,
    request: &Request<'static>,
) -> Result<Option<Response>, ExceptionCode> {
    let Request::Custom(_, bytes) = request else {
        return Err(ExceptionCode::IllegalFunction);
    };
    let (_, word) = words(bytes)?;
    data.diagnostics.listen_only = false;
    if word == 0xFF00 {
        clear_counters(data);
    }
    push_event(data, 0x00);
    Ok(Some(Response::Custom(0x08, Bytes::copy_from_slice(bytes))))
}

fn clear_counters(data: &mut ModbusData) {
    data.diagnostics.register = 0;
    data.diagnostics.event_count = 0;
    data.diagnostics.message_count = 0;
    data.diagnostics.comm_error_count = 0;
    data.diagnostics.exception_count = 0;
    data.diagnostics.server_message_count = 0;
    data.diagnostics.no_response_count = 0;
    data.diagnostics.nak_count = 0;
    data.diagnostics.busy_count = 0;
    data.diagnostics.overrun_count = 0;
    data.diagnostics.events.clear();
}

fn is_restart(request: &Request<'_>) -> bool {
    match request {
        Request::Custom(0x08, bytes) => matches!(words(bytes), Ok((0x0001, _))),
        _ => false,
    }
}

fn counts_event(request: &Request<'_>) -> bool {
    matches!(
        request,
        Request::ReadCoils(_, _)
            | Request::ReadDiscreteInputs(_, _)
            | Request::ReadHoldingRegisters(_, _)
            | Request::ReadInputRegisters(_, _)
            | Request::WriteSingleCoil(_, _)
            | Request::WriteSingleRegister(_, _)
            | Request::WriteMultipleCoils(_, _)
            | Request::WriteMultipleRegisters(_, _)
            | Request::MaskWriteRegister(_, _, _)
            | Request::ReadWriteMultipleRegisters(_, _, _, _)
            | Request::ReportServerId
            | Request::Custom(0x14 | 0x15 | 0x18, _)
    )
}

fn push_event(data: &mut ModbusData, event: u8) {
    data.diagnostics.events.push_front(event);
    while data.diagnostics.events.len() > 64 {
        data.diagnostics.events.pop_back();
    }
}

fn event_log(data: &ModbusData) -> Vec<u8> {
    let events: Vec<u8> = data.diagnostics.events.iter().copied().collect();
    let byte_count = 6 + events.len();
    let mut bytes = Vec::with_capacity(1 + byte_count);
    bytes.push(u8::try_from(byte_count).unwrap_or(u8::MAX));
    bytes.extend(0x0000u16.to_be_bytes());
    bytes.extend(data.diagnostics.event_count.to_be_bytes());
    bytes.extend(data.diagnostics.message_count.to_be_bytes());
    bytes.extend(events);
    bytes
}

fn read_fifo(data: &ModbusData, bytes: &[u8]) -> Result<Vec<u8>, ExceptionCode> {
    if bytes.len() != 2 {
        return Err(ExceptionCode::IllegalDataValue);
    }
    let addr = u16::from_be_bytes([bytes[0], bytes[1]]);
    let index = single_index(addr, data.holding_registers.len())?;
    let count = data.holding_registers[index];
    if count > 31 {
        return Err(ExceptionCode::IllegalDataValue);
    }
    let start = index + 1;
    let end = start + usize::from(count);
    if end > data.holding_registers.len() {
        return Err(ExceptionCode::IllegalDataAddress);
    }
    let byte_count = 2 + count * 2;
    let mut response = Vec::with_capacity(2 + usize::from(byte_count));
    response.extend(byte_count.to_be_bytes());
    response.extend(count.to_be_bytes());
    for value in &data.holding_registers[start..end] {
        response.extend(value.to_be_bytes());
    }
    Ok(response)
}

fn read_file(data: &ModbusData, bytes: &[u8]) -> Result<Vec<u8>, ExceptionCode> {
    let groups = parse_read_groups(bytes)?;
    let mut body = Vec::new();
    for group in groups {
        let values = file_values(data, group.file, group.record, group.length)?;
        let sub_len = 1 + values.len() * 2;
        body.push(u8::try_from(sub_len).map_err(|_| ExceptionCode::IllegalDataValue)?);
        body.push(FILE_REFERENCE_TYPE);
        for value in values {
            body.extend(value.to_be_bytes());
        }
    }
    if body.is_empty() || body.len() > 0xF5 {
        return Err(ExceptionCode::IllegalDataValue);
    }
    let mut response = Vec::with_capacity(1 + body.len());
    response.push(u8::try_from(body.len()).unwrap_or(u8::MAX));
    response.extend(body);
    Ok(response)
}

fn write_file(data: &mut ModbusData, bytes: &[u8]) -> Result<Vec<u8>, ExceptionCode> {
    let groups = parse_write_groups(bytes)?;
    for group in &groups {
        let end = u32::from(group.record) + u32::try_from(group.values.len()).unwrap_or(u32::MAX);
        if group.file == 0 || group.record > MAX_FILE_RECORD || end > u32::from(MAX_FILE_RECORD) + 1
        {
            return Err(ExceptionCode::IllegalDataAddress);
        }
        for (offset, value) in group.values.iter().enumerate() {
            let record = group.record + u16::try_from(offset).unwrap_or(u16::MAX);
            data.files.insert((group.file, record), *value);
        }
    }
    Ok(bytes.to_vec())
}

struct ReadGroup {
    file: u16,
    record: u16,
    length: u16,
}

struct WriteGroup {
    file: u16,
    record: u16,
    values: Vec<u16>,
}

fn parse_read_groups(bytes: &[u8]) -> Result<Vec<ReadGroup>, ExceptionCode> {
    if bytes.is_empty() {
        return Err(ExceptionCode::IllegalDataValue);
    }
    let byte_count = usize::from(bytes[0]);
    if byte_count != bytes.len() - 1 || !(0x07..=0xF5).contains(&byte_count) || byte_count % 7 != 0
    {
        return Err(ExceptionCode::IllegalDataValue);
    }
    let mut groups = Vec::new();
    for chunk in bytes[1..].chunks(7) {
        if chunk[0] != FILE_REFERENCE_TYPE {
            return Err(ExceptionCode::IllegalDataAddress);
        }
        groups.push(ReadGroup {
            file: u16::from_be_bytes([chunk[1], chunk[2]]),
            record: u16::from_be_bytes([chunk[3], chunk[4]]),
            length: u16::from_be_bytes([chunk[5], chunk[6]]),
        });
    }
    Ok(groups)
}

fn parse_write_groups(bytes: &[u8]) -> Result<Vec<WriteGroup>, ExceptionCode> {
    if bytes.is_empty() {
        return Err(ExceptionCode::IllegalDataValue);
    }
    let byte_count = usize::from(bytes[0]);
    if byte_count != bytes.len() - 1 || !(0x09..=0xFB).contains(&byte_count) {
        return Err(ExceptionCode::IllegalDataValue);
    }
    let mut groups = Vec::new();
    let mut offset = 1;
    while offset < bytes.len() {
        if offset + 7 > bytes.len() || bytes[offset] != FILE_REFERENCE_TYPE {
            return Err(ExceptionCode::IllegalDataAddress);
        }
        let file = u16::from_be_bytes([bytes[offset + 1], bytes[offset + 2]]);
        let record = u16::from_be_bytes([bytes[offset + 3], bytes[offset + 4]]);
        let length = usize::from(u16::from_be_bytes([bytes[offset + 5], bytes[offset + 6]]));
        let data_at = offset + 7;
        let data_end = data_at + length * 2;
        if length == 0 || data_end > bytes.len() {
            return Err(ExceptionCode::IllegalDataValue);
        }
        let mut values = Vec::with_capacity(length);
        for pair in bytes[data_at..data_end].chunks(2) {
            values.push(u16::from_be_bytes([pair[0], pair[1]]));
        }
        groups.push(WriteGroup {
            file,
            record,
            values,
        });
        offset = data_end;
    }
    Ok(groups)
}

fn file_values(
    data: &ModbusData,
    file: u16,
    record: u16,
    length: u16,
) -> Result<Vec<u16>, ExceptionCode> {
    if length == 0 {
        return Err(ExceptionCode::IllegalDataValue);
    }
    let end = u32::from(record) + u32::from(length);
    if file == 0 || record > MAX_FILE_RECORD || end > u32::from(MAX_FILE_RECORD) + 1 {
        return Err(ExceptionCode::IllegalDataAddress);
    }
    Ok((0..length)
        .map(|offset| {
            data.files
                .get(&(file, record + offset))
                .copied()
                .unwrap_or(0)
        })
        .collect())
}

/// Build the bytes that follow function code 0x2B for MEI type 14.
pub(crate) fn device_identification_payload(
    read_code: u8,
    object_id: u8,
) -> Result<Vec<u8>, ExceptionCode> {
    let objects = device_objects();
    let selected: Vec<(u8, &'static [u8])> = match read_code {
        0x01..=0x03 => {
            let max = if read_code == 0x01 { 0x02 } else { 0x06 };
            let matched: Vec<_> = objects
                .iter()
                .copied()
                .filter(|(id, _)| *id >= object_id && *id <= max)
                .collect();
            if matched.is_empty() {
                objects.into_iter().filter(|(id, _)| *id <= max).collect()
            } else {
                matched
            }
        }
        0x04 => {
            let object = objects
                .into_iter()
                .find(|(id, _)| *id == object_id)
                .ok_or(ExceptionCode::IllegalDataAddress)?;
            vec![object]
        }
        _ => return Err(ExceptionCode::IllegalDataValue),
    };
    let mut payload = vec![0x0E, read_code, 0x82, 0x00, 0x00, selected.len() as u8];
    for (id, value) in selected {
        payload.push(id);
        payload.push(u8::try_from(value.len()).map_err(|_| ExceptionCode::IllegalDataValue)?);
        payload.extend(value);
    }
    Ok(payload)
}

fn device_objects() -> Vec<(u8, &'static [u8])> {
    vec![
        (0x00, b"mb-cli"),
        (0x01, b"mb"),
        (0x02, env!("CARGO_PKG_VERSION").as_bytes()),
        (0x03, b"https://github.com/wkrettek/mb-cli"),
        (0x04, b"mb-cli"),
        (0x05, b"memory-server"),
        (0x06, b""),
    ]
}

/// A write request changes coils, holding registers, or file records.
pub(crate) fn is_write(request: &Request<'_>) -> bool {
    matches!(
        request,
        Request::WriteSingleCoil(_, _)
            | Request::WriteSingleRegister(_, _)
            | Request::WriteMultipleCoils(_, _)
            | Request::WriteMultipleRegisters(_, _)
            | Request::MaskWriteRegister(_, _, _)
            | Request::ReadWriteMultipleRegisters(_, _, _, _)
            | Request::Custom(0x15, _)
    )
}

/// One verbose line: station, function, start address, quantity, and outcome.
pub(crate) fn summarize(
    station: u8,
    request: &Request<'_>,
    outcome: &Result<Option<Response>, ExceptionCode>,
) -> String {
    let function = request.function_code().value();
    let (addr, qty) = request_span(request);
    let addr = addr
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string());
    let qty = qty
        .map(|value| value.to_string())
        .unwrap_or_else(|| "-".to_string());
    let result = match outcome {
        Ok(Some(_)) => "ok".to_string(),
        Ok(None) => "no-response".to_string(),
        Err(code) => format!("exception 0x{:02X}", u8::from(*code)),
    };
    format!("station={station} fc=0x{function:02X} addr={addr} qty={qty} result={result}")
}

fn request_span(request: &Request<'_>) -> (Option<u16>, Option<u16>) {
    match request {
        Request::ReadCoils(addr, qty)
        | Request::ReadDiscreteInputs(addr, qty)
        | Request::ReadHoldingRegisters(addr, qty)
        | Request::ReadInputRegisters(addr, qty) => (Some(*addr), Some(*qty)),
        Request::WriteSingleCoil(addr, _)
        | Request::WriteSingleRegister(addr, _)
        | Request::MaskWriteRegister(addr, _, _) => (Some(*addr), Some(1)),
        Request::WriteMultipleCoils(addr, values) => {
            (Some(*addr), u16::try_from(values.len()).ok())
        }
        Request::WriteMultipleRegisters(addr, values) => {
            (Some(*addr), u16::try_from(values.len()).ok())
        }
        Request::ReadWriteMultipleRegisters(addr, qty, _, _) => (Some(*addr), Some(*qty)),
        Request::Custom(0x18, bytes) if bytes.len() >= 2 => {
            (Some(u16::from_be_bytes([bytes[0], bytes[1]])), None)
        }
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

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
        assert!(payload.windows(6).any(|window| window == b"mb-cli"));

        let response = handle_request(&mut data, Request::ReportServerId, 7)
            .unwrap()
            .unwrap();
        match response {
            Response::Custom(0x11, bytes) => {
                assert_eq!(
                    bytes.as_ref(),
                    [0x08, 0x07, 0xFF, b'm', b'b', b'-', b'c', b'l', b'i']
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
}
