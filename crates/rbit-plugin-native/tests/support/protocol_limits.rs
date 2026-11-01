//! Final compact OS facts only. No execution permit or production supervisor.
use super::{ACK, Harness, identity, limits, send};
use crate::support::{Helper, Leader, Output, deadline};
use nix::sys::signal::Signal;
use rbit_plugin_api::worker::{WORKER_PROTOCOL_VERSION, WorkerMessage};
use rbit_plugin_native::{Phase, ProtocolError, Session, encode_worker};
use std::{
    io::{self, Read, Write},
    os::unix::process::ExitStatusExt,
    time::Duration,
};
use tokio::time::Instant;

const CAP: usize = 16 * 1024;
const CLEANUP_RESERVATION: usize = 256 * 1024;

pub(super) fn partial(input: &mut impl Read, output: &mut impl Write) {
    let ready =
        encode_worker(&WorkerMessage::Ready { protocol_version: WORKER_PROTOCOL_VERSION, identity: identity(), limits: limits() }).unwrap();
    output.write_all(&ready[..2]).unwrap();
    output.flush().unwrap();
    let mut byte = [0; 1];
    input.read_exact(&mut byte).expect("parent must force the partial fixture");
    panic!("partial fixture received unexpected input");
}

pub(super) fn flood(input: &mut impl Read, output: &mut impl Write) {
    let mut start = [0; 1];
    input.read_exact(&mut start).unwrap();
    assert_eq!(start, [b'!']); // Explicit raw-stream mode, not SDK protocol.
    std::thread::scope(|scope| {
        let stdout = scope.spawn(|| -> io::Result<()> {
            let mut stdout = io::stdout().lock();
            for _ in 0..512 {
                stdout.write_all(&[b'o'; 512])?;
            }
            stdout.flush()
        });
        for _ in 0..512 {
            output.write_all(&[b'e'; 512]).unwrap();
        }
        output.flush().unwrap();
        stdout.join().unwrap().unwrap();
    });
    input.read_exact(&mut start).expect("parent must force the raw flood fixture");
    panic!("raw flood fixture received unexpected input");
}

async fn ack(output: &Output) {
    let mut bytes = [0; ACK.len()];
    output.exact(&mut bytes, deadline()).await.unwrap();
    assert_eq!(bytes.as_slice(), ACK);
}

#[tokio::test]
async fn actual_session_partial_ready_times_out_then_term_grace_precedes_kill_and_settlement() {
    let (mut leader, pipes) = Leader::spawn(Helper::PartialReady).unwrap();
    let mut harness = Harness::new();
    ack(&pipes.error).await;
    let mut session = Session::with_limits(identity(), limits().frames(), limits().bytes()).unwrap();
    let end = Instant::now() + Duration::from_millis(300);
    send(&pipes.input, &session.hello().unwrap(), end).await.unwrap();
    let mut prefix = [0; 2];
    pipes.error.exact(&mut prefix, end).await.unwrap();
    let ready =
        encode_worker(&WorkerMessage::Ready { protocol_version: WORKER_PROTOCOL_VERSION, identity: identity(), limits: limits() }).unwrap();
    let exact_prefix = prefix == ready[..2];
    let progress = session.feed(&prefix).unwrap();
    assert_eq!(progress.consumed, 2);
    assert!(progress.message.is_none());
    assert_eq!(session.phase(), Phase::AwaitReady);
    let mut byte = [0; 1];
    let stalled = pipes.error.read(&mut byte, end).await;
    let stalled_deadline = Instant::now() >= end;
    session.fail();
    let term = leader.terminate_group();
    let grace = Instant::now() + Duration::from_millis(300);
    // Absolutely no wait/try_wait between TERM and force-before-reap.
    let during_grace = pipes.error.read(&mut byte, grace).await;
    let grace_elapsed = Instant::now() >= grace;
    let (status, group) = leader.force_and_reap().await.unwrap();
    group.absent().await.unwrap();
    harness.drain(&pipes.output).await.unwrap();
    pipes.error.eof().await.unwrap();
    assert!(exact_prefix);
    assert!(matches!(stalled, Err(ref error) if error.kind() == io::ErrorKind::TimedOut));
    assert!(stalled_deadline);
    term.unwrap();
    assert!(matches!(during_grace, Err(ref error) if error.kind() == io::ErrorKind::TimedOut));
    assert!(grace_elapsed);
    assert_eq!(status.signal(), Some(Signal::SIGKILL as i32));
    assert_eq!(session.phase(), Phase::Failed);
    assert_eq!(session.ping().unwrap_err(), ProtocolError::Terminal);
}

