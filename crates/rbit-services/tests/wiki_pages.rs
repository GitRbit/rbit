//! Wiki pages through the production service: publication, history, conflicts, access.
mod access;
mod fixture;

use fixture::{Fixture, input};
use rbit_core::Visibility;
use rbit_services::wikis::{
    errors::WikiError,
    pages::{WikiLookup, WikiPageView},
};
use rbit_wikis::{WikiGeneration, WikiPath, change::WikiChange};

fn path(value: &str) -> WikiPath {
    value.parse().unwrap()
}

fn page(lookup: WikiLookup) -> WikiPageView {
    match lookup {
        WikiLookup::Page(view) => view,
        WikiLookup::Missing { empty } => panic!("missing page (empty wiki: {empty})"),
    }
}

#[tokio::test]
async fn pages_are_created_edited_renamed_and_keep_their_history() {
    let fixture = Fixture::new(Visibility::Private).await;
    let wikis = &fixture.services.wikis;
    let repo = fixture.repo(&fixture.writer).await;
    assert!(matches!(wikis.page(&repo, Some(&fixture.writer), &path("Home")).await.unwrap(), WikiLookup::Missing { empty: true }));

    let created = wikis.create(&repo, &fixture.writer, &input("Home", "", "# Welcome\r\n\r\nFirst <script>x</script>\r\n")).await.unwrap();
    assert_eq!(created.as_str(), "Home");
    let home = page(wikis.page(&repo, Some(&fixture.reader), &created).await.unwrap());
    assert_eq!(home.content.as_str(), "# Welcome\n\nFirst <script>x</script>\n", "CRLF from browsers is stored as LF");
    let first = home.page.head.generation;

    let derived = wikis.create(&repo, &fixture.writer, &input("Getting started", "", "Steps")).await.unwrap();
    assert_eq!(derived.as_str(), "Getting-started");

    let owner_repo = fixture.repo(&fixture.owner).await;
    let edited = wikis.edit(&owner_repo, &fixture.owner, home.page.id, first, &input("Home", "Home", "# Welcome\n\nSecond\n")).await;
    assert!(edited.is_ok(), "{edited:?}");
    let base = page(wikis.page(&repo, Some(&fixture.writer), &created).await.unwrap()).page.head.generation;
    let renamed =
        wikis.edit(&repo, &fixture.writer, home.page.id, base, &input("Start here", "Guide/Start", "# Welcome\n\nSecond\n")).await.unwrap();
    assert_eq!(renamed.as_str(), "Guide/Start");
    assert!(matches!(wikis.page(&repo, Some(&fixture.writer), &path("Home")).await.unwrap(), WikiLookup::Missing { empty: false }));
    let moved = page(wikis.page(&repo, Some(&fixture.writer), &renamed).await.unwrap());
    assert_eq!((moved.page.id, moved.page.title.as_str()), (home.page.id, "Start here"), "the stable ID survives a rename");

    let history = wikis.history(&repo, Some(&fixture.reader), home.page.id, None).await.unwrap();
    let kinds: Vec<_> = history.entries.iter().map(|entry| entry.change).collect();
    assert_eq!(kinds, [Some(WikiChange::Rename), Some(WikiChange::Edit), Some(WikiChange::Create)]);
    let authors: Vec<_> = history.entries.iter().map(|entry| entry.author.as_deref()).collect();
    assert_eq!(authors, [Some("writer"), Some("owner"), Some("writer")]);

    let comparison = wikis.compare(&repo, Some(&fixture.reader), home.page.id, first, moved.page.head.generation).await.unwrap();
    assert_eq!((comparison.changes.additions, comparison.changes.deletions), (1, 1));
    let old = wikis.revision(&repo, Some(&fixture.reader), home.page.id, first).await.unwrap();
    assert_eq!((old.page.path.as_str(), old.page.title.as_str()), ("Home", "Home"));
    assert!(old.content.as_str().contains("First"));
    assert_eq!(
        wikis.previous(&repo, Some(&fixture.reader), home.page.id, moved.page.head.generation).await.unwrap().map(WikiGeneration::get),
        Some(3)
    );
}

