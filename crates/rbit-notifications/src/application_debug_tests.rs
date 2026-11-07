use super::InboxApplication;
use rbit_contracts::AccountId;

struct NoDebug;
struct PanicDebug;
impl std::fmt::Debug for PanicDebug {
    fn fmt(&self, _: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        panic!("payload Debug must never run")
    }
}
#[test]
fn debug_has_no_payload_bounds_or_viewer_disclosure() {
    let value = InboxApplication { repository: &NoDebug, access: &NoDebug, user: AccountId(987_654_321) };
    assert_eq!(format!("{value:?}"), "InboxApplication { .. }");
    assert_eq!(format!("{value:#?}"), "InboxApplication { .. }");
}
#[test]
fn debug_never_invokes_even_panicking_payload_formatters() {
    let value = InboxApplication { repository: &PanicDebug, access: &PanicDebug, user: AccountId(987_654_321) };
    assert_eq!(format!("{value:?}"), "InboxApplication { .. }");
}
