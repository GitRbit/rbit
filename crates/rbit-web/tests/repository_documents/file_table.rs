//! The file table reads like a table on the repository home and in every folder: column
//! headers above the rows, and last-commit cells that fill the same columns.
use super::{Fixture, HOME, StatusCode};

const HEADERS: [&str; 3] = [
    r#"<span role="columnheader" class="min-w-0 flex-1 truncate md:w-2/5 md:flex-none">Name</span>"#,
    r#"<span role="columnheader" class="hidden min-w-0 flex-1 truncate md:block">Last commit message</span>"#,
    r#"<span role="columnheader" class="ms-auto shrink-0 text-end">Last commit date</span>"#,
];

/// Byte offset of `needle` in `html`, which must contain it.
fn at(html: &str, needle: &str) -> usize {
    html.find(needle).unwrap_or_else(|| panic!("{needle} in the page"))
}

#[tokio::test]
async fn home_and_folders_label_the_file_table_columns() {
    let fixture = Fixture::new().await;
    let folder = format!("{HOME}/tree/{}/docs", fixture.main_commit);
    for (path, parent, first_entry) in [(HOME.to_owned(), false, "README.md"), (folder, true, "CONTRIBUTING.md")] {
        let (status, html) = fixture.get(&path, false, false).await;
        assert_eq!(status, StatusCode::OK, "{path}");
        let table = at(&html, r#"<div role="table" aria-label="Files">"#);
        assert!(at(&html, "class=\"box-header") < table, "{path}: the latest commit stays above the headers");
        let mut previous = table;
        for header in HEADERS {
            let offset = at(&html, header);
            assert!(previous < offset, "{path}: {header} in column order");
            previous = offset;
        }
        assert_eq!(html.contains(r#"aria-label="Parent directory">..</a>"#), parent, "{path}");
        if parent {
            assert!(previous < at(&html, r#"aria-label="Parent directory""#), "{path}: the parent row follows the headers");
        }
        let entry = at(&html, &format!(">{first_entry}</span>"));
        assert!(previous < entry, "{path}: entries follow the headers");
        assert!(html.contains(r#"<span id="entry-commit-0" class="contents"></span>"#), "{path}: a slot for the commit cells");
    }
}

#[tokio::test]
async fn commit_cells_fill_the_message_and_date_columns() {
    let fixture = Fixture::new().await;
    let (status, cells) = fixture.get(&format!("{HOME}/tree-commits/{}/docs", fixture.main_commit), false, false).await;
    assert_eq!(status, StatusCode::OK);
    assert!(cells.contains(r#"<span role="cell" class="hidden min-w-0 flex-1 md:block"><a href="#), "{cells}");
    assert!(cells.contains(r#"<span role="cell" class="ms-auto shrink-0 text-end"><time "#), "{cells}");
    assert!(cells.contains("data-relative>"), "the date column reads as an age: {cells}");
}
