//! Small real model fixtures shared by independent application tests.

use rbit_collaboration::{
    IssueState,
    threads::{Issue, IssueId, IssueKind, IssueNumber, IssueText, IssueTitle},
};
use rbit_contracts::{AccountId, RepoId, Timestamp};

pub fn issue() -> Issue {
    Issue {
        id: IssueId(1),
        repo_id: RepoId(3),
        number: IssueNumber::new(1).unwrap(),
        kind: IssueKind::Issue,
        title: IssueTitle::parse("title").unwrap(),
        body: IssueText::body("").unwrap(),
        author: Some(AccountId(4)),
        state: IssueState::Open,
        state_reason: None,
        closed_at: None,
        closed_by: None,
        locked: false,
        comment_count: 0,
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    }
}
