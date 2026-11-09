//! The trust table (moved unchanged from `rbit_core::actions`), grants and isolation.
use super::*;

#[test]
fn pull_request_trust_table() {
    use AuthorStanding::{Suspended, Unknown, Vouched};
    let known = |access, suspended, is_author| TriggeringActor::Known { access, suspended, is_author };
    let trusted = RunTrust::Trusted;
    let waits = RunTrust::Untrusted { awaits_approval: true };
    let runs = RunTrust::Untrusted { awaits_approval: false };
    let accesses = [RepoAccess::None, RepoAccess::Read, RepoAccess::Write, RepoAccess::Admin];
    for author in [Unknown, Vouched, Suspended] {
        assert_eq!(classify_pull_request(TriggeringActor::Unknown, author), waits, "unknown pusher, {author:?}");
        for access in accesses {
            for is_author in [false, true] {
                // A suspended actor is never trusted and never skips the wait.
                assert_eq!(classify_pull_request(known(access, true, is_author), author), waits);
                let expected = if access.can_write() {
                    trusted
                } else if is_author && author == Vouched {
                    runs
                } else {
                    waits
                };
                assert_eq!(classify_pull_request(known(access, false, is_author), author), expected, "{access:?} {is_author} {author:?}");
            }
        }
    }
    // The cases §11.1 calls out.
    assert_eq!(classify_pull_request(known(RepoAccess::Read, false, true), Unknown), waits, "same-repository pull from a reader");
    assert_eq!(classify_pull_request(known(RepoAccess::Read, false, false), Vouched), waits, "someone else pushed a vouched author's head");
    assert_eq!(classify_pull_request(known(RepoAccess::Write, false, false), Unknown), trusted, "a writer pushed the head");
    assert_eq!(classify_pull_request(known(RepoAccess::Read, false, true), Suspended), waits);
}

#[test]
fn only_trusted_runs_are_granted_anything_whatever_their_approval() {
    let none = Grants { secrets_and_variables: false, write_token: false };
    let all = Grants { secrets_and_variables: true, write_token: true };
    assert_eq!(RunTrust::Trusted.grants(), all);
    for awaits_approval in [false, true] {
        assert_eq!(RunTrust::Untrusted { awaits_approval }.grants(), none);
    }
    assert_eq!((Grants::of_stored(true), Grants::of_stored(false)), (all, none), "approval is not stored in trust");
    assert_eq!(RunTrust::from_stored(false), RunTrust::Untrusted { awaits_approval: false });
    assert_eq!(RunTrust::from_stored(true), RunTrust::Trusted);
}

#[test]
fn untrusted_state_lives_under_its_pull_request_or_run() {
    let own = || "heads/main".to_owned();
    assert_eq!(Isolation::of(true, Some(3)), Isolation::Shared);
    assert_eq!(Isolation::of(true, None).namespaced_group("ci"), "ci");
    assert_eq!(Isolation::of(true, Some(3)).cache_write_scope(own, 41), "heads/main");
    let pull = Isolation::of(false, Some(3));
    assert_eq!((pull.namespaced_group("ci"), pull.cache_write_scope(own, 41)), ("pr-3/ci".to_owned(), "pull/3".to_owned()));
    let alone = Isolation::of(false, None);
    assert_eq!((alone.namespaced_group("ci"), alone.cache_write_scope(own, 41)), ("untrusted/ci".to_owned(), "untrusted/41".to_owned()));
}
