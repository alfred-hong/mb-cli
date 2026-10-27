use crate::cli::{DecodeArgs, DecodeFormat, RegisterOrder};
use crate::table::{write_register_table, AddressStyle};
use std::fmt::Write as _;

/// One decoded value and the PDU address of its first register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedRow {
    pub address: u32,
    pub value: String,
}

/// Registers consumed by one value of this format.
pub fn width(format: DecodeFormat) -> usize {
    match format {
        DecodeFormat::Bin
        | DecodeFormat::Hex
        | DecodeFormat::Ansi
        | DecodeFormat::U16
        | DecodeFormat::I16 => 1,
        DecodeFormat::U32 | DecodeFormat::I32 | DecodeFormat::F32 => 2,
        DecodeFormat::U64 | DecodeFormat::I64 | DecodeFormat::F64 => 4,
    }
}

/// Decode complete groups. The leftover count is the trailing registers that do not fill a value.
pub fn decode_rows(
    words: &[u16],
    start: u16,
    format: DecodeFormat,
    order: RegisterOrder,
) -> (Vec<DecodedRow>, usize) {
    let step = width(format);
    let groups = words.len() / step;
    let mut rows = Vec::with_capacity(groups);
    for index in 0..groups {
        let chunk = &words[index * step..(index + 1) * step];
        let address = u32::from(start) + (index * step) as u32;
        rows.push(DecodedRow {
            address,
            value: render(chunk, format, order),
        });
    }
    (rows, words.len() % step)
}

/// `--swap-bytes` and `--order lsrf` describe a decoded value, so they need `--decode`.
pub fn ensure_decode_flags(options: &DecodeArgs) -> anyhow::Result<()> {
    if !options.formats.is_empty() {
        return Ok(());
    }
    match (options.swap_bytes, options.order) {
        (true, RegisterOrder::Lsrf) => {
            anyhow::bail!("--swap-bytes and --order lsrf require --decode");
        }
        (true, _) => anyhow::bail!("--swap-bytes requires --decode"),
        (false, RegisterOrder::Lsrf) => anyhow::bail!("--order lsrf requires --decode"),
        (false, RegisterOrder::Msrf) => Ok(()),
    }
}

/// Append the raw register table, or only the decoded tables when `--decode` is set.
pub fn write_words(
    out: &mut String,
    words: &[u16],
    start: u16,
    verbose: bool,
    options: &DecodeArgs,
    style: AddressStyle,
) -> anyhow::Result<()> {
    if options.formats.is_empty() {
        write_register_table(out, words, start, verbose, style);
    }
    write_decoded(out, words, start, options, style)
}

/// Print the raw register table, or only the decoded tables when `--decode` is set.
pub fn print_words(
    words: &[u16],
    start: u16,
    verbose: bool,
    options: &DecodeArgs,
    style: AddressStyle,
) -> anyhow::Result<()> {
    let mut out = String::new();
    write_words(&mut out, words, start, verbose, options, style)?;
    print!("{out}");
    Ok(())
}

/// Append one table per requested format. Skipped when no format was requested.
pub fn write_decoded(
    out: &mut String,
    words: &[u16],
    start: u16,
    options: &DecodeArgs,
    style: AddressStyle,
) -> anyhow::Result<()> {
    ensure_decode_flags(options)?;
    if options.formats.is_empty() || words.is_empty() {
        return Ok(());
    }
    let prepared = apply_byte_swap(words, options.swap_bytes);
    for format in &options.formats {
        let (rows, leftover) = decode_rows(&prepared, start, *format, options.order);
        let _ = writeln!(out);
        let _ = writeln!(out, "{}", heading(*format, options));
        if rows.is_empty() {
            let _ = writeln!(
                out,
                "{} needs {} registers; got {}.",
                format.name(),
                width(*format),
                prepared.len()
            );
            continue;
        }
        let _ = writeln!(out, "{:<8} Value", "Address");
        let _ = writeln!(out, "{:-<8} {:-<12}", "", "");
        for row in &rows {
            let address = crate::table::format_address(style, row.address);
            let _ = writeln!(out, "{address:<8} {}", row.value);
        }
        if leftover != 0 {
            let _ = writeln!(
                out,
                "{leftover} register(s) remain; {} needs {}.",
                format.name(),
                width(*format)
            );
        }
    }
    Ok(())
}

fn apply_byte_swap(words: &[u16], swap_bytes: bool) -> Vec<u16> {
    if swap_bytes {
        words.iter().map(|word| word.swap_bytes()).collect()
    } else {
        words.to_vec()
    }
}

fn render(chunk: &[u16], format: DecodeFormat, order: RegisterOrder) -> String {
    let least_first = uses_register_order(format) && order == RegisterOrder::Lsrf;
    let bytes = value_bytes(chunk, least_first);
    match format {
        DecodeFormat::Bin => format_bits(&bytes),
        DecodeFormat::Hex => format_hex(&bytes),
        DecodeFormat::Ansi => format_ansi(&bytes),
        DecodeFormat::U16 => integer::<2, u16>(&bytes, u16::from_be_bytes),
        DecodeFormat::I16 => integer::<2, i16>(&bytes, i16::from_be_bytes),
        DecodeFormat::U32 => integer::<4, u32>(&bytes, u32::from_be_bytes),
        DecodeFormat::I32 => integer::<4, i32>(&bytes, i32::from_be_bytes),
        DecodeFormat::U64 => integer::<8, u64>(&bytes, u64::from_be_bytes),
        DecodeFormat::I64 => integer::<8, i64>(&bytes, i64::from_be_bytes),
        DecodeFormat::F32 => format_f32(&bytes),
        DecodeFormat::F64 => format_f64(&bytes),
    }
}

