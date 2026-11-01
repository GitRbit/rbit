//! Explicit test-only channel reversal: stdin SDK requests, stderr SDK replies,
//! stdout bounded libtest text. Never scan harness noise for a frame prefix.
use super::support::{Helper, Input, Leader, Output, Pipes, deadline};
use rbit_plugin_api::worker::{HostMessage, WORKER_PROTOCOL_VERSION, WorkerIdentity, WorkerLimits, WorkerMessage};
use rbit_plugin_native::{Decoder, Phase, ProtocolError, Session, encode_worker};
use std::{
    io::{self, Read, Write},
    time::Duration,
};
use tokio::time::Instant;

const ACK: &[u8] = b"RBIT_FIXED_PROTOCOL_V1\n";
const HARNESS_LIMIT: usize = 16 * 1024;

fn identity() -> WorkerIdentity {
    serde_json::from_value(serde_json::json!({
        "package":{"plugin_id":"unknown.pipe-probe","version":"1.0.0+fixture","archive_digest":"a".repeat(64)},
        "epoch":"b".repeat(64),"host_policy":"c".repeat(64),
        "selection_revision":"1","review_revision":"2","host_instance":"d".repeat(64),
        "generation":1,"launch_nonce":"e".repeat(64)
    }))
    .unwrap()
}
fn limits() -> WorkerLimits {
    WorkerLimits::new(8, 16 * 1024).unwrap()
}

/// Exact ignored dispatch only. Blocking IO here has no autonomous deadline;
/// the actual parent owns pipe deadlines and process cleanup.
#[test]
#[ignore = "fixed child fixture; only its exact owned parent dispatch is valid"]
fn protocol_child() {
    let mode = std::env::var("RBIT_FIXED_PROTOCOL_MODE").expect("fixed fixture mode missing");
    assert!(matches!(mode.as_str(), "good" | "bad_ready"));
    let mut output = io::stderr().lock();
    output.write_all(ACK).unwrap();
    output.flush().unwrap();
    let mut input = io::stdin().lock();
    assert_eq!(
        child_request(&mut input),
        HostMessage::Hello { protocol_version: WORKER_PROTOCOL_VERSION, identity: identity(), limits: limits() }
    );
    let mut ready = identity();
    if mode == "bad_ready" {
        ready.launch_nonce = serde_json::from_value(serde_json::json!("f".repeat(64))).unwrap();
    }
    child_reply(&mut output, &WorkerMessage::Ready { protocol_version: WORKER_PROTOCOL_VERSION, identity: ready, limits: limits() });
    if mode == "bad_ready" {
        let mut byte = [0; 1];
        input.read_exact(&mut byte).expect("parent must force the mismatched fixture");
        panic!("bad Ready fixture received unexpected input");
    }
    let HostMessage::Ping { sequence } = child_request(&mut input) else {
        panic!("expected one Ping");
    };
    assert_eq!(sequence.get(), 1);
    child_reply(&mut output, &WorkerMessage::Pong { sequence });
    assert_eq!(child_request(&mut input), HostMessage::Stop {});
    child_reply(&mut output, &WorkerMessage::Stopped {});
}
fn child_request(input: &mut impl Read) -> HostMessage {
    let mut decoder = Decoder::<HostMessage>::new();
    loop {
        let mut byte = [0; 1];
        input.read_exact(&mut byte).unwrap();
        let progress = decoder.feed(&byte).unwrap();
        assert_eq!(progress.consumed, 1);
        if let Some(message) = progress.message {
            return message;
        }
    }
}
fn child_reply(output: &mut impl Write, message: &WorkerMessage) {
    output.write_all(&encode_worker(message).unwrap()).unwrap();
    output.flush().unwrap();
}

