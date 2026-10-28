//! Retry policy, scan window, and framed-result classification.

use std::time::Duration;

use crate::cli::ReadTiming;
use crate::session::{
    AttemptError, DEFAULT_RETRIES, ReadWindow, each_read, handle_modbus_response_with_timeout,
    log_record, present_sample, utc_stamp, with_retries,
};
use tokio_modbus::prelude::ExceptionCode;

#[test]
fn test_handle_modbus_response_with_timeout_success() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let success_result: Result<
            Result<Result<Vec<bool>, ExceptionCode>, tokio_modbus::Error>,
            tokio::time::error::Elapsed,
        > = Ok(Ok(Ok([true, false, true].to_vec())));

        let result = handle_modbus_response_with_timeout(success_result, "test operation", 5).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), [true, false, true]);
    });
}

#[test]
fn test_handle_modbus_response_with_timeout_exception() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let exception_result: Result<
            Result<Result<Vec<bool>, ExceptionCode>, tokio_modbus::Error>,
            tokio::time::error::Elapsed,
        > = Ok(Ok(Err(ExceptionCode::IllegalDataAddress)));

        let result =
            handle_modbus_response_with_timeout(exception_result, "test operation", 5).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Modbus exception"));
    });
}

#[tokio::test]
async fn test_handle_modbus_response_with_timeout_elapsed() {
    // Simulate a timeout by creating an Elapsed error
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct TimeoutFuture;
    impl Future for TimeoutFuture {
        type Output = Result<Result<Vec<bool>, ExceptionCode>, tokio_modbus::Error>;
        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
            Poll::Pending // Never completes
        }
    }

    let result = tokio::time::timeout(Duration::from_millis(1), TimeoutFuture).await;

    let timeout_result = handle_modbus_response_with_timeout(result, "test operation", 5).await;
    assert!(timeout_result.is_err());
    assert!(
        timeout_result
            .unwrap_err()
            .to_string()
            .contains("timed out")
    );
}

#[test]
fn test_handle_modbus_response_with_timeout_modbus_error() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let io_error =
            std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "connection failed");
        let modbus_error_result: Result<
            Result<Result<Vec<bool>, ExceptionCode>, tokio_modbus::Error>,
            tokio::time::error::Elapsed,
        > = Ok(Err(tokio_modbus::Error::Transport(io_error)));

        let result =
            handle_modbus_response_with_timeout(modbus_error_result, "test operation", 5).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("connection failed")
        );
    });
}

#[test]
fn read_window_follows_the_timeout_flag() {
    assert_eq!(ReadWindow::from_seconds(None), ReadWindow::Once);
    assert_eq!(
        ReadWindow::from_seconds(Some(10)),
        ReadWindow::For(Duration::from_secs(10))
    );
    assert_eq!(ReadWindow::from_seconds(Some(0)), ReadWindow::Forever);
    assert_eq!(ReadWindow::from_seconds(Some(-3)), ReadWindow::Forever);
    let finite = ReadWindow::For(Duration::from_secs(10));
    assert!(finite.allows(Duration::from_secs(9), 9));
    assert!(!finite.allows(Duration::from_secs(10), 10));
    assert!(ReadWindow::Once.allows(Duration::ZERO, 0));
    assert!(!ReadWindow::Once.allows(Duration::ZERO, 1));
}

#[tokio::test]
async fn omitted_timeout_reads_once() {
    let mut count = 0;
    let timing = ReadTiming {
        timeout: None,
        interval_ms: 1000,
        log: None,
    };
    each_read(&timing, async || {
        count += 1;
        Ok(String::from("sample\n"))
    })
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn a_scan_keeps_an_error_and_counts_valid_replies() {
    let err = present_sample(
        ReadWindow::For(Duration::from_secs(2)),
        Err(anyhow::anyhow!("no reply")),
        2,
        1,
    );
    let frame = err.expect("scan continues");
    assert!(frame.contains("no reply"), "{frame}");
    assert!(frame.contains("requests=2 valid=1"), "{frame}");

    let stopped = present_sample(ReadWindow::Once, Err(anyhow::anyhow!("no reply")), 1, 0);
    assert!(stopped.is_err());
}

#[test]
fn log_record_is_one_line() {
    let line = log_record("2026-10-28T00:00:00Z", "Read 1\n0        7\n");
    assert_eq!(line, "2026-10-28T00:00:00Z\tRead 1 | 0        7");
    assert!(!line.contains('\n'));
}

#[test]
fn utc_stamp_formats_the_epoch() {
    assert_eq!(utc_stamp(0), "1970-01-01T00:00:00Z");
    assert_eq!(utc_stamp(86_400), "1970-01-02T00:00:00Z");
}

#[tokio::test]
async fn transport_failure_retries_twice_then_stops() {
    let mut calls = 0;
    let err = with_retries(DEFAULT_RETRIES, async || {
        calls += 1;
        Err::<(), _>(AttemptError::retry("no link"))
    })
    .await
    .unwrap_err();
    assert_eq!(calls, 3);
    assert!(err.to_string().contains("no link"), "{err}");
}

#[tokio::test]
async fn modbus_exception_is_not_retried() {
    let mut calls = 0;
    let err = with_retries(DEFAULT_RETRIES, async || {
        calls += 1;
        Err::<(), _>(AttemptError::stop(
            "Modbus exception response: IllegalDataAddress",
        ))
    })
    .await
    .unwrap_err();
    assert_eq!(calls, 1);
    assert!(err.to_string().contains("IllegalDataAddress"), "{err}");
}

#[tokio::test]
async fn a_later_attempt_can_succeed() {
    let mut calls = 0;
    let value = with_retries(DEFAULT_RETRIES, async || {
        calls += 1;
        if calls < 3 {
            Err(AttemptError::retry("down"))
        } else {
            Ok(7)
        }
    })
    .await
    .unwrap();
    assert_eq!((calls, value), (3, 7));
}

// Test timeout configuration ranges
#[test]
fn test_timeout_duration_creation() {
    let duration1 = Duration::from_secs(1);
    assert_eq!(duration1.as_secs(), 1);

    let duration30 = Duration::from_secs(30);
    assert_eq!(duration30.as_secs(), 30);

    let duration_max = Duration::from_secs(u64::MAX);
    assert_eq!(duration_max.as_secs(), u64::MAX);
}
