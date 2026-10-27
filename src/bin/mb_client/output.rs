//! Paint a read sample. A repeating read redraws the previous sample in place.

use std::fs::File;
use std::io::{self, IsTerminal, Write};

/// Logical lines in a sample, including a last line that has no trailing newline.
pub fn frame_lines(frame: &str) -> usize {
    if frame.is_empty() {
        return 0;
    }
    let mut lines = frame.bytes().filter(|byte| *byte == b'\n').count();
    if !frame.ends_with('\n') {
        lines += 1;
    }
    lines
}

/// ANSI that moves to the first line of the previous sample and clears downward.
pub fn home_sequence(previous_lines: usize) -> String {
    if previous_lines == 0 {
        String::new()
    } else {
        // Cursor sits on the line after the sample. Move up, then erase to the end of the screen.
        format!("\r\x1b[{previous_lines}A\x1b[0J")
    }
}

fn with_newline(frame: &str) -> String {
    if frame.is_empty() || frame.ends_with('\n') {
        frame.to_string()
    } else {
        format!("{frame}\n")
    }
}

/// Draw `frame`, replacing the previous sample when a terminal is available.
pub fn paint(previous_lines: usize, frame: &str) -> io::Result<usize> {
    let body = with_newline(frame);
    let lines = frame_lines(&body);

    if io::stdout().is_terminal() {
        let mut out = io::stdout().lock();
        if previous_lines > 0 {
            write!(out, "{}", home_sequence(previous_lines))?;
        }
        write!(out, "{body}")?;
        out.flush()?;
        return Ok(lines);
    }

    // stdout is redirected. Still refresh an attached terminal when possible.
    if previous_lines > 0 {
        if let Ok(mut tty) = File::options().write(true).open("/dev/tty") {
            write!(tty, "{}{body}", home_sequence(previous_lines))?;
            tty.flush()?;
        }
    } else if let Ok(mut tty) = File::options().write(true).open("/dev/tty") {
        write!(tty, "{body}")?;
        tty.flush()?;
    }

    let mut out = io::stdout().lock();
    write!(out, "{body}")?;
    out.flush()?;
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
