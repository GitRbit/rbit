//! Authorization/error precedence and exact repository-wide optimistic state.
use rbit_access::RepoAccess;
use rbit_contracts::{AccountId, RepoId};
use rbit_wikis::{
    WikiGeneration, WikiPageId,
    policy::{WikiAccess, WikiExpected, WikiHead, WikiMutationState, WikiPolicyError, check_read, check_write},
};

fn access(level: RepoAccess) -> WikiAccess {
    WikiAccess { repository: RepoId(7), actor: Some(AccountId(1)), access: level, enabled: true, archived: false }
}

fn head(generation: i64, digit: char) -> WikiHead {
    WikiHead { generation: WikiGeneration::new(generation).unwrap(), revision: digit.to_string().repeat(40).parse().unwrap() }
}

fn existing() -> (WikiExpected, WikiMutationState) {
    let page = WikiPageId::new(11).unwrap();
    let head = head(3, 'a');
    (
        WikiExpected::Change { repository: RepoId(7), page, head },
        WikiMutationState { repository: RepoId(7), head: Some(head), page: Some(page), pending: false },
    )
}

#[test]
fn anonymous_read_is_allowed_only_with_current_read_access_and_enabled_feature() {
    let mut facts = access(RepoAccess::Read);
    facts.actor = None;
    assert_eq!(check_read(&facts, RepoId(7)), Ok(()));
    assert_eq!(check_read(&facts, RepoId(8)), Err(WikiPolicyError::NotFound));
    facts.archived = true;
    assert_eq!(check_read(&facts, RepoId(7)), Ok(()));
    facts.access = RepoAccess::None;
    assert_eq!(check_read(&facts, RepoId(7)), Err(WikiPolicyError::NotFound));
    facts.access = RepoAccess::Admin;
    facts.enabled = false;
    assert_eq!(check_read(&facts, RepoId(7)), Err(WikiPolicyError::NotFound));
}

#[test]
fn hidden_then_writer_then_archive_precede_pending_and_stale_state() {
    let (expected, mut current) = existing();
    current.pending = true;
    current.head = Some(head(9, 'b'));
    let mut facts = access(RepoAccess::None);
    facts.archived = true;
    assert_eq!(check_write(&facts, expected, &current), Err(WikiPolicyError::NotFound));
    facts.access = RepoAccess::Read;
    assert_eq!(check_write(&facts, expected, &current), Err(WikiPolicyError::Forbidden));
    facts.access = RepoAccess::Admin;
    assert_eq!(check_write(&facts, expected, &current), Err(WikiPolicyError::Archived));
    facts.archived = false;
    assert_eq!(check_write(&facts, expected, &current), Err(WikiPolicyError::Pending));
    facts.actor = None;
    assert_eq!(check_write(&facts, expected, &current), Err(WikiPolicyError::Forbidden), "bad composition cannot make an anonymous writer");
}

#[test]
fn creation_requires_exact_whole_wiki_head_and_unoccupied_path_slot() {
    let facts = access(RepoAccess::Write);
    let mut current = WikiMutationState { repository: facts.repository, head: None, page: None, pending: false };
    assert_eq!(check_write(&facts, WikiExpected::Create { repository: RepoId(7), head: None }, &current), Ok(WikiGeneration::INITIAL));
    current.head = Some(head(3, 'a'));
    assert_eq!(check_write(&facts, WikiExpected::Create { repository: RepoId(7), head: None }, &current), Err(WikiPolicyError::Conflict));
    assert_eq!(check_write(&facts, WikiExpected::Create { repository: RepoId(7), head: current.head }, &current).unwrap().get(), 4);
    current.page = WikiPageId::new(8);
    assert_eq!(
        check_write(&facts, WikiExpected::Create { repository: RepoId(7), head: current.head }, &current),
        Err(WikiPolicyError::Conflict)
    );
}

#[test]
fn changed_generation_oid_repository_page_or_missing_target_never_matches() {
    let facts = access(RepoAccess::Write);
    let (expected, current) = existing();
    assert_eq!(check_write(&facts, expected, &current).unwrap().get(), 4);
    assert_eq!(
        check_write(&facts, WikiExpected::Create { repository: RepoId(8), head: current.head }, &current),
        Err(WikiPolicyError::Conflict)
    );
    for variant in [
        WikiMutationState { repository: RepoId(8), ..current },
        WikiMutationState { page: WikiPageId::new(12), ..current },
        WikiMutationState { page: None, ..current },
        WikiMutationState { head: None, ..current },
        WikiMutationState { head: Some(head(4, 'a')), ..current },
        WikiMutationState { head: Some(head(3, 'b')), ..current },
    ] {
        assert_eq!(check_write(&facts, expected, &variant), Err(WikiPolicyError::Conflict));
    }
}

#[test]
fn rename_or_restore_does_not_reset_generation_and_exhaustion_never_wraps() {
    let facts = access(RepoAccess::Admin);
    let page = WikiPageId::new(11).unwrap();
    let head = head(i64::MAX, 'a');
    let current = WikiMutationState { repository: facts.repository, head: Some(head), page: Some(page), pending: false };
    assert_eq!(
        check_write(&facts, WikiExpected::Change { repository: RepoId(7), page, head }, &current),
        Err(WikiPolicyError::VersionExhausted)
    );
    let unoccupied = WikiMutationState { page: None, ..current };
    assert_eq!(
        check_write(&facts, WikiExpected::Create { repository: RepoId(7), head: Some(head) }, &unoccupied),
        Err(WikiPolicyError::VersionExhausted)
    );
    assert_eq!(
        check_write(&facts, WikiExpected::Create { repository: RepoId(7), head: None }, &unoccupied),
        Err(WikiPolicyError::Conflict)
    );
}

#[test]
fn renamed_projection_retains_page_identity_and_its_original_expected_head() {
    use rbit_contracts::Timestamp;
    use rbit_wikis::WikiPage;
    let page = WikiPage {
        repository: RepoId(7),
        id: WikiPageId::new(11).unwrap(),
        path: "Old".parse().unwrap(),
        title: "Original".parse().unwrap(),
        head: head(3, 'a'),
        created_at: Timestamp(1),
        updated_at: Timestamp(2),
    };
    let mut renamed = page.clone();
    renamed.path = "New".parse().unwrap();
    renamed.title = "Renamed".parse().unwrap();
    assert_eq!(page.id, renamed.id);
    let (_, mut current) = existing();
    current.head = Some(head(4, 'b'));
    assert_eq!(check_write(&access(RepoAccess::Write), renamed.expected_change(), &current), Err(WikiPolicyError::Conflict));
}
