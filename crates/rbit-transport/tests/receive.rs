//! The push gate's protocol layer: reading a receive-pack command list before git does,
//! replaying it byte for byte, and the exact refusal reports for every framing a client
//! may ask for.

use std::time::Duration;

use proptest::prelude::*;
use rbit_transport::receive::{
    CommandLimits, CommandList, DECLINED, Intercept, PushCommand, Refusal, Remaining, ZERO_ID, drain, intercept, refusal, refusal_message,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const OLD: &str = "1111111111111111111111111111111111111111";
const NEW: &str = "2222222222222222222222222222222222222222";

fn pkt(payload: &str) -> Vec<u8> {
    let mut line = format!("{:04x}", payload.len() + 4).into_bytes();
    line.extend_from_slice(payload.as_bytes());
    line
}

fn request(lines: &[String]) -> Vec<u8> {
    let mut bytes: Vec<u8> = lines.iter().flat_map(|line| pkt(line)).collect();
    bytes.extend_from_slice(b"0000");
    bytes
}

async fn read(bytes: &[u8]) -> (CommandList, Vec<u8>, Remaining) {
    read_with(bytes, CommandLimits::default()).await.expect("a valid request")
}

async fn read_with(bytes: &[u8], limits: CommandLimits) -> Result<(CommandList, Vec<u8>, Remaining), String> {
    match intercept(bytes, limits).await {
        Ok(Intercept::Read { commands, mut input }) => {
            let remaining = input.remaining();
            let mut replayed = Vec::new();
            input.read_to_end(&mut replayed).await.unwrap();
            Ok((commands, replayed, remaining))
        }
        Ok(Intercept::Unread(_)) => panic!("intercept always reads"),
        Err(error) => Err(error.to_string()),
    }
}

fn command(old: &str, new: &str, name: &str) -> PushCommand {
    PushCommand { old: old.to_owned(), new: new.to_owned(), name: name.to_owned() }
}

#[tokio::test]
async fn commands_with_capabilities_and_the_pack_are_replayed_byte_for_byte() {
    let mut bytes = request(&[
        format!("{OLD} {NEW} refs/heads/main\0 report-status side-band-64k push-options agent=git/2.50.0\n"),
        format!("{ZERO_ID} {NEW} refs/heads/topic\n"),
        format!("{OLD} {ZERO_ID} refs/tags/old"),
    ]);
    let command_section = bytes.len();
    // Push options, then the pack: left to git, untouched.
    bytes.extend(pkt("ci.skip\n"));
    bytes.extend_from_slice(b"0000PACK\0\0\0\x02\0\0\0\x01binary\xff\xfe pack data");

    let (list, replayed, remaining) = read(&bytes).await;
    assert_eq!(replayed, bytes, "the replay is the request, byte for byte");
    assert_eq!(remaining, Remaining::ToEnd);
    assert_eq!(
        list.commands(),
        [command(OLD, NEW, "refs/heads/main"), command(ZERO_ID, NEW, "refs/heads/topic"), command(OLD, ZERO_ID, "refs/tags/old")]
    );
    assert_eq!(list.capabilities(), ["report-status", "side-band-64k", "push-options", "agent=git/2.50.0"]);
    assert!(list.wants_report() && list.has_capability("agent") && list.shallow().is_empty());

    // Only the command section was read: the rest is still in the stream.
    let Intercept::Read { input, .. } = intercept(&bytes[..], CommandLimits::default()).await.unwrap() else { unreachable!() };
    assert_eq!(input.pending().len(), command_section);
    assert_eq!(input.into_inner(), &bytes[command_section..]);
}

#[tokio::test]
async fn commands_without_capabilities_and_shallow_lines() {
    let shallow = "3".repeat(40);
    let bytes = request(&[format!("shallow {shallow}"), format!("{OLD} {NEW} refs/heads/main\n")]);
    let (list, replayed, remaining) = read(&bytes).await;
    assert_eq!(replayed, bytes);
    assert_eq!(list.shallow(), [shallow]);
    assert!(list.capabilities().is_empty() && !list.wants_report());
    assert_eq!(list.commands(), [command(OLD, NEW, "refs/heads/main")]);
    assert_eq!(remaining, Remaining::ToEnd);

    // SHA-256 ids.
    let (old, new) = ("a".repeat(64), "b".repeat(64));
    let (list, _, _) = read(&request(&[format!("{old} {new} refs/heads/main\0report-status")])).await;
    assert_eq!(list.updates()[0].after.as_deref(), Some(new.as_str()));
}

#[tokio::test]
async fn empty_pushes_read_nothing_more() {
    for bytes in [&b"0000"[..], b""] {
        let (list, replayed, remaining) = read(bytes).await;
        assert!(list.is_empty());
        assert_eq!((replayed.as_slice(), remaining), (bytes, Remaining::Nothing));
    }
}

#[tokio::test]
async fn deletion_only_pushes_never_wait_for_more_input() {
    // Over SSH the client keeps the channel open for the report: reading on would hang.
    let mut bytes = request(&[format!("{OLD} {ZERO_ID} refs/heads/gone\0report-status push-options")]);
    bytes.extend(pkt("reason=cleanup\n"));
    bytes.extend_from_slice(b"0000");
    let (mut client, server) = tokio::io::duplex(1 << 16);
    client.write_all(&bytes).await.unwrap();
    let intercepted = tokio::time::timeout(Duration::from_secs(5), intercept(server, CommandLimits::default())).await;
    let Intercept::Read { commands, mut input } = intercepted.expect("does not wait for EOF").unwrap() else { unreachable!() };
    assert_eq!(input.remaining(), Remaining::Nothing);
    assert_eq!(input.pending(), bytes, "the push options were read too");
    assert!(commands.updates()[0].is_deleted());
    let mut replayed = vec![0; bytes.len()];
    input.read_exact(&mut replayed).await.unwrap();
    assert_eq!(replayed, bytes);
    tokio::time::timeout(Duration::from_secs(5), drain(input)).await.expect("draining does not wait either");

    // Without push options only the command section is read.
    let bytes = request(&[format!("{OLD} {ZERO_ID} refs/heads/gone\0report-status")]);
    let (list, replayed, remaining) = read(&bytes).await;
    assert_eq!((list.commands().len(), replayed, remaining), (1, bytes, Remaining::Nothing));
}

#[tokio::test]
async fn draining_reads_the_rest_of_a_refused_push() {
    let mut bytes = request(&[format!("{OLD} {NEW} refs/heads/main\0report-status")]);
    bytes.extend(vec![0; 200_000]);
    let (mut client, server) = tokio::io::duplex(1024);
    let writer = tokio::spawn(async move {
        client.write_all(&bytes).await.unwrap();
        client.shutdown().await.unwrap();
    });
    let Intercept::Read { input, .. } = intercept(server, CommandLimits::default()).await.unwrap() else { unreachable!() };
    tokio::time::timeout(Duration::from_secs(5), drain(input)).await.expect("drained to the end");
    writer.await.expect("the client could write its whole request");
}

#[tokio::test]
async fn oversized_and_malformed_requests_are_errors() {
    let line = |name: &str| format!("{OLD} {NEW} {name}");
    let three: Vec<String> = ["a", "b", "c"].iter().map(|name| line(&format!("refs/heads/{name}"))).collect();
    let limits = CommandLimits { max_commands: 2, max_bytes: 4096 };
    let error = read_with(&request(&three), limits).await.unwrap_err();
    assert_eq!(error, "too many ref updates in one push (at most 2)");
    let limits = CommandLimits { max_commands: 10, max_bytes: 100 };
    let error = read_with(&request(&three), limits).await.unwrap_err();
    assert_eq!(error, "the push command list is too large (at most 100 bytes)");

    let cases: Vec<(Vec<u8>, &str)> = vec![
        (b"00zz".to_vec(), "invalid pkt-line length"),
        (b"0001".to_vec(), "invalid pkt-line length"),
        (b"0004".to_vec(), "invalid pkt-line length"),
        (b"00".to_vec(), "the request ended early"),
        (pkt(&line("refs/heads/main"))[..20].to_vec(), "the request ended early"),
        (pkt(&line("refs/heads/main")), "the request ended early"),
        (request(&[format!("{OLD} {NEW}")]), "invalid command line"),
        (request(&[format!("{OLD} nothex refs/heads/main")]), "invalid object id in a command"),
        (request(&[format!("{} {NEW} refs/heads/main", "A".repeat(40))]), "invalid object id in a command"),
        (request(&[format!("{OLD} {} refs/heads/main", "b".repeat(64))]), "invalid object id in a command"),
        (request(&[format!("{OLD} {NEW} ")]), "invalid ref name in a command"),
        (request(&[format!("{OLD} {NEW} refs/heads/a b")]), "invalid ref name in a command"),
        (request(&[line("refs/heads/a"), format!("{}\0report-status", line("refs/heads/b"))]), "capabilities after the first command"),
        (request(&["shallow 123".to_owned()]), "invalid shallow line"),
        (request(&["push-cert\0report-status".to_owned()]), "signed pushes are not supported"),
        ([b"0008\xff\xfe\xfd\xfc".as_slice(), b"0000"].concat(), "a command line is not UTF-8"),
    ];
    for (bytes, message) in cases {
        let error = read_with(&bytes, CommandLimits::default()).await.unwrap_err();
        assert_eq!(error, format!("malformed push request: {message}"), "{bytes:?}");
    }
    // Deletion-only pushes with push options: the options must end with a flush too.
    let mut bytes = request(&[format!("{OLD} {ZERO_ID} refs/heads/gone\0push-options")]);
    bytes.extend(pkt("x"));
    assert_eq!(read_with(&bytes, CommandLimits::default()).await.unwrap_err(), "malformed push request: the request ended early");
}

fn list(capabilities: &str, names: &[&str]) -> CommandList {
    let commands = names.iter().map(|name| command(OLD, NEW, name)).collect();
    CommandList::new(commands, capabilities.split(' ').filter(|c| !c.is_empty()).map(str::to_owned).collect(), Vec::new())
}

fn refused(name: &str, reason: &str) -> Refusal {
    Refusal { name: name.to_owned(), reason: reason.to_owned() }
}

const PROTECTED: &str = "protected branch: deleting it is not allowed";

fn expected_report() -> Vec<u8> {
    [
        pkt("unpack ok\n"),
        pkt(&format!("ng refs/heads/main {PROTECTED}\n")),
        pkt(&format!("ng refs/heads/topic {DECLINED}\n")),
        b"0000".to_vec(),
    ]
    .concat()
}

#[test]
fn refusal_reports_for_report_status_and_v2() {
    let refusals = [refused("refs/heads/main", PROTECTED)];
    for capabilities in ["report-status", "report-status-v2", "report-status agent=git/2.50.0"] {
        let report = refusal(&list(capabilities, &["refs/heads/main", "refs/heads/topic"]), &refusals);
        assert_eq!(report, expected_report(), "{capabilities}");
    }
    let report = refusal(&list("report-status", &["refs/heads/main", "refs/heads/topic"]), &refusals);
    assert_eq!(
        String::from_utf8(report).unwrap(),
        format!("000eunpack ok\n0044ng refs/heads/main {PROTECTED}\n004cng refs/heads/topic {DECLINED}\n0000")
    );
}

#[test]
fn refusal_reports_on_a_side_band() {
    let refusals = [refused("refs/heads/main", PROTECTED)];
    let names = ["refs/heads/main", "refs/heads/topic"];
    let message = format!("Rbit: refs/heads/main: {PROTECTED}\n");
    for capabilities in ["report-status side-band-64k", "report-status-v2 side-band", "side-band-64k report-status"] {
        let report = refusal(&list(capabilities, &names), &refusals);
        let mut expected = format!("{:04x}\x02{message}", message.len() + 5).into_bytes();
        let inner = expected_report();
        expected.extend(format!("{:04x}\x01", inner.len() + 5).into_bytes());
        expected.extend(inner);
        expected.extend_from_slice(b"0000");
        assert_eq!(report, expected, "{capabilities}");
    }
}

#[test]
fn no_report_without_report_status() {
    let refusals = [refused("refs/heads/main", PROTECTED)];
    assert_eq!(refusal(&list("side-band-64k agent=git/2.50.0", &["refs/heads/main"]), &refusals), b"");
    assert_eq!(refusal(&list("", &["refs/heads/main"]), &refusals), b"");
    assert_eq!(refusal_message(&refusals), format!("refs/heads/main: {PROTECTED}\n"));
}

#[test]
fn reasons_cannot_break_the_report() {
    let refusals = [refused("refs/heads/main", "bad\nreason")];
    let report = refusal(&list("report-status", &["refs/heads/main"]), &refusals);
    assert_eq!(report, [pkt("unpack ok\n"), pkt("ng refs/heads/main bad reason\n"), b"0000".to_vec()].concat());
}

/// Splits side-band packets into (band, data), checking each packet's size limit.
fn demux(mut bytes: &[u8], limit: usize) -> Vec<(u8, Vec<u8>)> {
    let mut packets = Vec::new();
    loop {
        let length = usize::from_str_radix(std::str::from_utf8(&bytes[..4]).unwrap(), 16).unwrap();
        if length == 0 {
            assert_eq!(bytes.len(), 4, "the flush packet ends the response");
            return packets;
        }
        assert!(length <= limit, "packet of {length} bytes exceeds {limit}");
        packets.push((bytes[4], bytes[5..length].to_vec()));
        bytes = &bytes[length..];
    }
}

#[test]
fn long_reports_are_split_into_side_band_packets() {
    let names: Vec<String> = (0..40).map(|n| format!("refs/heads/branch-with-a-long-name-{n:03}")).collect();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let refusals: Vec<Refusal> = names.iter().map(|name| refused(name, PROTECTED)).collect();
    for (capability, limit) in [("side-band", 1000), ("side-band-64k", 65520)] {
        let report = refusal(&list(&format!("report-status {capability}"), &name_refs), &refusals);
        let packets = demux(&report, limit);
        assert_eq!(packets.iter().filter(|(band, _)| *band == 2).count(), names.len(), "{capability}: one message per refusal");
        let inner: Vec<u8> = packets.iter().filter(|(band, _)| *band == 1).flat_map(|(_, data)| data.clone()).collect();
        let plain = refusal(&list("report-status", &name_refs), &refusals);
        assert_eq!(inner, plain, "{capability}: band 1 carries the plain report");
        if capability == "side-band" {
            assert!(packets.iter().filter(|(band, _)| *band == 1).count() > 1, "split into several packets");
        }
    }
}

fn arbitrary_command() -> impl Strategy<Value = PushCommand> {
    let id = prop_oneof![Just(ZERO_ID.to_owned()), "[0-9a-f]{40}"];
    (id.clone(), id, "refs/(heads|tags|pull)/[a-zA-Z0-9._/-]{1,40}").prop_map(|(old, new, name)| PushCommand { old, new, name })
}

proptest! {
    #[test]
    fn any_command_list_is_read_back_and_replayed(
        commands in proptest::collection::vec(arbitrary_command(), 1..20),
        capabilities in proptest::collection::vec("[a-z0-9=./-]{1,12}".prop_filter("no push options", |c| c != "push-options"), 0..5),
        tail in proptest::collection::vec(any::<u8>(), 0..200),
    ) {
        let mut lines: Vec<String> = commands.iter().map(|c| format!("{} {} {}", c.old, c.new, c.name)).collect();
        lines[0] = format!("{}\0{}", lines[0], capabilities.join(" "));
        let mut bytes = request(&lines);
        bytes.extend(&tail);
        let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
        let (list, replayed, remaining) = runtime.block_on(read(&bytes));
        prop_assert_eq!(list.commands(), commands.as_slice());
        prop_assert_eq!(list.capabilities(), capabilities.as_slice());
        let deletions_only = commands.iter().all(|c| c.new == ZERO_ID);
        prop_assert_eq!(remaining, if deletions_only { Remaining::Nothing } else { Remaining::ToEnd });
        prop_assert_eq!(replayed, bytes);
    }
}
