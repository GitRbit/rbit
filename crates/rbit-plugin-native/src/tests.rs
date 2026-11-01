use super::*;
use rbit_plugin_api::worker::{MAX_WORKER_SESSION_BYTES, MAX_WORKER_SESSION_FRAMES, WorkerFailureCode};
use serde_json::json;
use std::io::Write as _;

fn identity() -> WorkerIdentity {
    serde_json::from_value(json!({
        "package": {"plugin_id":"unknown.probe","version":"1.0.0+one","archive_digest":"a".repeat(64)},
        "epoch":"b".repeat(64),"host_policy":"c".repeat(64),
        "selection_revision":"1","review_revision":"2","host_instance":"d".repeat(64),
        "generation":1,"launch_nonce":"e".repeat(64)
    }))
    .unwrap()
}
fn ready(identity: WorkerIdentity) -> Vec<u8> {
    ready_with_limits(identity, WorkerLimits::default())
}
fn ready_with_limits(identity: WorkerIdentity, limits: WorkerLimits) -> Vec<u8> {
    encode_worker(&WorkerMessage::Ready { protocol_version: WORKER_PROTOCOL_VERSION, identity, limits }).unwrap()
}
fn raw(body: &[u8]) -> Vec<u8> {
    let mut frame = u32::try_from(body.len()).unwrap().to_be_bytes().to_vec();
    frame.extend_from_slice(body);
    frame
}
fn running() -> Session {
    let mut session = Session::new(identity());
    session.hello().unwrap();
    session.feed(&ready(identity())).unwrap();
    assert_eq!(session.phase(), Phase::Ready);
    session
}

#[test]
fn exact_wire_and_every_split_roundtrip_preserve_caller_owned_suffix() {
    assert_eq!(encode_worker(&WorkerMessage::Stopped {}).unwrap(), b"\0\0\0\x12{\"type\":\"stopped\"}");
    let bytes = ready(identity());
    for split in 0..bytes.len() {
        let mut decoder = Decoder::<WorkerMessage>::new();
        let first = decoder.feed(&bytes[..split]).unwrap();
        assert_eq!(first.consumed, split);
        assert!(first.message.is_none());
        let mut rest = bytes[split..].to_vec();
        rest.extend_from_slice(b"caller suffix");
        let next = decoder.feed(&rest).unwrap();
        assert_eq!(next.consumed, bytes.len() - split);
        assert_eq!(next.message, Some(WorkerMessage::Ready { protocol_version: 1, identity: identity(), limits: WorkerLimits::default() }));
    }
    let hello = HostMessage::Hello { protocol_version: 1, identity: identity(), limits: WorkerLimits::default() };
    let bytes = encode_host(&hello).unwrap();
    assert_eq!(Decoder::<HostMessage>::new().feed(&bytes).unwrap().message, Some(hello));
}

#[test]
fn hostile_prefixes_allocate_no_body_and_all_truncation_is_terminal() {
    for length in [0u32, 65_537, u32::MAX] {
        let mut decoder = Decoder::<WorkerMessage>::new();
        assert_eq!(decoder.feed(&length.to_be_bytes()).unwrap_err(), ProtocolError::Length);
        assert_eq!(decoder.body.capacity(), 0);
        assert_eq!(decoder.feed(&[]).unwrap_err(), ProtocolError::Terminal);
    }
    let bytes = ready(identity());
    for end in 1..bytes.len() {
        let mut decoder = Decoder::<WorkerMessage>::new();
        assert!(decoder.feed(&bytes[..end]).unwrap().message.is_none());
        assert_eq!(decoder.finish().unwrap_err(), ProtocolError::Truncated);
        assert_eq!(decoder.feed(&bytes[end..]).unwrap_err(), ProtocolError::Terminal);
    }
}

