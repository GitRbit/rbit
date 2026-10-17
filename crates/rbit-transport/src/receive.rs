//! The push gate's view of `git receive-pack`: the command list a client sends before
//! its pack, read before git sees it, and the refusal report sent instead of running git.
//!
//! Milestone 2 workstream B (branch protection) owns this file; the contract is
//! `docs/design/milestone-2b.md` §4.
//!
//! A receive-pack request starts with pkt-lines `old new ref[\0capabilities]`
//! (optionally preceded by `shallow <id>` lines) ending in a flush packet; push options
//! (when the `push-options` capability was requested) and the pack follow. The glue in
//! both transports does:
//!
//! ```text
//! match intercept(input, CommandLimits::default()).await {
//!     Err(_)                            => refuse the request (HTTP 400, SSH stderr + exit 1)
//!     Ok(Intercept::Unread(input))      => run git on `input`
//!     Ok(Intercept::Read { commands, input }) => {
//!         decide on commands.updates();
//!         refused => { drain(input).await; answer refusal(&commands, &refusals) }
//!         allowed => run git on `input` (which replays the commands byte for byte)
//!     }
//! }
//! ```

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use rbit_core::RefUpdate;
pub use rbit_core::protection::Refusal;
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};

/// The all-zero object id: `old` of a created ref, `new` of a deleted one.
pub const ZERO_ID: &str = "0000000000000000000000000000000000000000";

/// Bounds for the command section, so a client cannot make the server buffer without limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandLimits {
    /// Most commands (ref updates) in one push.
    pub max_commands: usize,
    /// Most bytes of the command section (shallow lines, commands, the flush packet).
    pub max_bytes: usize,
}

impl Default for CommandLimits {
    fn default() -> Self {
        Self { max_commands: 10_000, max_bytes: 4 * 1024 * 1024 }
    }
}

/// Why the command section could not be read. The message is shown to the client.
#[derive(Debug, thiserror::Error)]
pub enum ReceiveError {
    #[error("too many ref updates in one push (at most {max})")]
    TooManyCommands { max: usize },
    #[error("the push command list is too large (at most {max} bytes)")]
    TooLarge { max: usize },
    #[error("malformed push request: {0}")]
    Malformed(String),
    #[error("I/O error reading the push request: {0}")]
    Io(#[from] io::Error),
}

/// One `old new ref` line of the command list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushCommand {
    /// The id the client believes the ref has now ([`ZERO_ID`] to create it).
    pub old: String,
    /// The id the ref should get ([`ZERO_ID`] to delete it).
    pub new: String,
    /// The full ref name, e.g. `refs/heads/main`.
    pub name: String,
}

impl PushCommand {
    /// As a [`RefUpdate`]: zero ids become `None`.
    pub fn update(&self) -> RefUpdate {
        let id = |value: &str| (value != ZERO_ID).then(|| value.to_owned());
        RefUpdate { name: self.name.clone(), before: id(&self.old), after: id(&self.new) }
    }
}

/// The parsed command section of a receive-pack request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandList {
    commands: Vec<PushCommand>,
    /// The capabilities requested on the first command line (`report-status`,
    /// `side-band-64k`, `push-options`, `atomic`, …).
    capabilities: Vec<String>,
    /// `shallow <id>` lines that preceded the commands.
    shallow: Vec<String>,
}

impl CommandList {
    pub fn new(commands: Vec<PushCommand>, capabilities: Vec<String>, shallow: Vec<String>) -> Self {
        Self { commands, capabilities, shallow }
    }

    pub fn commands(&self) -> &[PushCommand] {
        &self.commands
    }

    /// The requested ref updates, in the client's order.
    pub fn updates(&self) -> Vec<RefUpdate> {
        self.commands.iter().map(PushCommand::update).collect()
    }

    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    /// Whether the client requested `capability` (a bare name such as `report-status`; for
    /// `key=value` capabilities, the key).
    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|requested| requested.split_once('=').map_or(requested.as_str(), |(key, _)| key) == capability)
    }

    pub fn shallow(&self) -> &[String] {
        &self.shallow
    }

    /// An empty list: the client has nothing to push (`Everything up-to-date`).
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Whether the client expects a report at all (`report-status` or `report-status-v2`).
    pub fn wants_report(&self) -> bool {
        self.has_capability("report-status") || self.has_capability("report-status-v2")
    }
}

/// What [`intercept`] made of the start of a request.
#[derive(Debug)]
pub enum Intercept<R> {
    /// Nothing was read: hand the input to git unchanged.
    Unread(R),
    /// The command section was read; `input` yields it again, then the rest of the stream.
    Read { commands: CommandList, input: Replay<R> },
}

