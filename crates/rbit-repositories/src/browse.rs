//! Browsing decisions that do not need Git: revision/path splitting against known refs,
//! the landing revision, tree filtering, path arithmetic, history paging bounds and
//! which blobs are shown as images.
//!
//! Rules:
//! * A `<revision>/<path>` URL splits at the longest branch or tag name that is a whole
//!   leading path segment; anything else (commit ids) is left to Git's longest-prefix
//!   search.
//! * The landing revision is the configured default branch when it exists, otherwise
//!   the first branch; a repository without branches has none.
//! * History pages hold [`COMMITS_PER_PAGE`] commits; page 0 and 1 both start at the
//!   newest commit and an offset that would overflow is refused, never wrapped.
//! * Raw downloads are bounded by [`MAX_RAW_BYTES`].
//! * Tree filters match entry names case-insensitively; an empty filter keeps all.

/// Commits per history page.
pub const COMMITS_PER_PAGE: usize = 30;
/// Largest blob served raw.
pub const MAX_RAW_BYTES: usize = 100 * 1024 * 1024;

/// The longest name in `refs` that `combined` starts with as a whole path segment, and
/// the path after it.
pub fn split_known_ref<'a>(refs: impl IntoIterator<Item = &'a str>, combined: &str) -> Option<(String, String)> {
    let combined = combined.trim_matches('/');
    refs.into_iter()
        .filter(|name| combined.strip_prefix(name).is_some_and(|rest| rest.is_empty() || rest.starts_with('/')))
        .max_by_key(|name| name.len())
        .map(|name| (name.to_owned(), combined[name.len()..].trim_start_matches('/').to_owned()))
}

/// The configured default branch if it exists, otherwise the first branch.
pub fn landing_revision<'a, I>(branches: I, default_branch: &str) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
    I::IntoIter: Clone,
{
    let mut branches = branches.into_iter();
    let first = branches.clone().next();
    branches.find(|name| *name == default_branch).or(first)
}

/// How many commits page `page` (1-based; 0 counts as 1) skips, or `None` on overflow.
pub fn commit_offset(page: usize) -> Option<usize> {
    page.saturating_sub(1).checked_mul(COMMITS_PER_PAGE)
}

/// Entries whose name contains `query`, ignoring case, with their positions.
pub fn matching_entries<'a, T>(entries: &'a [T], name: impl Fn(&T) -> &str, query: &str) -> Vec<(usize, &'a T)> {
    let needle = query.to_lowercase();
    entries.iter().enumerate().filter(|(_, entry)| needle.is_empty() || name(entry).to_lowercase().contains(&needle)).collect()
}

pub fn join_path(directory: &str, name: &str) -> String {
    if directory.is_empty() { name.to_owned() } else { format!("{directory}/{name}") }
}

pub fn parent_path(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

pub fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// The image media type for `path`'s extension, for the types browsers display in `<img>`.
pub fn image_content_type(path: &str) -> Option<&'static str> {
    let extension = path.rsplit_once('.')?.1.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}
