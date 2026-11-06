#![cfg(feature = "applications")]
//! Independent human-comment ordering and fast-path contracts.

use rbit_collaboration::{
    comments::{Comment, CommentId, CommentSource, HumanCommentError, HumanCommentRepository, HumanComments},
    threads::{IssueId, IssueText},
};
use rbit_contracts::{AccountId, Timestamp};
use std::sync::Mutex;

fn comment() -> Comment {
    Comment {
        id: CommentId(2),
        issue_id: IssueId(1),
        author: Some(AccountId(3)),
        body: IssueText::comment("a\nb").unwrap(),
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
        edited_at: None,
        source: CommentSource::User,
    }
}
struct State {
    calls: Vec<&'static str>,
    row: Option<Comment>,
    vanished: bool,
}
impl Default for State {
    fn default() -> Self {
        Self { calls: Vec::new(), row: Some(comment()), vanished: false }
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
    async fn find(&self, issue: IssueId, id: CommentId) -> Result<Option<Comment>, Self::Error> {
        assert_eq!((issue, id), (IssueId(1), CommentId(2)));
        let mut state = self.0.lock().unwrap();
        state.calls.push("find");
        Ok(state.row.clone())
    }
    async fn add(&self, issue: IssueId, actor: AccountId, body: &IssueText, at: Timestamp) -> Result<Option<Comment>, Self::Error> {
        assert_eq!((issue, actor, at), (IssueId(1), AccountId(3), Timestamp(9)));
        let mut state = self.0.lock().unwrap();
        state.calls.push("add");
        Ok(Some(Comment { body: body.clone(), ..comment() }))
    }
    async fn edit(&self, _: CommentId, body: &IssueText, at: Timestamp) -> Result<Option<Comment>, Self::Error> {
        assert_eq!(at, Timestamp(9));
        let mut state = self.0.lock().unwrap();
        state.calls.push("edit");
        Ok((!state.vanished).then(|| Comment { body: body.clone(), ..comment() }))
    }
    async fn delete(&self, _: CommentId) -> Result<Option<Comment>, Self::Error> {
        let mut state = self.0.lock().unwrap();
        state.calls.push("delete");
        Ok(if state.vanished { None } else { state.row.take() })
    }
}
fn app(archived: bool) -> HumanComments {
    HumanComments::new(IssueId(1), AccountId(3), archived)
}
fn spend(state: &Mutex<State>) -> impl FnOnce() -> Result<(), &'static str> + '_ {
    move || {
        state.lock().unwrap().calls.push("spend");
        Ok(())
    }
}
fn clock(state: &Mutex<State>) -> Timestamp {
    state.lock().unwrap().calls.push("clock");
    Timestamp(9)
}
fn allowed(state: &Mutex<State>, value: bool) -> bool {
    state.lock().unwrap().calls.push("permission");
    value
}

#[tokio::test]
async fn create_rejection_and_unpolled_futures_do_not_construct_store() {
    let state = Mutex::new(State::default());
    assert!(matches!(
        app(true).create("", false, || factory(&state), spend(&state), || clock(&state)).await,
        Err(HumanCommentError::Forbidden)
    ));
    assert!(matches!(
        app(true).create("", true, || factory(&state), spend(&state), || clock(&state)).await,
        Err(HumanCommentError::Archived)
    ));
    assert!(matches!(
        app(false).create("", true, || factory(&state), spend(&state), || clock(&state)).await,
        Err(HumanCommentError::Text(_))
    ));
    assert!(matches!(
        app(false).create("valid", true, || factory(&state), || Err("limited"), || clock(&state)).await,
        Err(HumanCommentError::Throttle("limited"))
    ));
    let app = app(false);
    drop(app.create("valid", true, || factory(&state), spend(&state), || clock(&state)));
    drop(app.edit(CommentId(2), "valid", || factory(&state), |_| allowed(&state, true), spend(&state), || clock(&state)));
    drop(app.delete(CommentId(2), || factory(&state), |_| allowed(&state, true)));
    assert_eq!(state.lock().unwrap().calls, Vec::<&str>::new());
}
#[tokio::test]
async fn create_keeps_spend_store_clock_order_and_comment_normalization() {
    let state = Mutex::new(State::default());
    let result = app(false).create("a\r\nb", true, || factory(&state), spend(&state), || clock(&state)).await.unwrap();
    assert_eq!(result.body.as_str(), "a\nb");
    assert_eq!(state.lock().unwrap().calls, ["spend", "construct", "clock", "add"]);
}
#[tokio::test]
async fn edit_lookup_precedes_permission_archive_and_text_validation() {
    let state = Mutex::new(State { row: None, ..State::default() });
    let result = app(true).edit(CommentId(2), "", || factory(&state), |_| panic!("permission"), spend(&state), || clock(&state)).await;
    assert!(matches!(result, Err(HumanCommentError::NotFound)));
    assert_eq!(state.lock().unwrap().calls, ["construct", "find"]);
    for (permit, expected) in [(false, "forbidden"), (true, "archived")] {
        let state = Mutex::new(State::default());
        let result =
            app(true).edit(CommentId(2), "", || factory(&state), |_| allowed(&state, permit), spend(&state), || clock(&state)).await;
        assert!(matches!(
            (expected, result),
            ("forbidden", Err(HumanCommentError::Forbidden)) | ("archived", Err(HumanCommentError::Archived))
        ));
        assert_eq!(state.lock().unwrap().calls, ["construct", "find", "permission"]);
    }
}
#[tokio::test]
async fn normalized_unchanged_edit_avoids_throttle_clock_write_and_previous_body() {
    let state = Mutex::new(State::default());
    let result = app(false)
        .edit(CommentId(2), "a\r\nb", || factory(&state), |_| allowed(&state, true), || panic!("spend"), || panic!("clock"))
        .await;
    let result: Result<_, HumanCommentError<&str, ()>> = result;
    let result = result.unwrap();
    assert_eq!(result.comment, comment());
    assert!(result.previous_body.is_none());
    assert_eq!(state.lock().unwrap().calls, ["construct", "find", "permission"]);
}
#[tokio::test]
async fn changed_edit_reuses_store_and_retains_old_body_only_after_successful_write() {
    let state = Mutex::new(State::default());
    let result =
        app(false).edit(CommentId(2), "new", || factory(&state), |_| allowed(&state, true), spend(&state), || clock(&state)).await.unwrap();
    assert_eq!(result.previous_body.unwrap().as_str(), "a\nb");
    assert_eq!(result.comment.body.as_str(), "new");
    assert_eq!(state.lock().unwrap().calls, ["construct", "find", "permission", "spend", "clock", "edit"]);
    let state = Mutex::new(State { vanished: true, ..State::default() });
    assert!(matches!(
        app(false).edit(CommentId(2), "new", || factory(&state), |_| true, spend(&state), || clock(&state)).await,
        Err(HumanCommentError::NotFound)
    ));
}
#[tokio::test]
async fn delete_keeps_lookup_permission_archive_order_and_concurrent_missing_success() {
    let state = Mutex::new(State { vanished: true, ..State::default() });
    assert!(app(false).delete(CommentId(2), || factory(&state), |_| allowed(&state, true)).await.unwrap().is_none());
    assert_eq!(state.lock().unwrap().calls, ["construct", "find", "permission", "delete"]);
    let state = Mutex::new(State::default());
    assert!(matches!(
        app(true).delete(CommentId(2), || factory(&state), |_| allowed(&state, false)).await,
        Err(HumanCommentError::Forbidden)
    ));
    assert_eq!(state.lock().unwrap().calls, ["construct", "find", "permission"]);
}
