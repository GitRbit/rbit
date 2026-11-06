#![cfg(feature = "applications")]
//! Independent Actions comment authority, source and fast-path ordering.
use rbit_collaboration::{
    comments::{Comment, CommentId, CommentSource, HumanCommentRepository, TaskCommentError, TaskCommentRepository, TaskComments},
    threads::{IssueId, IssueText},
};
use rbit_contracts::{AccountId, Timestamp};
use std::sync::Mutex;
fn row(source: CommentSource) -> Comment {
    Comment {
        id: CommentId(2),
        issue_id: IssueId(1),
        author: None,
        body: IssueText::comment("old\nbody").unwrap(),
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
        edited_at: None,
        source,
    }
}
struct State {
    calls: Vec<&'static str>,
    row: Option<Comment>,
    vanished: bool,
}
impl Default for State {
    fn default() -> Self {
        Self { calls: Vec::new(), row: Some(row(CommentSource::Actions)), vanished: false }
    }
}
struct Port<'a>(&'a Mutex<State>);
fn factory(state: &Mutex<State>) -> Port<'_> {
    state.lock().unwrap().calls.push("construct");
    Port(state)
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl HumanCommentRepository for Port<'_> {
    type Error = &'static str;
    async fn find(&self, _: IssueId, _: CommentId) -> Result<Option<Comment>, Self::Error> {
        let mut state = self.0.lock().unwrap();
        state.calls.push("find");
        Ok(state.row.clone())
    }
    async fn add(&self, _: IssueId, _: AccountId, _: &IssueText, _: Timestamp) -> Result<Option<Comment>, Self::Error> {
        panic!("human insert")
    }
    async fn edit(&self, _: CommentId, body: &IssueText, at: Timestamp) -> Result<Option<Comment>, Self::Error> {
        assert_eq!(at, Timestamp(9));
        let mut state = self.0.lock().unwrap();
        state.calls.push("edit");
        Ok((!state.vanished).then(|| Comment { body: body.clone(), ..row(CommentSource::Actions) }))
    }
    async fn delete(&self, _: CommentId) -> Result<Option<Comment>, Self::Error> {
        let mut state = self.0.lock().unwrap();
        state.calls.push("delete");
        Ok(if state.vanished { None } else { state.row.take() })
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like the database adapter")]
impl TaskCommentRepository for Port<'_> {
    async fn add_actions(&self, _: IssueId, body: &IssueText, at: Timestamp) -> Result<Option<Comment>, Self::Error> {
        assert_eq!(at, Timestamp(9));
        self.0.lock().unwrap().calls.push("add_actions");
        Ok(Some(Comment { body: body.clone(), ..row(CommentSource::Actions) }))
    }
}
fn app(allowed: bool, archived: bool) -> TaskComments {
    TaskComments::new(IssueId(1), allowed, archived)
}
fn clock(state: &Mutex<State>) -> Timestamp {
    state.lock().unwrap().calls.push("clock");
    Timestamp(9)
}
#[tokio::test]
async fn denied_creation_and_deletion_do_no_store_work_but_edit_keeps_legacy_write_handle() {
    let state = Mutex::new(State::default());
    assert!(matches!(app(false, true).create("", || factory(&state), || clock(&state)).await, Err(TaskCommentError::Forbidden)));
    assert!(matches!(app(false, true).delete(CommentId(2), || factory(&state)).await, Err(TaskCommentError::Forbidden)));
    assert_eq!(state.lock().unwrap().calls, Vec::<&str>::new());
    assert!(matches!(
        app(false, true).edit(CommentId(2), "", || factory(&state), || clock(&state)).await,
        Err(TaskCommentError::Forbidden)
    ));
    assert_eq!(state.lock().unwrap().calls, ["construct"]);
}
#[tokio::test]
async fn creation_archive_and_text_reject_before_store_and_accepted_create_keeps_clock_order() {
    let state = Mutex::new(State::default());
    assert!(matches!(app(true, true).create("", || factory(&state), || clock(&state)).await, Err(TaskCommentError::Archived)));
    assert!(matches!(app(true, false).create("", || factory(&state), || clock(&state)).await, Err(TaskCommentError::Text(_))));
    assert_eq!(state.lock().unwrap().calls, Vec::<&str>::new());
    let result = app(true, false).create("new\r\nbody", || factory(&state), || clock(&state)).await.unwrap();
    assert_eq!((result.author, result.source, result.body.as_str()), (None, CommentSource::Actions, "new\nbody"));
    assert_eq!(state.lock().unwrap().calls, ["construct", "clock", "add_actions"]);
}
#[tokio::test]
async fn missing_then_source_then_archive_precede_edit_text_and_clock() {
    for (row, expected) in [(None, "missing"), (Some(row(CommentSource::User)), "source"), (Some(row(CommentSource::Actions)), "archive")] {
        let state = Mutex::new(State { row, ..State::default() });
        let result = app(true, true).edit(CommentId(2), "", || factory(&state), || panic!("clock")).await;
        assert!(matches!(
            (expected, result),
            ("missing", Err(TaskCommentError::NotFound))
                | ("source", Err(TaskCommentError::Forbidden))
                | ("archive", Err(TaskCommentError::Archived))
        ));
        assert_eq!(state.lock().unwrap().calls, ["construct", "construct", "find"]);
    }
}
#[tokio::test]
async fn normalized_noop_and_changed_edits_retain_two_handles_and_single_scoped_lookup() {
    let state = Mutex::new(State::default());
    let result = app(true, false).edit(CommentId(2), "old\r\nbody", || factory(&state), || panic!("clock")).await.unwrap();
    assert!(result.previous_body.is_none());
    assert_eq!(state.lock().unwrap().calls, ["construct", "construct", "find"]);
    state.lock().unwrap().calls.clear();
    let result = app(true, false).edit(CommentId(2), "changed", || factory(&state), || clock(&state)).await.unwrap();
    assert_eq!(result.previous_body.unwrap().as_str(), "old\nbody");
    assert_eq!(state.lock().unwrap().calls, ["construct", "construct", "find", "clock", "edit"]);
}
#[tokio::test]
async fn deletion_constructs_write_handle_only_after_source_and_archive_and_keeps_race_success() {
    let state = Mutex::new(State { vanished: true, ..State::default() });
    assert!(app(true, false).delete(CommentId(2), || factory(&state)).await.unwrap().is_none());
    assert_eq!(state.lock().unwrap().calls, ["construct", "find", "construct", "delete"]);
    let state = Mutex::new(State { row: Some(row(CommentSource::User)), ..State::default() });
    assert!(matches!(app(true, true).delete(CommentId(2), || factory(&state)).await, Err(TaskCommentError::Forbidden)));
    assert_eq!(state.lock().unwrap().calls, ["construct", "find"]);
}
#[tokio::test]
async fn unpolled_task_operations_never_construct_a_store() {
    let state = Mutex::new(State::default());
    let app = app(true, false);
    drop(app.create("body", || factory(&state), || clock(&state)));
    drop(app.edit(CommentId(2), "body", || factory(&state), || clock(&state)));
    drop(app.delete(CommentId(2), || factory(&state)));
    assert_eq!(state.lock().unwrap().calls, Vec::<&str>::new());
}