/// What follows the saved bytes in a [`Replay`], so [`drain`] reads exactly as much as
/// git would have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remaining {
    /// Nothing more is expected: every command is a deletion and the push options, if
    /// any, are already among the saved bytes. (Over SSH the client keeps the channel open
    /// waiting for the report, so reading on would never end.)
    Nothing,
    /// A pack (and possibly push options) follows, up to the end of the stream.
    ToEnd,
}

/// Replays saved bytes, then the rest of the underlying stream, byte for byte.
#[derive(Debug)]
pub struct Replay<R> {
    saved: Vec<u8>,
    position: usize,
    inner: R,
    remaining: Remaining,
}

impl<R> Replay<R> {
    /// `saved` are the bytes already read from `inner`.
    pub fn new(saved: Vec<u8>, inner: R, remaining: Remaining) -> Self {
        Self { saved, position: 0, inner, remaining }
    }

    pub fn remaining(&self) -> Remaining {
        self.remaining
    }

    /// The saved bytes not yet replayed.
    pub fn pending(&self) -> &[u8] {
        &self.saved[self.position..]
    }

    /// The underlying stream, discarding saved bytes not yet replayed.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for Replay<R> {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if this.position < this.saved.len() {
            let pending = &this.saved[this.position..];
            let take = pending.len().min(buf.remaining());
            buf.put_slice(&pending[..take]);
            this.position += take;
            if this.position == this.saved.len() {
                // Replayed in full: release the buffer early.
                this.saved = Vec::new();
                this.position = 0;
            }
            return Poll::Ready(Ok(()));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

/// The largest pkt-line git sends (the 4 length bytes included).
const MAX_PKT_LEN: usize = 65520;

/// Reads the command section from `input` (bounded by `limits`): optional `shallow` lines,
/// the commands (capabilities after a NUL on the first) and the flush packet, plus — when
/// every command is a deletion — the push options, so that nothing else is expected (see
/// [`Remaining`]). Reads exactly those bytes, never into the pack; memory stays bounded
/// by `limits` whatever the client sends.
///
/// Always returns [`Intercept::Read`]; an empty stream counts as an empty command list.
/// Signed pushes (`push-cert`) are malformed here: rbit never advertises them.
pub async fn intercept<R: AsyncRead + Unpin>(mut input: R, limits: CommandLimits) -> Result<Intercept<R>, ReceiveError> {
    let mut reader = PktReader { saved: Vec::new(), limits };
    let mut commands = Vec::new();
    let mut capabilities = Vec::new();
    let mut shallow = Vec::new();
    loop {
        let line = match reader.next(&mut input, true).await? {
            Pkt::Eof | Pkt::Flush => break,
            Pkt::Line(line) => line,
        };
        if commands.is_empty()
            && let Some(id) = line.strip_prefix("shallow ")
        {
            if !is_object_id(id) {
                return Err(malformed("invalid shallow line"));
            }
            shallow.push(id.to_owned());
            continue;
        }
        if line.starts_with("push-cert") {
            return Err(malformed("signed pushes are not supported"));
        }
        let command = match line.split_once('\0') {
            Some((command, requested)) if commands.is_empty() => {
                capabilities = requested.split(' ').filter(|capability| !capability.is_empty()).map(str::to_owned).collect();
                command
            }
            Some(_) => return Err(malformed("capabilities after the first command")),
            None => line.as_str(),
        };
        if commands.len() == limits.max_commands {
            return Err(ReceiveError::TooManyCommands { max: limits.max_commands });
        }
        commands.push(parse_command(command)?);
    }

    let list = CommandList::new(commands, capabilities, shallow);
    let remaining = if list.commands().iter().all(|command| command.new == ZERO_ID) {
        if !list.is_empty() && list.has_capability("push-options") {
            // No pack follows, so read the push options too: then nothing is left to wait for.
            while let Pkt::Line(_) = reader.next(&mut input, false).await? {}
        }
        Remaining::Nothing
    } else {
        Remaining::ToEnd
    };
    Ok(Intercept::Read { commands: list, input: Replay::new(reader.saved, input, remaining) })
}

fn malformed(message: &str) -> ReceiveError {
    ReceiveError::Malformed(message.to_owned())
}

/// A full lower-case hexadecimal object id (SHA-1 or SHA-256).
fn is_object_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// `old SP new SP ref`.
fn parse_command(line: &str) -> Result<PushCommand, ReceiveError> {
    let mut parts = line.splitn(3, ' ');
    let (Some(old), Some(new), Some(name)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(malformed("invalid command line"));
    };
    if !is_object_id(old) || !is_object_id(new) || old.len() != new.len() {
        return Err(malformed("invalid object id in a command"));
    }
    if name.is_empty() || name.contains(|c: char| c.is_control() || c == ' ') {
        return Err(malformed("invalid ref name in a command"));
    }
    Ok(PushCommand { old: old.to_owned(), new: new.to_owned(), name: name.to_owned() })
}

/// One pkt-line as [`PktReader`] reads it.
enum Pkt {
    Line(String),
    Flush,
    /// The stream ended before the first byte of the request.
    Eof,
}

/// Reads pkt-lines one at a time, exactly, keeping every byte read.
struct PktReader {
    saved: Vec<u8>,
    limits: CommandLimits,
}

impl PktReader {
    /// The next pkt-line. `eof_ok`: an empty stream is [`Pkt::Eof`] rather than an error.
    async fn next<R: AsyncRead + Unpin>(&mut self, input: &mut R, eof_ok: bool) -> Result<Pkt, ReceiveError> {
        let mut header = [0; 4];
        let first = input.read(&mut header).await?;
        if first == 0 {
            return if eof_ok && self.saved.is_empty() { Ok(Pkt::Eof) } else { Err(malformed("the request ended early")) };
        }
        read_rest(input, &mut header[first..]).await?;
        let length = std::str::from_utf8(&header)
            .ok()
            .filter(|digits| digits.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .and_then(|digits| usize::from_str_radix(digits, 16).ok())
            .ok_or_else(|| malformed("invalid pkt-line length"))?;
        self.reserve(header.len())?;
        self.saved.extend_from_slice(&header);
        if length == 0 {
            return Ok(Pkt::Flush);
        }
        if length <= header.len() || length > MAX_PKT_LEN {
            return Err(malformed("invalid pkt-line length"));
        }
        let payload_len = length - header.len();
        self.reserve(payload_len)?;
        let start = self.saved.len();
        self.saved.resize(start + payload_len, 0);
        read_rest(input, &mut self.saved[start..]).await?;
        let payload = &self.saved[start..];
        let payload = payload.strip_suffix(b"\n").unwrap_or(payload);
        let line = std::str::from_utf8(payload).map_err(|_| malformed("a command line is not UTF-8"))?;
        Ok(Pkt::Line(line.to_owned()))
    }

    fn reserve(&self, more: usize) -> Result<(), ReceiveError> {
        if self.saved.len() + more > self.limits.max_bytes {
            return Err(ReceiveError::TooLarge { max: self.limits.max_bytes });
        }
        Ok(())
    }
}

/// `read_exact` that reports an early end of the stream as malformed input.
async fn read_rest<R: AsyncRead + Unpin>(input: &mut R, buffer: &mut [u8]) -> Result<(), ReceiveError> {
    match input.read_exact(buffer).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Err(malformed("the request ended early")),
        Err(error) => Err(error.into()),
    }
}

/// Reads and discards what is left of a refused request — as much as git would have
/// read (see [`Remaining`]) — so the client gets the report instead of a broken pipe.
/// Read errors only end the draining early.
pub async fn drain<R: AsyncRead + Unpin>(input: Replay<R>) {
    if input.remaining() == Remaining::Nothing {
        return;
    }
    let mut inner = input.into_inner();
    let mut buffer = vec![0; 64 * 1024];
    while let Ok(read) = inner.read(&mut buffer).await {
        if read == 0 {
            break;
        }
    }
}

/// The reason given for refs of a refused push that were not refused themselves.
pub const DECLINED: &str = "push declined: another ref in this push was refused";

/// The response to send instead of running git when `refusals` is not empty: `unpack ok`,
/// `ng <ref> <reason>` per refused ref and `ng <ref> push declined: …` for the others (in
/// the client's order), framed for `report-status`/`report-status-v2` and
/// `side-band-64k`/`side-band` as the client requested. With a side band, a band-2
/// `rbit: <ref>: <reason>` message per refusal comes first and a flush packet last, as
/// git sends them. Empty when the client requested no report (the glue then answers
/// HTTP 403 / SSH stderr with [`refusal_message`]).
pub fn refusal(commands: &CommandList, refusals: &[Refusal]) -> Vec<u8> {
    if !commands.wants_report() {
        return Vec::new();
    }
    let mut report = Vec::new();
    pkt_line(&mut report, b"unpack ok\n");
    for command in commands.commands() {
        let reason = refusals.iter().find(|refusal| refusal.name == command.name).map_or(DECLINED, |refusal| refusal.reason.as_str());
        pkt_line(&mut report, format!("ng {} {}\n", command.name, one_line(reason)).as_bytes());
    }
    report.extend_from_slice(b"0000");

    let band_size = if commands.has_capability("side-band-64k") {
        MAX_PKT_LEN - 5
    } else if commands.has_capability("side-band") {
        1000 - 5
    } else {
        return report;
    };
    let mut framed = Vec::with_capacity(report.len() + 64);
    for refusal in refusals {
        let message = format!("rbit: {}: {}\n", one_line(&refusal.name), one_line(&refusal.reason));
        for chunk in message.as_bytes().chunks(band_size) {
            band(&mut framed, 2, chunk);
        }
    }
    for chunk in report.chunks(band_size) {
        band(&mut framed, 1, chunk);
    }
    framed.extend_from_slice(b"0000");
    framed
}

/// Appends `payload` as one pkt-line, cut to what a pkt-line can hold.
fn pkt_line(out: &mut Vec<u8>, payload: &[u8]) {
    let payload = &payload[..payload.len().min(MAX_PKT_LEN - 4)];
    out.extend_from_slice(format!("{:04x}", payload.len() + 4).as_bytes());
    out.extend_from_slice(payload);
}

/// Appends `data` as one side-band packet on `channel`.
fn band(out: &mut Vec<u8>, channel: u8, data: &[u8]) {
    out.extend_from_slice(format!("{:04x}", data.len() + 5).as_bytes());
    out.push(channel);
    out.extend_from_slice(data);
}

/// `text` with control characters turned into spaces, so it stays one report line.
fn one_line(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// A plain-text explanation of `refusals`, one line per refused ref, for clients that
/// requested no report.
pub fn refusal_message(refusals: &[Refusal]) -> String {
    let mut message = String::new();
    for refusal in refusals {
        message.push_str(&refusal.name);
        message.push_str(": ");
        message.push_str(&refusal.reason);
        message.push('\n');
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn replay_yields_saved_bytes_then_the_rest() {
        let rest: &[u8] = b" world";
        let mut replay = Replay::new(b"hello".to_vec(), rest, Remaining::ToEnd);
        let mut all = Vec::new();
        replay.read_to_end(&mut all).await.unwrap();
        assert_eq!(all, b"hello world");
    }

    #[tokio::test]
    async fn replay_respects_small_reads() {
        let rest: &[u8] = b"cd";
        let mut replay = Replay::new(b"ab".to_vec(), rest, Remaining::ToEnd);
        let mut one = [0; 1];
        let mut seen = Vec::new();
        while replay.read(&mut one).await.unwrap() == 1 {
            seen.push(one[0]);
        }
        assert_eq!(seen, b"abcd");
    }

    #[tokio::test]
    async fn drain_reads_to_the_end_only_when_more_is_expected() {
        let (mut client, server) = tokio::io::duplex(16);
        // The client keeps the stream open: draining must not wait for it.
        drain(Replay::new(b"0000".to_vec(), server, Remaining::Nothing)).await;

        let (client2, server2) = tokio::io::duplex(16);
        drop(client2);
        drain(Replay::new(Vec::new(), server2, Remaining::ToEnd)).await;
        tokio::io::AsyncWriteExt::shutdown(&mut client).await.unwrap();
    }

    #[test]
    fn report_lines_stay_on_one_line() {
        assert_eq!(one_line("a\nb\rc"), "a b c");
        let mut out = Vec::new();
        pkt_line(&mut out, b"unpack ok\n");
        assert_eq!(out, b"000eunpack ok\n");
    }

    #[test]
    fn commands_convert_to_ref_updates() {
        let id = "1".repeat(40);
        let list = CommandList::new(
            vec![
                PushCommand { old: ZERO_ID.to_owned(), new: id.clone(), name: "refs/heads/new".to_owned() },
                PushCommand { old: id.clone(), new: ZERO_ID.to_owned(), name: "refs/heads/gone".to_owned() },
            ],
            vec!["report-status".to_owned(), "agent=git/2.50".to_owned()],
            Vec::new(),
        );
        let updates = list.updates();
        assert!(updates[0].is_created() && updates[0].after.as_deref() == Some(id.as_str()));
        assert!(updates[1].is_deleted() && updates[1].before.as_deref() == Some(id.as_str()));
        assert!(list.wants_report() && list.has_capability("agent") && !list.has_capability("side-band-64k"));
        assert!(!list.is_empty() && CommandList::default().is_empty());
    }

    #[test]
    fn refusal_messages_list_each_ref() {
        let refusals = [Refusal { name: "refs/heads/main".to_owned(), reason: "protected branch".to_owned() }];
        assert_eq!(refusal_message(&refusals), "refs/heads/main: protected branch\n");
    }
}