#[tokio::test]
async fn a_stale_form_conflicts_only_when_its_own_page_changed() {
    let fixture = Fixture::new(Visibility::Public).await;
    let wikis = &fixture.services.wikis;
    let repo = fixture.repo(&fixture.writer).await;
    let one = wikis.create(&repo, &fixture.writer, &input("One", "", "one")).await.unwrap();
    let two = wikis.create(&repo, &fixture.writer, &input("Two", "", "two")).await.unwrap();
    let one = page(wikis.page(&repo, Some(&fixture.writer), &one).await.unwrap());
    let two = page(wikis.page(&repo, Some(&fixture.writer), &two).await.unwrap());
    let base = one.page.head.generation;

    // Another person edits page two after page one's form was read: no conflict.
    let owner_repo = fixture.repo(&fixture.owner).await;
    wikis.edit(&owner_repo, &fixture.owner, two.page.id, base, &input("Two", "Two", "two, edited")).await.unwrap();
    wikis.edit(&repo, &fixture.writer, one.page.id, base, &input("One", "One", "one, first save")).await.unwrap();
    let both = page(wikis.page(&repo, None, &path("Two")).await.unwrap());
    assert_eq!(both.content.as_str(), "two, edited", "an edit of another page is kept, never overwritten");

    // The same stale form saved again: page one changed after `base`.
    let stale = wikis.edit(&repo, &fixture.writer, one.page.id, base, &input("One", "One", "one, lost update")).await;
    assert!(matches!(stale, Err(WikiError::Conflict)), "{stale:?}");
    let current = page(wikis.page(&repo, None, &path("One")).await.unwrap());
    assert_eq!(current.content.as_str(), "one, first save");

    let unchanged =
        wikis.edit(&repo, &fixture.writer, one.page.id, current.page.head.generation, &input("One", "One", "one, first save")).await;
    assert!(matches!(unchanged, Err(WikiError::Unchanged)));
    let clash = wikis.create(&repo, &fixture.writer, &input("Other", "Two", "x")).await;
    assert!(matches!(clash, Err(WikiError::PathTaken)));
    let rename_clash = wikis.edit(&repo, &fixture.writer, one.page.id, current.page.head.generation, &input("One", "Two", "x")).await;
    assert!(matches!(rename_clash, Err(WikiError::PathTaken)));
    for (bad, expected) in [("", "title"), ("Ok", "path")] {
        let refused = wikis.create(&repo, &fixture.writer, &input(bad, if expected == "path" { "a/../b" } else { "" }, "x")).await;
        assert!(matches!(refused, Err(WikiError::InvalidTitle(_) | WikiError::InvalidPath(_))), "{expected}: {refused:?}");
    }
}

#[tokio::test]
async fn restore_and_delete_publish_new_generations_without_rewriting_history() {
    let fixture = Fixture::new(Visibility::Public).await;
    let wikis = &fixture.services.wikis;
    let repo = fixture.repo(&fixture.writer).await;
    let created = wikis.create(&repo, &fixture.writer, &input("Notes", "", "original")).await.unwrap();
    let notes = page(wikis.page(&repo, None, &created).await.unwrap());
    let first = notes.page.head.generation;
    wikis.edit(&repo, &fixture.writer, notes.page.id, first, &input("Notes", "Notes", "vandalized")).await.unwrap();
    let current = page(wikis.page(&repo, None, &created).await.unwrap()).page.head.generation;

    let restored = wikis.restore(&repo, &fixture.writer, notes.page.id, first, current).await.unwrap();
    assert_eq!(page(wikis.page(&repo, None, &restored).await.unwrap()).content.as_str(), "original");
    let history = wikis.history(&repo, None, notes.page.id, None).await.unwrap();
    assert_eq!(history.entries.len(), 3);
    assert_eq!(history.entries[0].change, Some(WikiChange::Restore));
    let latest = history.entries[0].head.generation;
    assert!(matches!(wikis.restore(&repo, &fixture.writer, notes.page.id, first, latest).await, Err(WikiError::Unchanged)));

    let kept = wikis.create(&repo, &fixture.writer, &input("Kept", "", "kept")).await.unwrap();
    assert!(matches!(wikis.delete(&repo, &fixture.writer, notes.page.id, first).await, Err(WikiError::Conflict)), "stale delete");
    let head = page(wikis.page(&repo, None, &kept).await.unwrap()).page.head.generation;
    wikis.delete(&repo, &fixture.writer, notes.page.id, head).await.unwrap();
    assert!(matches!(wikis.page(&repo, None, &created).await.unwrap(), WikiLookup::Missing { empty: false }));
    assert!(matches!(wikis.page_by_id(&repo, None, notes.page.id).await, Err(WikiError::NotFound)));
    assert_eq!(page(wikis.page(&repo, None, &kept).await.unwrap()).content.as_str(), "kept");
    let listed = wikis.pages(&repo, None, None).await.unwrap();
    assert_eq!(listed.entries.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(), ["Kept"]);
}

#[tokio::test]
async fn search_matches_titles_and_paths_literally() {
    let fixture = Fixture::new(Visibility::Public).await;
    let wikis = &fixture.services.wikis;
    let repo = fixture.repo(&fixture.writer).await;
    wikis.create(&repo, &fixture.writer, &input("Release process", "Process/Release", "body mentions deploy")).await.unwrap();
    wikis.create(&repo, &fixture.writer, &input("100% coverage", "Coverage", "x")).await.unwrap();
    let titles =
        |found: Vec<rbit_wikis::listing::WikiPageEntry>| found.into_iter().map(|entry| entry.title.to_string()).collect::<Vec<_>>();
    assert_eq!(titles(wikis.search(&repo, None, "release").await.unwrap()), ["Release process"]);
    assert_eq!(titles(wikis.search(&repo, None, "process/").await.unwrap()), ["Release process"]);
    assert_eq!(titles(wikis.search(&repo, None, "%").await.unwrap()), ["100% coverage"]);
    assert!(wikis.search(&repo, None, "deploy").await.unwrap().is_empty(), "content is not searched");
}
