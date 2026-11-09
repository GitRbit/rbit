use super::*;

#[test]
fn steps_open_read_and_close() {
    let pending = PendingRegistrations::default();
    let token = pending.start("alice".to_owned());
    assert_eq!(pending.get(&token).as_deref(), Some("alice"));
    assert_eq!(pending.get(&token).as_deref(), Some("alice"), "reading keeps the step open");
    pending.end(&token);
    assert_eq!(pending.get(&token), None);
    assert_eq!(pending.get(&SessionToken::generate()), None);
}

#[test]
fn a_full_store_drops_the_oldest_step() {
    let pending = PendingRegistrations::default();
    let first = pending.start(0_usize);
    let rest: Vec<SessionToken> = (1..MAX_PENDING_REGISTRATIONS).map(|value| pending.start(value)).collect();
    let newest = pending.start(MAX_PENDING_REGISTRATIONS);
    assert_eq!(pending.get(&first), None);
    assert_eq!(pending.get(&rest[0]), Some(1));
    assert_eq!(pending.get(&newest), Some(MAX_PENDING_REGISTRATIONS));
}