fn rejected(body: &[u8], expected: ProtocolError) {
    let mut decoder = Decoder::<WorkerMessage>::new();
    assert_eq!(decoder.feed(&raw(body)).unwrap_err(), expected);
    assert_eq!(decoder.feed(&ready(identity())).unwrap_err(), ProtocolError::Terminal);
}
#[test]
fn duplicate_unknown_noncanonical_utf8_and_depth_are_rejected() {
    rejected(br#"{"type":"stopped","type":"stopped"}"#, ProtocolError::Message);
    rejected(br#"{"type":"stopped","call":"database"}"#, ProtocolError::Message);
    rejected(br#"{"type":"host_call"}"#, ProtocolError::Message);
    rejected(br#"{ "type":"stopped"}"#, ProtocolError::Message);
    rejected(&[0xff], ProtocolError::Message);
    let bytes = ready(identity());
    let body = std::str::from_utf8(&bytes[4..]).unwrap();
    for replacement in ["\"generation\":1,\"generation\":1", "\"generation\":1,\"extra\":0"] {
        rejected(body.replace("\"generation\":1", replacement).as_bytes(), ProtocolError::Message);
    }
    rejected(body.replace("\"review_revision\":\"2\"", "\"review_revision\":\"02\"").as_bytes(), ProtocolError::Message);
    for replacement in ["\"version\":\"1.0.0+one\",\"version\":\"1.0.0+one\"", "\"version\":\"1.0.0+one\",\"extra\":0"] {
        rejected(body.replace("\"version\":\"1.0.0+one\"", replacement).as_bytes(), ProtocolError::Message);
    }
    let nested = format!("{}0{}", "[".repeat(MAX_WORKER_JSON_DEPTH + 1), "]".repeat(MAX_WORKER_JSON_DEPTH + 1));
    rejected(nested.as_bytes(), ProtocolError::Depth);
    // Brackets and escaped quotes inside a valid string must not affect nesting.
    assert!(depth(br#"{"key":"[\"{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{{"}"#).is_ok());
}

#[test]
fn exact_session_identity_and_protocol_cannot_be_replayed_or_substituted() {
    let base = serde_json::to_value(identity()).unwrap();
    for (field, value) in [
        ("epoch", json!("f".repeat(64))),
        ("host_policy", json!("f".repeat(64))),
        ("host_instance", json!("f".repeat(64))),
        ("launch_nonce", json!("f".repeat(64))),
        ("generation", json!(2)),
        ("selection_revision", json!("2")),
        ("review_revision", json!("3")),
        ("package", json!({"plugin_id":"unknown.probe","version":"1.0.0+two","archive_digest":"a".repeat(64)})),
        ("package", json!({"plugin_id":"unknown.probe","version":"1.0.0+one","archive_digest":"f".repeat(64)})),
    ] {
        let mut changed = base.clone();
        changed[field] = value;
        let mut session = Session::new(identity());
        session.hello().unwrap();
        let changed = serde_json::from_value(changed).unwrap();
        assert_eq!(session.feed(&ready(changed)).unwrap_err(), ProtocolError::Identity);
        assert_eq!(session.phase(), Phase::Failed);
        assert_eq!(session.hello().unwrap_err(), ProtocolError::Terminal);
    }
    let mut session = Session::new(identity());
    session.hello().unwrap();
    let wrong =
        encode_worker(&WorkerMessage::Ready { protocol_version: 2, identity: identity(), limits: WorkerLimits::default() }).unwrap();
    assert_eq!(session.feed(&wrong).unwrap_err(), ProtocolError::Identity);
    let mut session = running();
    assert_eq!(session.feed(&ready(identity())).unwrap_err(), ProtocolError::State);
}

#[test]
fn one_outstanding_sequence_graceful_stop_and_terminal_failures() {
    let mut session = running();
    let ping = session.ping().unwrap();
    let expected = HostMessage::Ping { sequence: PolicyRevision::new(1).unwrap() };
    assert_eq!(Decoder::<HostMessage>::new().feed(&ping).unwrap().message, Some(expected));
    session.feed(&encode_worker(&WorkerMessage::Pong { sequence: PolicyRevision::new(1).unwrap() }).unwrap()).unwrap();
    session.stop().unwrap();
    session.feed(&encode_worker(&WorkerMessage::Stopped {}).unwrap()).unwrap();
    assert_eq!(session.phase(), Phase::Stopped);
    session.finish().unwrap();
    assert_eq!(session.ping().unwrap_err(), ProtocolError::Terminal);
    assert_eq!(session.feed(&ready(identity())).unwrap_err(), ProtocolError::Terminal);
    let mut session = running();
    session.ping().unwrap();
    assert_eq!(session.ping().unwrap_err(), ProtocolError::State);
    let mut session = running();
    session.ping().unwrap();
    let wrong = encode_worker(&WorkerMessage::Pong { sequence: PolicyRevision::new(2).unwrap() }).unwrap();
    assert_eq!(session.feed(&wrong).unwrap_err(), ProtocolError::State);
    let mut session = running();
    session.sequence = i64::MAX as u64;
    assert_eq!(session.ping().unwrap_err(), ProtocolError::Sequence);
    let mut session = running();
    assert_eq!(session.finish().unwrap_err(), ProtocolError::State);
    assert_eq!(session.phase(), Phase::Failed);
    let mut session = running();
    session.ping().unwrap();
    let failure = encode_worker(&WorkerMessage::Error { code: WorkerFailureCode::Internal }).unwrap();
    assert_eq!(session.feed(&failure).unwrap_err(), ProtocolError::Worker);
}

// Find the exact wire ceiling while accounting for the advertised ceiling's
// own decimal spelling. This is encoded-message sizing, not a fake I/O quota.
fn sized_limits(frames: usize, complete_exchange: bool) -> WorkerLimits {
    let mut bytes = 1024;
    for _ in 0..8 {
        let limits = WorkerLimits::new(frames, bytes).unwrap();
        let hello = encode_host(&HostMessage::Hello { protocol_version: 1, identity: identity(), limits }).unwrap();
        let target = hello.len() + if complete_exchange { ready_with_limits(identity(), limits).len() } else { 1 };
        if target == bytes {
            return limits;
        }
        bytes = target;
    }
    panic!("wire sizing did not converge");
}
#[test]
fn finite_aggregate_and_frame_work_caps_include_both_directions_and_partial_input() {
    let id = identity();
    let limits = sized_limits(2, true);
    let ready_bytes = ready_with_limits(id.clone(), limits);
    let mut session = Session::with_limits(id.clone(), limits.frames(), limits.bytes()).unwrap();
    let hello = session.hello().unwrap();
    let decoded = Decoder::<HostMessage>::new().feed(&hello).unwrap().message.unwrap();
    assert_eq!(decoded, HostMessage::Hello { protocol_version: 1, identity: id.clone(), limits });
    for byte in &ready_bytes {
        session.feed(std::slice::from_ref(byte)).unwrap();
    }
    assert_eq!(session.bytes, limits.bytes());
    assert_eq!(session.frames, 2);
    assert_eq!(session.ping().unwrap_err(), ProtocolError::Budget);
    let under = WorkerLimits::new(3, limits.bytes() - 1).unwrap();
    let mut session = Session::with_limits(id.clone(), under.frames(), under.bytes()).unwrap();
    session.hello().unwrap();
    assert_eq!(session.feed(&ready_with_limits(id.clone(), under)[..4]).unwrap_err(), ProtocolError::Budget);
    assert_eq!(session.decoder.body.capacity(), 0);
    let mut session = Session::with_limits(id.clone(), 1, MAX_WORKER_SESSION_BYTES).unwrap();
    session.hello().unwrap();
    assert_eq!(session.feed(&ready_with_limits(id.clone(), session.limits())).unwrap_err(), ProtocolError::Budget);
    assert_eq!(session.decoder.body.capacity(), 0);
    assert_eq!(session.decoder.prefix_len, 0);
    let limits = sized_limits(3, false);
    let mut session = Session::with_limits(id.clone(), limits.frames(), limits.bytes()).unwrap();
    session.hello().unwrap();
    let bytes = ready_with_limits(id.clone(), limits);
    let progress = session.feed(&bytes).unwrap();
    assert_eq!(progress.consumed, 1);
    assert!(progress.message.is_none());
    assert_eq!(session.decoder.prefix_len, 1);
    assert_eq!(session.decoder.body.capacity(), 0);
    assert_eq!(session.feed(&bytes[1..]).unwrap_err(), ProtocolError::Budget);
    for (frames, bytes) in [(0, 1), (1, 0), (MAX_WORKER_SESSION_FRAMES + 1, 1), (1, MAX_WORKER_SESSION_BYTES + 1)] {
        assert_eq!(Session::with_limits(id.clone(), frames, bytes).unwrap_err(), ProtocolError::Budget);
    }
}

#[test]
fn bounded_memory_encoder_rejects_excess_before_growing_past_logical_cap() {
    let value = "x".repeat(MAX_WORKER_FRAME_BYTES);
    assert_eq!(bounded_json(&value).unwrap_err(), ProtocolError::Length);
    let mut output = JsonBuffer { bytes: Vec::new(), exhausted: false, limit: MAX_WORKER_FRAME_BYTES };
    output.write_all(b"small").unwrap();
    assert!(output.write_all(&vec![0; MAX_WORKER_FRAME_BYTES]).is_err());
    assert_eq!(output.bytes, b"small");
}

#[test]
fn unsolicited_split_prefix_cannot_cross_a_later_host_request_boundary() {
    let ready_bytes = ready(identity());
    let pong = encode_worker(&WorkerMessage::Pong { sequence: PolicyRevision::new(1).unwrap() }).unwrap();
    for split in 1..4 {
        let mut session = Session::new(identity());
        assert_eq!(session.feed(&ready_bytes[..split]).unwrap_err(), ProtocolError::State);
        assert_eq!(session.decoder.prefix_len, 0);
        assert_eq!(session.decoder.body.capacity(), 0);
        assert_eq!(session.bytes, 0);
        assert_eq!(session.phase(), Phase::Failed);
        assert_eq!(session.hello().unwrap_err(), ProtocolError::Terminal);
        assert_eq!(session.feed(&ready_bytes[split..]).unwrap_err(), ProtocolError::Terminal);

        let mut session = running();
        let before = session.bytes;
        let capacity = session.decoder.body.capacity();
        assert_eq!(session.feed(&pong[..split]).unwrap_err(), ProtocolError::State);
        assert_eq!(session.decoder.prefix_len, 0);
        assert_eq!(session.decoder.body.capacity(), capacity);
        assert_eq!(session.decoder.body.len(), 0);
        assert_eq!(session.bytes, before);
        assert_eq!(session.ping().unwrap_err(), ProtocolError::Terminal);
        assert_eq!(session.feed(&pong[split..]).unwrap_err(), ProtocolError::Terminal);
        assert_eq!(session.phase(), Phase::Failed);
    }
}

#[test]
fn coalesced_unsolicited_suffix_cannot_be_deferred_until_a_future_ping() {
    let pong1 = encode_worker(&WorkerMessage::Pong { sequence: PolicyRevision::new(1).unwrap() }).unwrap();
    let pong2 = encode_worker(&WorkerMessage::Pong { sequence: PolicyRevision::new(2).unwrap() }).unwrap();
    let mut session = Session::new(identity());
    session.hello().unwrap();
    let mut supplied = ready(identity());
    supplied.extend_from_slice(&pong1);
    assert_eq!(session.feed(&supplied).unwrap_err(), ProtocolError::State);
    assert_eq!(session.decoder.prefix_len, 0);
    assert_eq!(session.decoder.body.len(), 0);
    assert_eq!(session.ping().unwrap_err(), ProtocolError::Terminal);
    assert_eq!(session.feed(&pong1).unwrap_err(), ProtocolError::Terminal);

    let mut session = running();
    session.ping().unwrap();
    let mut supplied = pong1;
    supplied.extend_from_slice(&pong2);
    assert_eq!(session.feed(&supplied).unwrap_err(), ProtocolError::State);
    assert_eq!(session.decoder.prefix_len, 0);
    assert_eq!(session.decoder.body.len(), 0);
    assert_eq!(session.ping().unwrap_err(), ProtocolError::Terminal);
    assert_eq!(session.feed(&pong2).unwrap_err(), ProtocolError::Terminal);
}

#[test]
fn advertised_limits_are_validated_and_ready_must_echo_exact_host_choice() {
    for (frames, bytes) in [(0, 1), (1, 0), (MAX_WORKER_SESSION_FRAMES + 1, 1), (1, MAX_WORKER_SESSION_BYTES + 1)] {
        assert!(WorkerLimits::new(frames, bytes).is_err());
        assert!(serde_json::from_value::<WorkerLimits>(json!({"lifetime_frames":frames,"lifetime_bytes":bytes})).is_err());
    }
    let limits = WorkerLimits::new(8, 16_384).unwrap();
    assert_eq!(serde_json::from_slice::<WorkerLimits>(&serde_json::to_vec(&limits).unwrap()).unwrap(), limits);
    for body in [
        r#"{"lifetime_frames":8,"lifetime_bytes":16384,"extra":0}"#,
        r#"{"lifetime_frames":8,"lifetime_frames":8,"lifetime_bytes":16384}"#,
        r#"{"lifetime_frames":8,"lifetime_bytes":16384,"lifetime_bytes":16384}"#,
    ] {
        assert!(serde_json::from_str::<WorkerLimits>(body).is_err());
    }
    let bytes = ready_with_limits(identity(), limits);
    let body = std::str::from_utf8(&bytes[4..]).unwrap();
    for replacement in [
        "\"lifetime_frames\":8,\"lifetime_frames\":8",
        "\"lifetime_frames\":8,\"extra\":0",
        "\"lifetime_frames\":0",
        "\"lifetime_frames\":4097",
    ] {
        rejected(body.replace("\"lifetime_frames\":8", replacement).as_bytes(), ProtocolError::Message);
    }
    for changed in [WorkerLimits::new(9, 16_384).unwrap(), WorkerLimits::new(8, 16_385).unwrap(), WorkerLimits::default()] {
        let mut session = Session::with_limits(identity(), limits.frames(), limits.bytes()).unwrap();
        session.hello().unwrap();
        assert_eq!(session.feed(&ready_with_limits(identity(), changed)).unwrap_err(), ProtocolError::Identity);
        assert_eq!(session.phase(), Phase::Failed);
        assert_eq!(session.ping().unwrap_err(), ProtocolError::Terminal);
    }
}