fn uses_register_order(format: DecodeFormat) -> bool {
    width(format) > 1
}

fn value_bytes(chunk: &[u16], least_significant_register_first: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(chunk.len() * 2);
    if least_significant_register_first {
        for register in chunk.iter().rev() {
            bytes.extend(register.to_be_bytes());
        }
    } else {
        for register in chunk {
            bytes.extend(register.to_be_bytes());
        }
    }
    bytes
}

fn integer<const N: usize, T: std::fmt::Display>(
    bytes: &[u8],
    from_be: fn([u8; N]) -> T,
) -> String {
    let array: [u8; N] = bytes
        .try_into()
        .expect("decoded width matches the integer size");
    from_be(array).to_string()
}

fn format_f32(bytes: &[u8]) -> String {
    let array: [u8; 4] = bytes.try_into().expect("f32 uses four bytes");
    format_float(f32::from_be_bytes(array))
}

fn format_f64(bytes: &[u8]) -> String {
    let array: [u8; 8] = bytes.try_into().expect("f64 uses eight bytes");
    format_float(f64::from_be_bytes(array))
}

fn format_float(value: impl std::fmt::Debug) -> String {
    // Display expands subnormal values into a long run of zeros. Debug keeps a short decimal.
    format!("{value:?}")
}

fn format_bits(bytes: &[u8]) -> String {
    let bits: String = bytes.iter().map(|byte| format!("{byte:08b}")).collect();
    bits.as_bytes()
        .chunks(4)
        .map(|chunk| std::str::from_utf8(chunk).expect("bit text is ascii"))
        .collect::<Vec<_>>()
        .join("_")
}

fn format_hex(bytes: &[u8]) -> String {
    let mut text = String::from("0x");
    for byte in bytes {
        text.push_str(&format!("{byte:02X}"));
    }
    text
}

fn format_ansi(bytes: &[u8]) -> String {
    bytes.iter().copied().map(ansi_char).collect()
}

/// Latin-1 for bytes that have a visible character. Controls become `.`.
fn ansi_char(byte: u8) -> char {
    match byte {
        0x20..=0x7E => byte as char,
        0xA0..=0xFF => char::from(byte),
        _ => '.',
    }
}

fn heading(format: DecodeFormat, options: &DecodeArgs) -> String {
    let mut text = format!("Decoded {}", format.name());
    if uses_register_order(format) {
        let order = match options.order {
            RegisterOrder::Msrf => "MSRF",
            RegisterOrder::Lsrf => "LSRF",
        };
        text.push_str(&format!(" ({order}"));
        if options.swap_bytes {
            text.push_str(", bytes swapped");
        }
        text.push(')');
    } else if options.swap_bytes {
        text.push_str(" (bytes swapped)");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(words: &[u16], format: DecodeFormat, order: RegisterOrder, swap: bool) -> String {
        let prepared = apply_byte_swap(words, swap);
        decode_rows(&prepared, 0, format, order).0[0].value.clone()
    }

    #[test]
    fn sixteen_bit_views_and_byte_swap() {
        assert_eq!(
            row(&[0x4142], DecodeFormat::Ansi, RegisterOrder::Msrf, false),
            "AB"
        );
        assert_eq!(
            row(&[0x4142], DecodeFormat::Ansi, RegisterOrder::Msrf, true),
            "BA"
        );
        assert_eq!(
            row(&[0x00FF], DecodeFormat::Hex, RegisterOrder::Msrf, true),
            "0xFF00"
        );
        assert_eq!(
            row(&[0x0001], DecodeFormat::Bin, RegisterOrder::Msrf, false),
            "0000_0000_0000_0001"
        );
        assert_eq!(
            row(&[0xFFFF], DecodeFormat::I16, RegisterOrder::Msrf, false),
            "-1"
        );
        assert_eq!(
            row(&[0xFFFF], DecodeFormat::U16, RegisterOrder::Msrf, false),
            "65535"
        );
    }

    #[test]
    fn register_order_and_byte_swap_combine() {
        let words = [0x1234, 0x5678];
        assert_eq!(
            row(&words, DecodeFormat::U32, RegisterOrder::Msrf, false),
            "305419896"
        );
        assert_eq!(
            row(&words, DecodeFormat::Hex, RegisterOrder::Lsrf, false),
            "0x1234"
        );
        assert_eq!(
            row(&words, DecodeFormat::U32, RegisterOrder::Lsrf, false),
            "1450709556"
        );
        assert_eq!(
            row(&words, DecodeFormat::U32, RegisterOrder::Msrf, true),
            "873625686"
        );
        assert_eq!(
            row(&words, DecodeFormat::U32, RegisterOrder::Lsrf, true),
            "2018915346"
        );
        assert_eq!(
            row(
                &[0xFFFF, 0xFFFF],
                DecodeFormat::I32,
                RegisterOrder::Msrf,
                false
            ),
            "-1"
        );
    }

    #[test]
    fn floats_use_ieee754() {
        let pi = row(
            &[0x4049, 0x0FDB],
            DecodeFormat::F32,
            RegisterOrder::Msrf,
            false,
        );
        assert!(pi.starts_with("3.14159"), "{pi}");
        let (rows, leftover) = decode_rows(&[0x4049], 10, DecodeFormat::F32, RegisterOrder::Msrf);
        assert!(rows.is_empty());
        assert_eq!(leftover, 1);
        let swapped = row(
            &[0x1234, 0x5678, 0x9ABC, 0xDEF0],
            DecodeFormat::U64,
            RegisterOrder::Lsrf,
            true,
        );
        let expected = u64::from_be_bytes([0xF0, 0xDE, 0xBC, 0x9A, 0x78, 0x56, 0x34, 0x12]);
        assert_eq!(swapped, expected.to_string());
    }
}
