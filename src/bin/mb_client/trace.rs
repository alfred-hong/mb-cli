//! Copy bytes that pass through a connection. `--verbose` prints them as ModScan shows traffic.

use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Stream wrapper that can print each successful read and write.
pub struct Trace<T> {
    inner: T,
    enabled: bool,
}

impl<T> Trace<T> {
    pub fn new(inner: T, enabled: bool) -> Self {
        Self { inner, enabled }
    }
}

impl<T> fmt::Debug for Trace<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Trace")
            .field("enabled", &self.enabled)
            .finish()
    }
}

/// Uppercase hex with a space between bytes.
pub fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().saturating_mul(3));
    for (index, byte) in bytes.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

impl<T: AsyncRead + Unpin> AsyncRead for Trace<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let result = Pin::new(&mut this.inner).poll_read(cx, buf);
        if this.enabled {
            if let Poll::Ready(Ok(())) = &result {
                let filled = buf.filled();
                if filled.len() > before {
                    eprintln!("rx {}", hex_bytes(&filled[before..]));
                }
            }
        }
        result
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for Trace<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();
        let result = Pin::new(&mut this.inner).poll_write(cx, buf);
        if this.enabled {
            if let Poll::Ready(Ok(size)) = &result {
                if *size > 0 {
                    eprintln!("tx {}", hex_bytes(&buf[..*size]));
                }
            }
        }
        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
