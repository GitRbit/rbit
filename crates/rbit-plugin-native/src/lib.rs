//! Pure incremental native-worker framing and host-side session validation.
//!
//! No I/O, timers, processes, executable authorization or host calls. The caller
//! supplies bounded slices and owns transport/backpressure. Limits are finite
//! session work budgets, NOT frames/second or an OS pipe deadline guarantee.
//! A later owned cancellable async transport must enforce actual absolute
//! deadlines/rate admission and terminate/reap workers on protocol failure.
use rbit_plugin_api::PolicyRevision;
use rbit_plugin_api::worker::{
    HostMessage, MAX_WORKER_FRAME_BYTES, MAX_WORKER_JSON_DEPTH, WORKER_PROTOCOL_VERSION, WorkerIdentity, WorkerLimits, WorkerMessage,
};
use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("invalid bounded worker frame length")]
    Length,
    #[error("worker JSON depth limit exceeded")]
    Depth,
    #[error("invalid canonical worker message")]
    Message,
    #[error("unexpected worker session message")]
    State,
    #[error("worker session identity mismatch")]
    Identity,
    #[error("worker session work budget exhausted")]
    Budget,
    #[error("worker session sequence exhausted or mismatched")]
    Sequence,
    #[error("truncated worker frame")]
    Truncated,
    #[error("worker reported failure")]
    Worker,
    #[error("worker session is terminal")]
    Terminal,
}

mod sealed {
    pub trait Message {}
    impl Message for rbit_plugin_api::worker::HostMessage {}
    impl Message for rbit_plugin_api::worker::WorkerMessage {}
}
/// Sealed to the two bounded SDK message enums; arbitrary allocating custom
/// deserializers cannot be installed in this public decoder.
pub trait WireMessage: sealed::Message + DeserializeOwned + Serialize {}
impl WireMessage for HostMessage {}
impl WireMessage for WorkerMessage {}

/// One feed consumes at most one frame; unconsumed bytes remain caller-owned.
#[derive(Debug)]
pub struct Progress<T> {
    pub consumed: usize,
    pub message: Option<T>,
}

/// Holds at most one bounded body. All errors permanently poison this decoder.
#[derive(Debug)]
pub struct Decoder<T> {
    prefix: [u8; 4],
    prefix_len: usize,
    expected: usize,
    body: Vec<u8>,
    failed: bool,
    message_type: std::marker::PhantomData<fn() -> T>,
}
impl<T: WireMessage> Default for Decoder<T> {
    fn default() -> Self {
        Self::new()
    }
}
impl<T: WireMessage> Decoder<T> {
    pub fn new() -> Self {
        Self { prefix: [0; 4], prefix_len: 0, expected: 0, body: Vec::new(), failed: false, message_type: std::marker::PhantomData }
    }
    pub fn feed(&mut self, input: &[u8]) -> Result<Progress<T>, ProtocolError> {
        self.feed_budget(input, usize::MAX)
    }
    fn feed_budget(&mut self, input: &[u8], budget: usize) -> Result<Progress<T>, ProtocolError> {
        if self.failed {
            return Err(ProtocolError::Terminal);
        }
        let input = &input[..input.len().min(budget)];
        let result = self.feed_inner(input, budget);
        if result.is_err() {
            self.failed = true;
            self.body.clear();
        }
        result
    }
    fn feed_inner(&mut self, input: &[u8], budget: usize) -> Result<Progress<T>, ProtocolError> {
        let mut consumed = 0;
        while self.prefix_len < 4 && consumed < input.len() {
            self.prefix[self.prefix_len] = input[consumed];
            self.prefix_len += 1;
            consumed += 1;
        }
        if self.prefix_len < 4 {
            return Ok(Progress { consumed, message: None });
        }
        if self.expected == 0 {
            self.expected = u32::from_be_bytes(self.prefix) as usize;
            if self.expected == 0 || self.expected > MAX_WORKER_FRAME_BYTES {
                return Err(ProtocolError::Length);
            }
        }
        // Reject a frame that cannot fit the remaining session wire budget before
        // reserving its body, even if this feed provides only its prefix.
        if self.expected - self.body.len() > budget - consumed {
            return Err(ProtocolError::Budget);
        }
        if self.body.capacity() < self.expected {
            self.body.reserve_exact(self.expected - self.body.len());
        }
        let count = (self.expected - self.body.len()).min(input.len() - consumed);
        self.body.extend_from_slice(&input[consumed..consumed + count]);
        consumed += count;
        if self.body.len() < self.expected {
            return Ok(Progress { consumed, message: None });
        }
        let message = canonical(&self.body)?;
        self.prefix_len = 0;
        self.expected = 0;
        self.body.clear();
        Ok(Progress { consumed, message: Some(message) })
    }
    /// Transport EOF is always terminal. Even a clean frame boundary does not
    /// establish graceful Stop/Stopped; that belongs to the session state.
    pub fn finish(&mut self) -> Result<(), ProtocolError> {
        if self.failed {
            return Err(ProtocolError::Terminal);
        }
        self.failed = true;
        if self.prefix_len != 0 || !self.body.is_empty() { Err(ProtocolError::Truncated) } else { Ok(()) }
    }
}

