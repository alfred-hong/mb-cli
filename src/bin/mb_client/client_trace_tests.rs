//! Hex formatting for verbose frames.

use crate::trace::hex_bytes;

#[test]
fn hex_bytes_groups_each_octet() {
    assert_eq!(hex_bytes(&[]), "");
    assert_eq!(hex_bytes(&[0x01, 0x03, 0x00]), "01 03 00");
}
