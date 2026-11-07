use rbit_notifications::{InboxPage, NotificationReason as Reason};
#[test]
fn stable_machine_reasons_ranks_and_original_labels_are_unchanged() {
    let expected = [
        ("manual", "Subscribed"),
        ("comment", "Commented"),
        ("reviewed", "Reviewed"),
        ("author", "Author"),
        ("mention", "Mentioned"),
        ("assigned", "Assigned"),
        ("review_requested", "Review requested"),
    ];
    for (rank, (machine, label)) in expected.iter().enumerate() {
        let reason = Reason::ALL[rank];
        assert_eq!(reason.as_str(), *machine);
        assert_eq!(reason.label(), *label);
        assert_eq!(Reason::parse(machine), Some(reason));
        if rank > 0 {
            assert!(reason > Reason::ALL[rank - 1]);
        }
    }
    for invalid in ["Manual", "review-requested", "", "mention\0", "<script>"] {
        assert_eq!(Reason::parse(invalid), None);
    }
}
#[test]
fn page_count_keeps_legacy_saturation_and_minimum() {
    for (total, per_page, expected) in [(0, 1, 1), (51, 50, 2), (u64::MAX, 1, u32::MAX)] {
        assert_eq!(InboxPage { notifications: vec![], total, page: 1, per_page }.page_count(), expected);
    }
}

#[test]
fn full_name_preserves_original_plain_text_for_empty_unicode_and_markup() {
    use rbit_collaboration::{
        IssueState,
        threads::{IssueId, IssueKind, IssueNumber},
    };
    use rbit_contracts::{AccountId, RepoId, Timestamp};
    use rbit_notifications::{Notification, NotificationReason};
    for (owner, repo, expected) in [("", "", "/"), ("é用户", "r🦀", "é用户/r🦀"), ("<script>", "a&b", "<script>/a&b")] {
        let row = Notification {
            id: -1,
            user: AccountId(2),
            issue: IssueId(3),
            repo: RepoId(4),
            reason: NotificationReason::Manual,
            unread: false,
            last_actor: None,
            updated_at: Timestamp(5),
            number: IssueNumber::new(1).unwrap(),
            kind: IssueKind::Issue,
            state: IssueState::Open,
            state_reason: None,
            title: String::new(),
            owner: owner.into(),
            repo_name: repo.into(),
        };
        assert_eq!(row.full_name(), expected);
    }
}