fn canonical<T: DeserializeOwned + Serialize>(body: &[u8]) -> Result<T, ProtocolError> {
    depth(body)?;
    let value: T = serde_json::from_slice(body).map_err(|_| ProtocolError::Message)?;
    // Deterministic profile: whitespace, field order, escapes and numeric spelling
    // must match the SDK encoder; unknown/duplicate fields never normalize away.
    if bounded_json(&value)? != body {
        return Err(ProtocolError::Message);
    }
    Ok(value)
}
fn depth(body: &[u8]) -> Result<(), ProtocolError> {
    let (mut nesting, mut string, mut escape) = (0usize, false, false);
    for &byte in body {
        if string {
            if escape {
                escape = false;
            } else if byte == b'\\' {
                escape = true;
            } else if byte == b'"' {
                string = false;
            }
        } else {
            match byte {
                b'"' => string = true,
                b'{' | b'[' => {
                    nesting += 1;
                    if nesting > MAX_WORKER_JSON_DEPTH {
                        return Err(ProtocolError::Depth);
                    }
                }
                b'}' | b']' => {
                    nesting = nesting.checked_sub(1).ok_or(ProtocolError::Message)?;
                }
                _ => {}
            }
        }
    }
    Ok(()) // serde validates matching delimiters, strings and UTF-8.
}
/// Bounded pure worker-side encoding. This grants no host-call authority.
pub fn encode_worker(message: &WorkerMessage) -> Result<Vec<u8>, ProtocolError> {
    frame(message)
}
/// Bounded pure host-side encoding; a launch coordinator must separately enforce
/// identity, policy and session sequencing. Prefer Session for host conversations.
pub fn encode_host(message: &HostMessage) -> Result<Vec<u8>, ProtocolError> {
    frame(message)
}
// serde's Write target is a pure bounded memory sink, never an OS I/O adapter.
struct JsonBuffer {
    bytes: Vec<u8>,
    exhausted: bool,
    limit: usize,
}
impl std::io::Write for JsonBuffer {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        let Some(required) = self.bytes.len().checked_add(input.len()).filter(|n| *n <= self.limit) else {
            self.exhausted = true;
            return Err(std::io::Error::other("worker frame length"));
        };
        if required > self.bytes.capacity() {
            self.bytes.reserve_exact(required - self.bytes.len());
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn bounded_json<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    bounded_json_limit(value, MAX_WORKER_FRAME_BYTES)
}
fn bounded_json_limit<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, ProtocolError> {
    let mut output = JsonBuffer { bytes: Vec::new(), exhausted: false, limit };
    let result = serde_json::to_writer(&mut output, value);
    if output.exhausted {
        return Err(ProtocolError::Length);
    }
    result.map_err(|_| ProtocolError::Message)?;
    Ok(output.bytes)
}
fn frame<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtocolError> {
    frame_limit(value, MAX_WORKER_FRAME_BYTES + 4)
}
fn frame_limit<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, ProtocolError> {
    // Wire variants contain only existing bounded identifiers and fixed enums;
    // callers cannot supply an unbounded message String/vector.
    let body = bounded_json_limit(value, limit.saturating_sub(4).min(MAX_WORKER_FRAME_BYTES))?;
    if body.is_empty() || body.len() > MAX_WORKER_FRAME_BYTES {
        return Err(ProtocolError::Length);
    }
    let length = u32::try_from(body.len()).map_err(|_| ProtocolError::Length)?;
    let mut output = Vec::with_capacity(body.len() + 4);
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(&body);
    Ok(output)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    New,
    AwaitReady,
    Ready,
    AwaitPong,
    Stopping,
    Stopped,
    Failed,
}

/// Passive host-side protocol machine. Construction performs no admission check
/// and MUST NOT be interpreted as an executable permit. No durable side effects.
#[derive(Debug)]
pub struct Session {
    identity: WorkerIdentity,
    decoder: Decoder<WorkerMessage>,
    phase: Phase,
    sequence: u64,
    frames: usize,
    bytes: usize,
    limits: WorkerLimits,
}
impl Session {
    pub fn new(identity: WorkerIdentity) -> Self {
        Self { identity, decoder: Decoder::new(), phase: Phase::New, sequence: 0, frames: 0, bytes: 0, limits: WorkerLimits::default() }
    }
    /// Host-selected shared finite work caps, advertised in Hello and echoed by
    /// Ready; never a grant or a time-based rate.
    pub fn with_limits(identity: WorkerIdentity, frames: usize, bytes: usize) -> Result<Self, ProtocolError> {
        let limits = WorkerLimits::new(frames, bytes).map_err(|_| ProtocolError::Budget)?;
        let mut session = Self::new(identity);
        session.limits = limits;
        Ok(session)
    }
    pub fn limits(&self) -> WorkerLimits {
        self.limits
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }
    fn active(&self) -> Result<(), ProtocolError> {
        if matches!(self.phase, Phase::Stopped | Phase::Failed) { Err(ProtocolError::Terminal) } else { Ok(()) }
    }
    fn account(&mut self, bytes: usize, frame: bool) -> Result<(), ProtocolError> {
        let count = self.frames.checked_add(usize::from(frame)).ok_or(ProtocolError::Budget)?;
        let total = self.bytes.checked_add(bytes).ok_or(ProtocolError::Budget)?;
        if count > self.limits.frames() || total > self.limits.bytes() {
            return Err(ProtocolError::Budget);
        }
        self.frames = count;
        self.bytes = total;
        Ok(())
    }
    fn result<T>(&mut self, result: Result<T, ProtocolError>) -> Result<T, ProtocolError> {
        if result.is_err() {
            self.phase = Phase::Failed;
        }
        result
    }
    pub fn hello(&mut self) -> Result<Vec<u8>, ProtocolError> {
        self.send(
            &HostMessage::Hello { protocol_version: WORKER_PROTOCOL_VERSION, identity: self.identity.clone(), limits: self.limits },
            Phase::New,
            Phase::AwaitReady,
        )
    }
    pub fn ping(&mut self) -> Result<Vec<u8>, ProtocolError> {
        let Some(next) = self.sequence.checked_add(1).and_then(|v| PolicyRevision::new(v).ok()) else {
            self.phase = Phase::Failed;
            return Err(ProtocolError::Sequence);
        };
        let output = self.send(&HostMessage::Ping { sequence: next }, Phase::Ready, Phase::AwaitPong)?;
        self.sequence = next.get();
        Ok(output)
    }
    pub fn stop(&mut self) -> Result<Vec<u8>, ProtocolError> {
        // One request at a time; finish/abort an outstanding Ping before Stop.
        self.send(&HostMessage::Stop {}, Phase::Ready, Phase::Stopping)
    }
    fn send(&mut self, message: &HostMessage, expected: Phase, next: Phase) -> Result<Vec<u8>, ProtocolError> {
        self.active()?;
        let result = (|| {
            if self.phase != expected {
                return Err(ProtocolError::State);
            }
            if self.frames >= self.limits.frames() {
                return Err(ProtocolError::Budget);
            }
            let remaining = self.limits.bytes() - self.bytes;
            if remaining < 4 {
                return Err(ProtocolError::Budget);
            }
            let output = frame_limit(message, remaining).map_err(|e| if e == ProtocolError::Length { ProtocolError::Budget } else { e })?;
            self.account(output.len(), true)?;
            self.phase = next;
            Ok(output)
        })();
        self.result(result)
    }
    pub fn feed(&mut self, input: &[u8]) -> Result<Progress<WorkerMessage>, ProtocolError> {
        self.active()?;
        let result = (|| {
            // This probe protocol has no unsolicited events, including idle Error.
            // Bind the first byte to an already outstanding host request. Otherwise
            // a partial idle frame could finish after hello()/ping() changes phase.
            if !input.is_empty() && matches!(self.phase, Phase::New | Phase::Ready) {
                return Err(ProtocolError::State);
            }
            if !input.is_empty() && self.frames >= self.limits.frames() {
                return Err(ProtocolError::Budget);
            }
            let remaining = self.limits.bytes() - self.bytes;
            if !input.is_empty() && remaining == 0 {
                return Err(ProtocolError::Budget);
            }
            let progress = self.decoder.feed_budget(input, remaining)?;
            // Unlike the low-level decoder, this one-outstanding-request session
            // cannot defer already supplied trailing bytes past a future send.
            if progress.message.is_some() && progress.consumed != input.len() {
                return Err(ProtocolError::State);
            }
            self.account(progress.consumed, progress.message.is_some())?;
            if let Some(message) = &progress.message {
                self.receive(message)?;
            }
            Ok(progress)
        })();
        self.result(result)
    }
    fn receive(&mut self, message: &WorkerMessage) -> Result<(), ProtocolError> {
        match (self.phase, message) {
            (Phase::AwaitReady, WorkerMessage::Ready { protocol_version, identity, limits }) => {
                if *protocol_version != WORKER_PROTOCOL_VERSION || identity != &self.identity || *limits != self.limits {
                    return Err(ProtocolError::Identity);
                }
                self.phase = Phase::Ready;
            }
            (Phase::AwaitPong, WorkerMessage::Pong { sequence }) if sequence.get() == self.sequence => self.phase = Phase::Ready,
            (Phase::Stopping, WorkerMessage::Stopped {}) => self.phase = Phase::Stopped,
            (_, WorkerMessage::Error { .. }) => return Err(ProtocolError::Worker),
            _ => return Err(ProtocolError::State),
        }
        Ok(())
    }
    /// Any transport EOF prevents further work; only an already Stopped session
    /// is graceful. Partial/truncated EOF cannot resurrect a session.
    pub fn finish(&mut self) -> Result<(), ProtocolError> {
        if self.phase == Phase::Stopped {
            return Ok(());
        }
        if self.phase == Phase::Failed {
            return Err(ProtocolError::Terminal);
        }
        let result = self.decoder.finish().and(Err(ProtocolError::State));
        self.phase = Phase::Failed;
        result
    }
    /// Transport deadline/cancellation may terminate the pure machine explicitly.
    /// This does not enforce a timer, close a pipe, kill or reap any process.
    pub fn fail(&mut self) {
        self.phase = Phase::Failed;
    }
}

#[cfg(test)]
mod tests;