#[derive(Debug, thiserror::Error)]
enum DrainFailure {
    #[error("raw stream cap reached")]
    Cap,
    #[error("raw stream IO failed")]
    Io(#[from] io::Error),
}
/// Total reservation/deadline established before the raw writer start byte.
/// Charge before await so dropped futures/unknown transferred bytes never reset it.
#[derive(Debug)]
struct Drain {
    charged: usize,
    observed: usize,
    end: Instant,
    total: usize,
}
impl Drain {
    fn new() -> Self {
        Self { charged: 0, observed: 0, end: Instant::now() + Duration::from_secs(20), total: CAP + CLEANUP_RESERVATION }
    }
    async fn drain_to_eof(&mut self, output: &Output, ceiling: usize, phase_end: Instant) -> Result<(), DrainFailure> {
        let ceiling = ceiling.min(self.total);
        let mut bytes = [0; 512];
        loop {
            let remaining = (ceiling + 1).saturating_sub(self.charged);
            if remaining == 0 {
                return Err(DrainFailure::Cap);
            }
            let request = remaining.min(bytes.len());
            self.charged += request;
            let count = output.read(&mut bytes[..request], phase_end.min(self.end)).await?;
            self.charged -= request - count;
            self.observed += count;
            if count == 0 {
                return Ok(());
            }
            if self.charged > ceiling {
                return Err(DrainFailure::Cap);
            }
        }
    }
}

#[tokio::test]
async fn both_raw_streams_hit_finite_caps_then_reserved_cleanup_reaches_both_eofs() {
    let (mut leader, pipes) = Leader::spawn(Helper::Flood).unwrap();
    ack(&pipes.error).await;
    let mut stdout = Drain::new();
    let mut stderr = Drain::new();
    let end = deadline();
    send(&pipes.input, b"!", end).await.unwrap();
    // No try_join: BOTH actual independent streams must reach their own cap.
    let (out, err) = tokio::join!(stdout.drain_to_eof(&pipes.output, CAP, end), stderr.drain_to_eof(&pipes.error, CAP, end));
    let refused = matches!(out, Err(DrainFailure::Cap)) && matches!(err, Err(DrainFailure::Cap));
    let both_observed = stdout.observed > CAP && stderr.observed > CAP;
    let (status, group) = leader.force_and_reap().await.unwrap();
    group.absent().await.unwrap();
    // Upfront reservation, original counters and original absolute deadlines.
    // Exceeding it is unresolved cleanup; no OS pipe-size bound is assumed.
    let cleanup = CAP + CLEANUP_RESERVATION;
    let out_end = stdout.end;
    let err_end = stderr.end;
    let (out, err) =
        tokio::join!(stdout.drain_to_eof(&pipes.output, cleanup, out_end), stderr.drain_to_eof(&pipes.error, cleanup, err_end));
    out.unwrap();
    err.unwrap();
    assert!(refused, "timeout/EOF/one noisy stream cannot substitute for both cap refusals");
    assert!(both_observed);
    assert_eq!(status.signal(), Some(Signal::SIGKILL as i32));
    assert!(stdout.charged <= cleanup + 1);
    assert!(stderr.charged <= cleanup + 1);
}
