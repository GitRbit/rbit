use super::*;
use crate::sessions::session_id;

const ALICE: AccountId = AccountId(1);

#[test]
fn unguarded_changes_end_every_session() {
    let ledger = PasswordChangeLedger::default();
    assert!(ledger.on_event(ALICE));
    assert!(ledger.lock().is_empty());
}

#[test]
fn a_guard_cannot_claim_a_future_change() {
    let ledger = PasswordChangeLedger::default();
    ledger.begin(ALICE);
    assert_eq!(ledger.finish(ALICE, true), FinishOutcome::default());
    assert!(ledger.on_event(ALICE), "a later change must still revoke sessions");
    assert!(ledger.lock().is_empty());
}

#[test]
fn own_change_defers_until_the_guard_finishes() {
    let ledger = PasswordChangeLedger::default();
    ledger.begin(ALICE);
    assert!(!ledger.on_event(ALICE), "deferred while the request runs");
    assert_eq!(ledger.finish(ALICE, true), FinishOutcome { end_others: true, end_all: false });
    assert!(ledger.lock().is_empty());
}

#[test]
fn change_elsewhere_during_a_guarded_request_still_ends_everything() {
    let ledger = PasswordChangeLedger::default();
    ledger.begin(ALICE);
    assert!(!ledger.on_event(ALICE));
    assert_eq!(ledger.finish(ALICE, false), FinishOutcome { end_others: false, end_all: true });
    assert!(ledger.on_event(ALICE), "later changes are unaffected");
}

#[test]
fn two_changes_one_of_them_ours() {
    let ledger = PasswordChangeLedger::default();
    ledger.begin(ALICE);
    assert!(!ledger.on_event(ALICE));
    assert!(!ledger.on_event(ALICE));
    assert_eq!(ledger.finish(ALICE, true), FinishOutcome { end_others: true, end_all: true });
    assert!(ledger.lock().is_empty());
}

#[test]
fn session_ids_are_short_hex_prefixes() {
    let id = session_id(&[0xab; 32]);
    assert_eq!(id.len(), 24);
    assert!(id.chars().all(|c| c == 'a' || c == 'b'));
}
