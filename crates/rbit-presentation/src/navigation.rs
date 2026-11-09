//! The partial page that boosted navigation between pages of one repository receives.
//!
//! Between two pages of the same repository the browser script replaces only `<main>` and
//! the repository tabs (`#repo-subnav`); the top bar, menus and footer stay mounted. Such a
//! request carries the repository scope of the page it was sent from (`/owner/repo`, the
//! tabs' `data-repo-scope`) in the [`PARTIAL_HEADER_NAME`] header. When the rendered page
//! belongs to that same repository, [`slice`] cuts it down to the parts the client swaps:
//!
//! ```html
//! <title>…</title>
//! <main id="main" …>…</main>
//! <div id="repo-subnav" hx-swap-oob="true" data-repo-scope="/owner/repo" …>…</div>
//! ```
//!
//! The page is cut at markers the layout emits (around `<title>` and `<main>` in the base
//! layout, around the tabs in the repository sub-navigation), so the partial response can
//! only ever contain a subset of the full page (plus the `hx-swap-oob` attribute). The
//! title and tabs are searched from the start of the page and `<main>`'s end from its end,
//! so content inside `<main>` can never move a cut. Responses vary on the header
//! ([`vary_with_partial`]), so the two shapes never share a cache entry or validator.

use std::ops::Range;

/// The request header (lower case) that carries the repository scope; echoed as `true`
/// on partial responses.
pub const PARTIAL_HEADER_NAME: &str = "rbit-partial";

/// Spelling of [`PARTIAL_HEADER_NAME`] in `Vary`.
pub const VARY_NAME: &str = "Rbit-Partial";

/// Opens the document title in the base layout.
pub const TITLE_START: &str = "<!--partial:title-->";
/// Closes the document title.
pub const TITLE_END: &str = "<!--/partial:title-->";
/// Opens the repository tabs in the repository sub-navigation.
pub const SUBNAV_START: &str = "<!--partial:subnav-->";
/// Closes the repository tabs.
pub const SUBNAV_END: &str = "<!--/partial:subnav-->";
/// Opens `<main>` in the base layout.
pub const MAIN_START: &str = "<!--partial:main-->";
/// Closes `<main>`.
pub const MAIN_END: &str = "<!--/partial:main-->";

/// How the tabs start in the repository sub-navigation, up to the scope's value.
pub const SUBNAV_OPEN: &str = "<div id=\"repo-subnav\" data-repo-scope=\"";
/// Where the out-of-band attribute goes: right after the tabs' id.
const SUBNAV_ID: &str = "<div id=\"repo-subnav\"";

/// The partial form of `page` for a request from the repository `scope`: its title, main
/// content and out-of-band repository tabs; `None` (send the full page) when a marker is
/// missing or out of order, or the page belongs to another repository or none.
pub fn slice(page: &[u8], scope: &[u8]) -> Option<Vec<u8>> {
    let title = between(page, 0, TITLE_START, TITLE_END)?;
    let subnav = between(page, title.end, SUBNAV_START, SUBNAV_END)?;
    let main_start = find(page, subnav.end, MAIN_START)? + MAIN_START.len();
    let main_end = rfind(page, MAIN_END).filter(|&end| end >= main_start)?;

    let tabs = page[subnav.clone()].trim_ascii();
    let rest = tabs.strip_prefix(SUBNAV_OPEN.as_bytes())?;
    let same_repository = !scope.is_empty() && rest.strip_prefix(scope).is_some_and(|after| after.starts_with(b"\""));
    if !same_repository {
        return None;
    }

    let mut out = Vec::with_capacity(title.len() + (main_end - main_start) + tabs.len() + 32);
    out.extend_from_slice(page[title].trim_ascii());
    out.push(b'\n');
    out.extend_from_slice(page[main_start..main_end].trim_ascii());
    out.push(b'\n');
    out.extend_from_slice(SUBNAV_ID.as_bytes());
    out.extend_from_slice(b" hx-swap-oob=\"true\"");
    out.extend_from_slice(&tabs[SUBNAV_ID.len()..]);
    out.push(b'\n');
    Some(out)
}

/// The `Vary` value of an HTML read that may be cut: `existing` plus [`VARY_NAME`].
pub fn vary_with_partial(existing: Option<&str>) -> String {
    match existing {
        Some(existing) if !existing.is_empty() => format!("{existing}, {VARY_NAME}"),
        _ => VARY_NAME.to_owned(),
    }
}

/// The bytes between the first `start` marker at or after `from` and the next `end`.
fn between(page: &[u8], from: usize, start: &str, end: &str) -> Option<Range<usize>> {
    let open = find(page, from, start)? + start.len();
    let close = find(page, open, end)?;
    Some(open..close)
}

fn find(haystack: &[u8], from: usize, needle: &str) -> Option<usize> {
    haystack.get(from..)?.windows(needle.len()).position(|w| w == needle.as_bytes()).map(|i| from + i)
}

fn rfind(haystack: &[u8], needle: &str) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle.as_bytes())
}

#[cfg(test)]
mod tests;