#[derive(Debug, thiserror::Error)]
enum Failure {
    #[error("pipe fixture IO failure")]
    Io(#[from] io::Error),
    #[error("pipe fixture protocol failure")]
    Protocol(#[from] ProtocolError),
    #[error("pipe fixture unexpected data")]
    Unexpected,
}

/// Counts reserved read work too: cancellation/timeout after a syscall cannot
/// reset or undercount already consumed harness bytes. One EOF probe is allowed.
#[derive(Debug)]
struct Harness {
    charged: usize,
    end: Instant,
}
impl Harness {
    fn new() -> Self {
        Self { charged: 0, end: Instant::now() + Duration::from_secs(20) }
    }
    async fn drain(&mut self, output: &Output) -> io::Result<()> {
        let mut bytes = [0; 512];
        loop {
            let available = (HARNESS_LIMIT + 1).saturating_sub(self.charged);
            if available == 0 {
                return Err(io::Error::other("harness stdout quota"));
            }
            let requested = bytes.len().min(available);
            self.charged += requested;
            let count = output.read(&mut bytes[..requested], self.end).await?;
            self.charged -= requested - count;
            if count == 0 {
                return Ok(());
            }
            if self.charged > HARNESS_LIMIT {
                return Err(io::Error::other("harness stdout quota"));
            }
        }
    }
}

async fn send(input: &Input, bytes: &[u8], end: Instant) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        let count = input.write(&bytes[offset..], end).await?;
        if count == 0 {
            return Err(io::ErrorKind::WriteZero.into());
        }
        offset += count;
    }
    Ok(())
}
async fn receive(output: &Output, session: &mut Session, expected: Phase, end: Instant) -> Result<(), Failure> {
    let mut bytes = [0; 256];
    loop {
        let count = output.read(&mut bytes, end).await?;
        if count == 0 {
            return Err(Failure::Unexpected);
        }
        let progress = session.feed(&bytes[..count])?;
        if progress.consumed != count {
            return Err(Failure::Unexpected);
        }
        if progress.message.is_some() {
            return if session.phase() == expected { Ok(()) } else { Err(Failure::Unexpected) };
        }
    }
}
async fn conversation(pipes: &Pipes, session: &mut Session) -> Result<(), Failure> {
    let end = deadline();
    let mut ack = [0; ACK.len()];
    pipes.error.exact(&mut ack, end).await?;
    if ack.as_slice() != ACK {
        return Err(Failure::Unexpected);
    }
    send(&pipes.input, &session.hello()?, end).await?;
    receive(&pipes.error, session, Phase::Ready, end).await?;
    let end = deadline();
    send(&pipes.input, &session.ping()?, end).await?;
    receive(&pipes.error, session, Phase::Ready, end).await?;
    let end = deadline();
    send(&pipes.input, &session.stop()?, end).await?;
    receive(&pipes.error, session, Phase::Stopped, end).await?;
    let mut byte = [0; 1];
    if pipes.error.read(&mut byte, end).await? != 0 {
        return Err(Failure::Unexpected);
    }
    session.finish()?;
    Ok(())
}
async fn exchange(pipes: &Pipes, session: &mut Session, harness: &mut Harness) -> Result<(), Failure> {
    tokio::try_join!(conversation(pipes, session), async { harness.drain(&pipes.output).await.map_err(Failure::Io) })?;
    Ok(())
}

#[tokio::test]
async fn exact_child_negotiates_ping_and_stop_then_both_eofs_before_graceful_reap() {
    let (mut leader, pipes) = Leader::spawn(Helper::Protocol).unwrap();
    let mut session = Session::with_limits(identity(), limits().frames(), limits().bytes()).unwrap();
    let mut harness = Harness::new();
    let result = exchange(&pipes, &mut session, &mut harness).await;
    if let Err(error) = result {
        session.fail();
        let (_status, group) = leader.force_and_reap().await.unwrap();
        group.absent().await.unwrap();
        // Continue with the SAME charged quota/deadline after try_join cancellation.
        harness.drain(&pipes.output).await.unwrap();
        panic!("fixed protocol exchange failed: {error}");
    }
    assert_eq!(session.phase(), Phase::Stopped);
    let (status, group) = leader.reap_owned().await.unwrap();
    assert!(status.success());
    group.absent().await.unwrap();
}

#[tokio::test]
async fn mismatched_ready_fails_terminally_and_real_owned_group_is_settled() {
    let (mut leader, pipes) = Leader::spawn(Helper::BadReady).unwrap();
    let mut session = Session::with_limits(identity(), limits().frames(), limits().bytes()).unwrap();
    let mut harness = Harness::new();
    let result = exchange(&pipes, &mut session, &mut harness).await;
    let expected = matches!(result, Err(Failure::Protocol(ProtocolError::Identity)));
    session.fail();
    let (_status, group) = leader.force_and_reap().await.unwrap();
    group.absent().await.unwrap();
    harness.drain(&pipes.output).await.unwrap();
    pipes.error.eof().await.unwrap();
    assert!(expected, "wrong dispatch/noise/timeout must not masquerade as identity rejection");
    assert_eq!(session.phase(), Phase::Failed);
    assert_eq!(session.ping().unwrap_err(), ProtocolError::Terminal);
}
