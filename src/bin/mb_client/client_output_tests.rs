//! Terminal framing for a repeating client scan.

use crate::output::{frame_lines, home_sequence};

#[test]
fn home_sequence_is_empty_for_the_first_sample() {
    assert_eq!(home_sequence(0), "");
    assert_eq!(home_sequence(4), "\r\x1b[4A\x1b[0J");
}

#[test]
fn frame_lines_counts_a_trailing_newline_once() {
    assert_eq!(frame_lines(""), 0);
    assert_eq!(frame_lines("Address  Value\n0        7\n"), 2);
    assert_eq!(frame_lines("only"), 1);
}
