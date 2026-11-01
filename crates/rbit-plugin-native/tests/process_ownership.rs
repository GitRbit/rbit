#![cfg(unix)]
mod support;
use rbit_plugin_api::worker::WorkerMessage;
use rbit_plugin_native::{Decoder, encode_worker};
use std::{io, time::Duration};
use support::{Helper, Leader, READY, deadline};
use tokio::time::Instant;

#[tokio::test]
async fn fixed_acknowledged_echo_roundtrips_real_partial_pipe_frames_and_reaps_group() {
    let (mut leader, pipes) = Leader::spawn(Helper::Echo).unwrap();
    pipes.output.ack(READY).await.unwrap();
    let frame = encode_worker(&WorkerMessage::Stopped {}).unwrap();
    let end = deadline();
    for byte in &frame {
        assert_eq!(pipes.input.write(std::slice::from_ref(byte), end).await.unwrap(), 1);
    }
    let mut decoder = Decoder::<WorkerMessage>::new();
    let mut result = None;
    for _ in 0..frame.len() {
        let mut byte = [0; 1];
        pipes.output.exact(&mut byte, end).await.unwrap();
        let progress = decoder.feed(&byte).unwrap();
        assert_eq!(progress.consumed, 1);
        if progress.message.is_some() {
            assert!(result.is_none());
            result = progress.message;
        }
    }
    assert_eq!(result, Some(WorkerMessage::Stopped {}));
    let (_status, group) = leader.force_and_reap().await.unwrap();
    assert!(!leader.is_owned());
    group.absent().await.unwrap();
    pipes.output.eof().await.unwrap();
    pipes.error.eof().await.unwrap();
}

#[tokio::test]
async fn stalled_partial_prefix_and_backpressure_use_absolute_deadlines_without_losing_owner() {
    let (mut leader, pipes) = Leader::spawn(Helper::Blocked).unwrap();
    pipes.output.ack(b"RBIT_BLOCKED_READY\n").await.unwrap();
    let mut prefix = [0; 1];
    pipes.output.exact(&mut prefix, deadline()).await.unwrap();
    let mut decoder = Decoder::<WorkerMessage>::new();
    assert!(decoder.feed(&prefix).unwrap().message.is_none());
    let end = Instant::now() + Duration::from_millis(150);
    let error = pipes.output.read(&mut prefix, end).await.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(Instant::now() >= end);
    assert!(leader.is_owned());
    let end = Instant::now() + Duration::from_millis(150);
    let block = [0u8; 4096];
    let mut written = 0;
    let error = loop {
        assert!(written < 8 * 1024 * 1024, "fixture did not produce bounded backpressure");
        match pipes.input.write(&block, end).await {
            Ok(0) => panic!("fixed pipe unexpectedly accepted a zero write"),
            Ok(count) => {
                written += count;
            }
            Err(error) => break error,
        }
    };
    assert_eq!(error.kind(), io::ErrorKind::TimedOut);
    assert!(written > 0);
    assert!(Instant::now() >= end);
    assert!(leader.is_owned());
    let (_status, group) = leader.force_and_reap().await.unwrap();
    group.absent().await.unwrap();
    pipes.output.eof().await.unwrap();
    pipes.error.eof().await.unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn linux_waitable_exited_leader_keeps_signal_authority_until_retained_pipe_cleanup() {
    let (mut leader, pipes) = Leader::spawn(Helper::ExitFirst).unwrap();
    // Both messages are fixed, but their scheduling order is intentionally free.
    let mut lines = Vec::new();
    let end = deadline();
    for _ in 0..2 {
        let mut line = Vec::new();
        loop {
            assert!(line.len() < 64);
            let mut byte = [0; 1];
            pipes.output.exact(&mut byte, end).await.unwrap();
            line.push(byte[0]);
            if byte[0] == b'\n' {
                break;
            }
        }
        lines.push(line);
    }
    lines.sort();
    assert_eq!(lines, vec![b"RBIT_DESCENDANT_READY\n".to_vec(), b"RBIT_LEADER_EXITING\n".to_vec()]);
    leader.observe_exit_without_reaping().await.unwrap();
    assert!(leader.is_owned());
    let mut byte = [0; 1];
    let error = pipes.output.read(&mut byte, Instant::now() + Duration::from_millis(150)).await.unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::TimedOut); // descendant still owns the pipe
    let (status, group) = leader.force_and_reap().await.unwrap();
    assert_eq!(status.code(), Some(0));
    group.absent().await.unwrap();
    pipes.output.eof().await.unwrap();
    pipes.error.eof().await.unwrap();
}
