//! Exact protocol wire and refusal acceptance fixtures.

use serde::{Serialize, de::DeserializeOwned};

use super::{
    Backend, Capabilities, DeclareRequest, DeclareResponse, DeclaredLabel, Endpoint, ErrorBody, ErrorCode, Intervals, LogsQuery,
    LogsResponse, MAX_CAPACITY, MAX_IDEMPOTENCY_KEY_BYTES, MAX_OUTPUT_BYTES, MAX_OUTPUT_KEY_BYTES, MAX_POST_STEP_NAME_BYTES,
    MAX_STEP_SUMMARY_BYTES, PROTOCOL_VERSION, PollRequest, RegisterRequest, RegisterResponse, RunnerId, SUPPORTED_PROTOCOL_VERSIONS,
    StepState, StepStatus, TaskId, TaskResult, TaskStateRequest, TaskStateResponse, TaskTermination, UnregisterRequest, ValidationError,
    valid_artifact_name, valid_idempotency_key, valid_runner_credential, valid_task_token,
};
use crate::secret::Secret;
use std::{collections::BTreeMap, fmt};

/// A change that makes a message invalid, and part of the message it is refused with.
pub(crate) type Refusal<T> = (fn(&mut T), &'static str);

/// Serialises to exactly `expected`, and `expected` deserialises back to the value.
#[track_caller]
pub(crate) fn golden<T: Serialize + DeserializeOwned + PartialEq + fmt::Debug>(value: &T, expected: &str) {
    assert_eq!(serde_json::to_string(value).unwrap(), expected);
    assert_eq!(&serde_json::from_str::<T>(expected).unwrap(), value);
}

fn capabilities() -> Capabilities {
    Capabilities {
        backends: vec![Backend::Container, Backend::Host],
        os: "linux".into(),
        arch: "x86_64".into(),
        capacity: 2,
        version: "0.2.0".into(),
        ephemeral: false,
        exclusive_labels: Vec::new(),
    }
}

fn labels() -> Vec<DeclaredLabel> {
    vec![
        DeclaredLabel { name: "ubuntu-latest".into(), backend: Backend::Container },
        DeclaredLabel { name: "self-hosted".into(), backend: Backend::Host },
    ]
}

const CAPABILITIES: &str =
    r#"{"backends":["container","host"],"os":"linux","arch":"x86_64","capacity":2,"version":"0.2.0","ephemeral":false}"#;
const LABELS: &str = r#"[{"name":"ubuntu-latest","backend":"container"},{"name":"self-hosted","backend":"host"}]"#;
const CREDENTIAL: &str = "rbrn_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

#[test]
fn golden_registration() {
    let request = RegisterRequest {
        token: Secret::new("registration-token"),
        name: "build-1".into(),
        labels: labels(),
        capabilities: capabilities(),
    };
    request.validate().unwrap();
    golden(&request, &format!(r#"{{"token":"registration-token","name":"build-1","labels":{LABELS},"capabilities":{CAPABILITIES}}}"#));

    let response = RegisterResponse {
        runner_id: RunnerId(7),
        credential: Secret::new(CREDENTIAL),
        intervals: Intervals { long_poll_seconds: 25, heartbeat_seconds: 10 },
    };
    response.validate().unwrap();
    golden(
        &response,
        &format!(r#"{{"runner_id":7,"credential":"{CREDENTIAL}","intervals":{{"long_poll_seconds":25,"heartbeat_seconds":10}}}}"#),
    );

    let declare = DeclareRequest { name: "build-1".into(), labels: labels(), capabilities: capabilities() };
    declare.validate().unwrap();
    golden(&declare, &format!(r#"{{"name":"build-1","labels":{LABELS},"capabilities":{CAPABILITIES}}}"#));
    golden(
        &DeclareResponse {
            runner_id: RunnerId(7),
            intervals: Intervals { long_poll_seconds: 25, heartbeat_seconds: 10 },
            shared_cache: false,
        },
        r#"{"runner_id":7,"intervals":{"long_poll_seconds":25,"heartbeat_seconds":10}}"#,
    );
    // Actions: shared cache. Older runners read the answer without it.
    golden(
        &DeclareResponse {
            runner_id: RunnerId(7),
            intervals: Intervals { long_poll_seconds: 25, heartbeat_seconds: 10 },
            shared_cache: true,
        },
        r#"{"runner_id":7,"intervals":{"long_poll_seconds":25,"heartbeat_seconds":10},"shared_cache":true}"#,
    );
    golden(&UnregisterRequest {}, "{}");
}

#[test]
fn golden_poll_state_and_logs() {
    let poll = PollRequest { free_slots: 1 };
    poll.validate().unwrap();
    golden(&poll, r#"{"free_slots":1}"#);

    let state = TaskStateRequest {
        steps: vec![
            StepState { index: 0, status: StepStatus::Success, started_at: Some(100), stopped_at: Some(102), log_index: 0, log_length: 12 },
            StepState { index: 1, status: StepStatus::Running, started_at: Some(102), stopped_at: None, log_index: 12, log_length: 3 },
        ],
        outputs: BTreeMap::from([("version".to_owned(), "1.2.3".to_owned())]),
        result: None,
        summaries: BTreeMap::new(),
        post_steps: BTreeMap::new(),
        environment_url: None,
        termination: None,
    };
    state.validate().unwrap();
    assert!(!state.is_final());
    golden(
        &state,
        concat!(
            r#"{"steps":[{"index":0,"status":"success","started_at":100,"stopped_at":102,"log_index":0,"log_length":12},"#,
            r#"{"index":1,"status":"running","started_at":102,"stopped_at":null,"log_index":12,"log_length":3}],"#,
            r#""outputs":{"version":"1.2.3"},"result":null}"#
        ),
    );
    let done = TaskStateRequest {
        steps: Vec::new(),
        outputs: BTreeMap::new(),
        result: Some(TaskResult::Cancelled),
        summaries: BTreeMap::new(),
        post_steps: BTreeMap::new(),
        environment_url: None,
        termination: None,
    };
    assert!(done.is_final());
    golden(&done, r#"{"steps":[],"outputs":{},"result":"cancelled"}"#);
    // Actions: step summaries. Older servers read the call the same way; newer ones keep them.
    let summarised = TaskStateRequest { summaries: BTreeMap::from([(0, "# Done\n".to_owned())]), ..done };
    golden(&summarised, r##"{"steps":[],"outputs":{},"result":"cancelled","summaries":{"0":"# Done\n"}}"##);
    let old: TaskStateRequest = serde_json::from_str(r#"{"steps":[],"outputs":{},"result":null}"#).unwrap();
    assert!(old.summaries.is_empty());
    // Agent loop: termination records, on the final call only; older peers leave them out.
    assert!(old.termination.is_none());
    let ended = TaskStateRequest {
        termination: Some(TaskTermination { reason: "step_failed".to_owned(), step: Some(1), exit_code: Some(2) }),
        ..done.clone()
    };
    ended.validate().unwrap();
    golden(&ended, r#"{"steps":[],"outputs":{},"result":"cancelled","termination":{"reason":"step_failed","step":1,"exit_code":2}}"#);
    let early = TaskStateRequest { result: None, ..ended };
    assert!(early.validate().is_err(), "a termination before the final call");
    golden(&TaskStateResponse { cancel: true }, r#"{"cancel":true}"#);
    golden(&LogsQuery { from: 42 }, r#"{"from":42}"#);
    golden(&LogsResponse { next_line: 57 }, r#"{"next_line":57}"#);
}

#[test]
fn golden_errors() {
    golden(&ErrorBody::new(ErrorCode::Forbidden, "not your task"), r#"{"code":"forbidden","message":"not your task"}"#);
    golden(
        &ErrorBody::unsupported_protocol(),
        r#"{"code":"unsupported_protocol","message":"this server does not speak the runner's protocol version","supported_versions":[1]}"#,
    );
    golden(&ErrorBody::log_gap(9), r#"{"code":"conflict","message":"the log continues at line 9","next_line":9}"#);
    let invalid: ErrorBody = ValidationError::new("bad").into();
    assert_eq!((invalid.code, invalid.message.as_str()), (ErrorCode::BadRequest, "bad"));
    let codes = [
        ErrorCode::BadRequest,
        ErrorCode::Unauthenticated,
        ErrorCode::Forbidden,
        ErrorCode::NotFound,
        ErrorCode::Conflict,
        ErrorCode::TooLarge,
        ErrorCode::UnsupportedProtocol,
        ErrorCode::Throttled,
        ErrorCode::Unavailable,
    ];
    let names = codes.map(|code| serde_json::to_string(&code).unwrap());
    assert_eq!(
        names,
        [
            r#""bad_request""#,
            r#""unauthenticated""#,
            r#""forbidden""#,
            r#""not_found""#,
            r#""conflict""#,
            r#""too_large""#,
            r#""unsupported_protocol""#,
            r#""throttled""#,
            r#""unavailable""#
        ]
    );
    assert_eq!(codes.map(ErrorCode::status), [400, 401, 403, 404, 409, 413, 426, 429, 503]);
}

#[test]
fn envelopes_are_forward_compatible_and_enums_closed() {
    // A newer server or runner may add fields within v1 …
    let error: ErrorBody = serde_json::from_str(r#"{"code":"quota_exceeded","message":"m","hint":"later"}"#).unwrap();
    assert_eq!((error.code, error.code.status()), (ErrorCode::Unknown, 503));
    let state: TaskStateResponse = serde_json::from_str(r#"{"cancel":false,"reason":"x"}"#).unwrap();
    assert!(!state.cancel);
    let caps: Capabilities = serde_json::from_str(&CAPABILITIES.replace('}', r#","gpus":2}"#)).unwrap();
    assert_eq!(caps, capabilities());
    // … but an unknown backend, status or result is refused rather than guessed.
    assert!(serde_json::from_str::<Backend>(r#""vm""#).is_err());
    assert!(serde_json::from_str::<StepStatus>(r#""paused""#).is_err());
    assert!(serde_json::from_str::<TaskResult>(r#""skipped""#).is_err());
}

#[test]
fn only_the_container_backend_isolates_jobs() {
    assert!(Backend::Container.isolates_jobs());
    assert!(!Backend::Host.isolates_jobs());
}

#[test]
fn runner_validation() {
    let request = |change: fn(&mut RegisterRequest)| {
        let mut request =
            RegisterRequest { token: Secret::new("t"), name: "build-1".into(), labels: labels(), capabilities: capabilities() };
        change(&mut request);
        request.validate().map_err(|error| error.message)
    };
    assert!(request(|_| ()).is_ok());
    let refused: [Refusal<RegisterRequest>; 12] = [
        (|r| r.token = Secret::default(), "registration token"),
        (|r| r.token = Secret::new("x".repeat(257)), "registration token"),
        (|r| r.name = String::new(), "runner name"),
        (|r| r.name = "a\nb".into(), "runner name"),
        (|r| r.labels.clear(), "1 to 64 labels"),
        (|r| r.labels[0].name = "has space".into(), "label `has space`"),
        (|r| r.labels[1].name = "ubuntu-latest".into(), "must not repeat"),
        (|r| r.capabilities.backends = vec![Backend::Container], "label `self-hosted` uses a backend"),
        (|r| r.capabilities.backends = vec![Backend::Host, Backend::Host], "backends must not repeat"),
        (|r| r.capabilities.capacity = 0, "capacity"),
        (|r| r.capabilities.capacity = MAX_CAPACITY + 1, "capacity"),
        (|r| r.capabilities.os = "linux; rm".into(), "`os`"),
    ];
    for (change, message) in refused {
        let error = request(change).unwrap_err();
        assert!(error.contains(message), "{error:?} should mention {message:?}");
    }
    assert!(request(|r| r.labels = (0..65).map(|i| DeclaredLabel { name: format!("l{i}"), backend: Backend::Host }).collect()).is_err());
}

#[test]
fn token_and_key_shapes() {
    assert!(valid_runner_credential(CREDENTIAL));
    assert!(valid_task_token(&format!("rbat_{}", "a-_9".repeat(10) + "xyz")));
    assert!(!valid_task_token(CREDENTIAL));
    assert!(!valid_runner_credential(&CREDENTIAL[..CREDENTIAL.len() - 1]));
    assert!(!valid_runner_credential(&CREDENTIAL.replace('A', "+")));
    assert!(valid_idempotency_key("0b1c2d3e-0000-4000-8000-000000000000"));
    assert!(!valid_idempotency_key(""));
    assert!(!valid_idempotency_key(&"k".repeat(MAX_IDEMPOTENCY_KEY_BYTES + 1)));
    assert!(!valid_idempotency_key("a b"));
    let response = RegisterResponse {
        runner_id: RunnerId(1),
        credential: Secret::new("rbrn_short"),
        intervals: Intervals { long_poll_seconds: 25, heartbeat_seconds: 10 },
    };
    assert!(response.validate().is_err());
}

#[test]
fn interval_poll_and_state_caps() {
    assert!(Intervals { long_poll_seconds: 26, heartbeat_seconds: 10 }.validate().is_err());
    assert!(Intervals { long_poll_seconds: 25, heartbeat_seconds: 11 }.validate().is_err());
    assert!(Intervals { long_poll_seconds: 0, heartbeat_seconds: 1 }.validate().is_err());
    assert!(PollRequest { free_slots: 0 }.validate().is_err());

    let step = StepState { index: 0, status: StepStatus::Running, started_at: Some(5), stopped_at: None, log_index: 0, log_length: 0 };
    let state = |steps: Vec<StepState>, outputs: BTreeMap<String, String>, result| {
        TaskStateRequest { steps, outputs, result, summaries: BTreeMap::new(), post_steps: BTreeMap::new(), environment_url: None, termination: None }
            .validate()
    };
    assert!(state(vec![step], BTreeMap::new(), None).is_ok());
    assert!(state(vec![step], BTreeMap::new(), Some(TaskResult::Failure)).is_err(), "a final state with a running step");
    assert!(state(vec![step, step], BTreeMap::new(), None).is_err(), "a repeated index");
    assert!(state(vec![StepState { index: 511, ..step }], BTreeMap::new(), None).is_ok(), "post steps follow the template's");
    assert!(state(vec![StepState { index: 512, ..step }], BTreeMap::new(), None).is_err());
    assert!(state(vec![StepState { stopped_at: Some(4), ..step }], BTreeMap::new(), None).is_err());
    assert!(state(vec![StepState { log_index: u64::MAX, log_length: 1, ..step }], BTreeMap::new(), None).is_err());
    let steps = (0..513).map(|index| StepState { index, ..step }).collect();
    assert!(state(steps, BTreeMap::new(), None).is_err());

    let outputs = |name: &str, bytes: usize| BTreeMap::from([(name.to_owned(), "x".repeat(bytes))]);
    assert!(state(Vec::new(), outputs("ok_name-1", MAX_OUTPUT_BYTES - 9), None).is_ok());
    assert!(state(Vec::new(), outputs("ok_name-1", MAX_OUTPUT_BYTES - 8), None).is_err());
    assert!(state(Vec::new(), outputs("1st", 1), None).is_err());
    assert!(state(Vec::new(), outputs(&"k".repeat(MAX_OUTPUT_KEY_BYTES + 1), 1), None).is_err());

    // Actions: step summaries.
    let summarised = |summaries: &[(u32, usize)]| TaskStateRequest {
        steps: Vec::new(),
        outputs: BTreeMap::new(),
        result: Some(TaskResult::Success),
        summaries: summaries.iter().map(|(index, bytes)| (*index, "x".repeat(*bytes))).collect(),
        post_steps: BTreeMap::new(),
        environment_url: None,
        termination: None,
    };
    assert!(summarised(&[(0, MAX_STEP_SUMMARY_BYTES)]).validate().is_ok());
    assert!(summarised(&[(0, MAX_STEP_SUMMARY_BYTES + 1)]).validate().is_err());
    assert!(summarised(&[(512, 1)]).validate().is_err(), "past the last step");

    // Actions: third-party actions.
    let posted = |index: u32, name: &str| TaskStateRequest { post_steps: BTreeMap::from([(index, name.to_owned())]), ..summarised(&[]) };
    assert!(posted(3, "Post Run actions/setup-node@v4").validate().is_ok());
    golden(&posted(3, "Post x"), r#"{"steps":[],"outputs":{},"result":"success","post_steps":{"3":"Post x"}}"#);
    assert!(posted(512, "Post x").validate().is_err());
    assert!(posted(3, "").validate().is_err());
    assert!(
        posted(
            3, "a
b"
        )
        .validate()
        .is_err()
    );
    assert!(posted(3, &"x".repeat(MAX_POST_STEP_NAME_BYTES + 1)).validate().is_err());
    let four: Vec<_> = (0..4).map(|index| (index, MAX_STEP_SUMMARY_BYTES)).collect();
    assert!(summarised(&four).validate().is_ok());
    let five: Vec<_> = (0..5).map(|index| (index, MAX_STEP_SUMMARY_BYTES)).collect();
    assert!(summarised(&five).validate().is_err(), "the task's budget");
    let largest = TaskStateRequest { outputs: BTreeMap::from([("big".to_owned(), "x".repeat(MAX_OUTPUT_BYTES - 3))]), ..summarised(&four) };
    assert!(serde_json::to_vec(&largest).unwrap().len() < Endpoint::TaskState.body_cap(), "the largest valid call fits its cap");
}

// Actions: artifacts (A).
#[test]
fn artifact_names_never_walk_up() {
    for name in ["dist", "dist-linux_x86.64", ".hidden", "...", "a..b"] {
        assert!(valid_artifact_name(name), "{name}");
    }
    for name in ["", ".", "..", "a/b", "a\\b", "../x", "a?b", "a b", "é"] {
        assert!(!valid_artifact_name(name), "{name}");
    }
    assert!(valid_artifact_name(&"x".repeat(128)) && !valid_artifact_name(&"x".repeat(129)));
}

#[test]
fn endpoints_and_caps() {
    let routes = Endpoint::ALL.map(|endpoint| (endpoint.path(), endpoint.body_cap()));
    assert_eq!(
        routes,
        [
            ("register", 65_536),
            ("declare", 65_536),
            ("poll", 4_096),
            ("tasks/{id}/state", 4_194_304),
            ("tasks/{id}/logs", 8_388_608),
            ("unregister", 1_024)
        ]
    );
    assert_eq!((PROTOCOL_VERSION, SUPPORTED_PROTOCOL_VERSIONS), (1, [1]));
    assert_eq!(TaskId(3).to_string(), "3");
}

#[test]
fn secrets_never_appear_in_debug_output() {
    let request =
        RegisterRequest { token: Secret::new("registration-token"), name: "n".into(), labels: labels(), capabilities: capabilities() };
    let response = RegisterResponse {
        runner_id: RunnerId(1),
        credential: Secret::new(CREDENTIAL),
        intervals: Intervals { long_poll_seconds: 1, heartbeat_seconds: 1 },
    };
    let debug = format!("{request:?} {response:?}");
    assert!(!debug.contains("registration-token") && !debug.contains(CREDENTIAL), "{debug}");
    assert!(debug.contains("Secret(***)"));
}
