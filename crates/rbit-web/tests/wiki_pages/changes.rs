//! Edit, preview, conflict, history, compare, restore and delete forms.
use super::{Fixture, W};
use axum::http::StatusCode;
use rbit_core::Visibility;

/// The `base` hidden field of a rendered form.
fn base(body: &str) -> String {
    let start = body.find("name=\"base\" value=\"").expect("base field") + "name=\"base\" value=\"".len();
    body[start..].split('"').next().unwrap().to_owned()
}

#[tokio::test]
async fn edits_preview_without_saving_and_stale_forms_conflict_with_the_text_kept() {
    let fixture = Fixture::new(Visibility::Public).await;
    fixture.page("Home", "first").await;
    let edit_url = format!("{W}/-/page/1/edit");
    let form = fixture.get(&edit_url, Some(&fixture.writer)).await;
    assert_eq!(form.status, StatusCode::OK);
    let stale = base(&form.body);

    let preview = fixture
        .post(
            &edit_url,
            Some(&fixture.writer),
            &[("title", "Home"), ("path", "Home"), ("content", "**draft**"), ("base", &stale), ("intent", "preview")],
        )
        .await;
    assert_eq!(preview.status, StatusCode::OK);
    assert!(preview.body.contains("<strong>draft</strong>"));
    assert!(fixture.get(&format!("{W}/Home"), None).await.body.contains("first"), "preview saves nothing");

    let saved = fixture
        .post(&edit_url, Some(&fixture.owner), &[("title", "Home"), ("path", "Home"), ("content", "second"), ("base", &stale)])
        .await;
    assert_eq!((saved.status, saved.location.as_deref()), (StatusCode::SEE_OTHER, Some("/owner/handbook/wiki/Home")));
    let conflict = fixture
        .post(&edit_url, Some(&fixture.writer), &[("title", "Home"), ("path", "Home"), ("content", "my lost words"), ("base", &stale)])
        .await;
    assert_eq!(conflict.status, StatusCode::CONFLICT);
    assert!(conflict.body.contains("my lost words"), "the submitted text is kept");
    assert!(conflict.body.contains("This page changed after you started."));
    assert!(conflict.body.contains("/owner/handbook/wiki/-/page/1/compare?base=1&#38;head=2"), "{}", conflict.body);
    assert!(fixture.get(&format!("{W}/Home"), None).await.body.contains("second"));

    let invalid =
        fixture.post(&edit_url, Some(&fixture.writer), &[("title", " "), ("path", "Home"), ("content", "x"), ("base", "2")]).await;
    assert_eq!(invalid.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(invalid.body.contains("Enter a title."));
    let taken = fixture.post(&format!("{W}/-/new"), Some(&fixture.writer), &[("title", "Other"), ("path", "Home"), ("content", "x")]).await;
    assert_eq!(taken.status, StatusCode::CONFLICT);
    assert!(taken.body.contains("Another page already uses this path."));
}

#[tokio::test]
async fn history_compare_restore_and_delete_work_through_forms() {
    let fixture = Fixture::new(Visibility::Public).await;
    fixture.page("Notes", "line one\n").await;
    let edit_url = format!("{W}/-/page/1/edit");
    let saved = fixture
        .post(&edit_url, Some(&fixture.writer), &[("title", "Notes"), ("path", "Notes"), ("content", "line two\n"), ("base", "1")])
        .await;
    assert_eq!(saved.status, StatusCode::SEE_OTHER);

    let history = fixture.get(&format!("{W}/-/page/1/history"), None).await;
    assert_eq!(history.status, StatusCode::OK);
    for expected in ["Revision 2", "Revision 1", "Edited", "Created", "href=\"/writer\"", "compare?base=1&#38;head=2"] {
        assert!(history.body.contains(expected), "missing {expected}");
    }
    let compare = fixture.get(&format!("{W}/-/page/1/compare?base=1&head=2"), None).await;
    assert!(compare.body.contains("1 addition, 1 deletion") && compare.body.contains("diff-del") && compare.body.contains("line one"));

    let old = fixture.get(&format!("{W}/-/page/1/revisions/1"), Some(&fixture.writer)).await;
    assert!(old.body.contains("You are viewing an older revision of this page.") && old.body.contains("line one"));
    let restore_url = format!("{W}/-/page/1/revisions/1/restore");
    assert_eq!(fixture.post(&restore_url, Some(&fixture.reader), &[("base", "2")]).await.status, StatusCode::FORBIDDEN);
    let restored = fixture.post(&restore_url, Some(&fixture.writer), &[("base", &base(&old.body))]).await;
    assert_eq!(restored.location.as_deref(), Some("/owner/handbook/wiki/Notes"));
    assert!(fixture.get(&format!("{W}/Notes"), None).await.body.contains("line one"));

    let confirm = fixture.get(&format!("{W}/-/page/1/delete"), Some(&fixture.writer)).await;
    assert!(confirm.body.contains("Delete \u{2068}Notes\u{2069}?") || confirm.body.contains("Delete Notes?"));
    let stale = fixture.post(&format!("{W}/-/page/1/delete"), Some(&fixture.writer), &[("base", "1")]).await;
    assert_eq!(stale.location.as_deref(), Some("/owner/handbook/wiki/-/page/1/history"), "a stale delete changes nothing");
    let deleted = fixture.post(&format!("{W}/-/page/1/delete"), Some(&fixture.writer), &[("base", &base(&confirm.body))]).await;
    assert_eq!(deleted.location.as_deref(), Some(W));
    assert_eq!(fixture.get(&format!("{W}/Notes"), None).await.status, StatusCode::NOT_FOUND);
    assert_eq!(fixture.get(&format!("{W}/-/page/1/history"), None).await.status, StatusCode::NOT_FOUND);
}
