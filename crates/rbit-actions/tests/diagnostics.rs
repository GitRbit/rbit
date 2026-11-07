//! Generic composition can carry confidential, non-Debug payloads without disclosure.
use rbit_actions::claims::{Claim, Claimed, Resumed};

struct Payload(&'static str);

#[test]
fn diagnostics_do_not_require_or_format_workflow_payloads() {
    const SENTINEL: &str = "confidential-workflow-fixture";
    let claim = Claim::Task(Box::new(Payload(SENTINEL)));
    let replay = Resumed::Task(Box::new(Payload(SENTINEL)));
    let claimed = Claimed { job: Payload(SENTINEL), task: Payload(SENTINEL) };
    for diagnostic in [format!("{claim:?}"), format!("{replay:#?}"), format!("{claimed:#?}")] {
        assert!(!diagnostic.contains(SENTINEL));
        assert!(diagnostic.len() < 64, "debug output must not scale with the supplied payload");
    }
    // Formatting cannot consume, replace or redact the actual runner data.
    let Claim::Task(spec) = claim else { panic!("expected task") };
    assert_eq!(spec.0, SENTINEL);
    let Resumed::Task(spec) = replay else { panic!("expected replay") };
    assert_eq!(spec.0, SENTINEL);
    assert_eq!((claimed.job.0, claimed.task.0), (SENTINEL, SENTINEL));
}

struct RefuseFormatting;
impl std::fmt::Debug for RefuseFormatting {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        panic!("payload debug formatting is forbidden")
    }
}

#[test]
fn even_debug_payloads_are_never_traversed_and_empty_outcomes_need_no_bound() {
    assert_eq!(format!("{:?}", Claim::Task(Box::new(RefuseFormatting))), "Claim::Task([redacted])");
    assert_eq!(format!("{:?}", Resumed::Task(Box::new(RefuseFormatting))), "Resumed::Task([redacted])");
    assert_eq!(format!("{:?}", Claimed { job: RefuseFormatting, task: RefuseFormatting }), "Claimed { .. }");
    assert_eq!(format!("{:?}", Claim::<Payload>::Nothing), "Claim::Nothing");
    assert_eq!(format!("{:?}", Claim::<Payload>::Spent), "Claim::Spent");
    assert_eq!(format!("{:?}", Resumed::<Payload>::Spent), "Resumed::Spent");
    assert_eq!(format!("{:?}", Resumed::<Payload>::Unknown), "Resumed::Unknown");
}
