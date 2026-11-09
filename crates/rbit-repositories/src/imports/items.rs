//! What each imported item becomes: labels, milestones, threads and their closing,
//! pull request branch data, comments, releases and their files. Text limits that other
//! features own are handed in as [`Limits`].
use super::model::{label_color, truncate_bytes, truncate_chars};

/// Label colour for labels without a valid one.
pub const DEFAULT_LABEL_COLOR: &str = "#ededed";
/// Longest label name and description, and milestone title (rbit's limits).
pub const MAX_LABEL_NAME: usize = 50;
pub const MAX_LABEL_DESCRIPTION: usize = 100;
pub const MAX_MILESTONE_TITLE: usize = 100;
/// The title of a thread whose own title is not usable.
pub const UNTITLED: &str = "(untitled)";
/// The text of a comment the source has without one.
pub const EMPTY_COMMENT: &str = "*(no text)*";

/// Limits owned by threads and releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub title_chars: usize,
    pub text_bytes: usize,
    pub release_name_chars: usize,
    pub release_notes_chars: usize,
}

/// A label as it is created here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelItem {
    pub name: String,
    pub color: String,
    pub description: String,
}

/// The label to create, or the warning when its name is not valid here.
pub fn label(name: &str, color: &str, description: Option<&str>) -> Result<LabelItem, String> {
    let trimmed = truncate_chars(name.trim(), MAX_LABEL_NAME).trim().to_owned();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return Err(format!("label {name:?} was skipped: its name is not valid here"));
    }
    Ok(LabelItem {
        color: label_color(color).unwrap_or_else(|| DEFAULT_LABEL_COLOR.to_owned()),
        description: truncate_chars(description.unwrap_or_default().trim(), MAX_LABEL_DESCRIPTION).to_owned(),
        name: trimmed,
    })
}

/// A milestone's title (`None`: skipped) and description.
pub fn milestone(title: &str, description: Option<&str>, limits: Limits) -> Option<(String, String)> {
    let title = truncate_chars(title.trim(), MAX_MILESTONE_TITLE).trim().to_owned();
    (!title.is_empty()).then(|| (title, truncate_bytes(description.unwrap_or_default(), limits.text_bytes).to_owned()))
}

/// A due date as stored: the `YYYY-MM-DD` part of an ISO 8601 time.
pub fn due_date(iso8601: &str) -> String {
    iso8601.chars().take(10).collect()
}

/// How labels and milestones are matched to threads: case-insensitively, trimmed.
pub fn lookup_key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// A thread title cut to the limit, with control characters as spaces. Composition
/// falls back to [`UNTITLED`] when the result is still not a valid title.
pub fn thread_title(title: &str, limits: Limits) -> String {
    truncate_chars(title.trim(), limits.title_chars).chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// The note added to an open pull request whose commits did not come along.
pub fn closed_pull_note(platform: &str) -> String {
    format!("\n\n*This pull request was open on {platform}; its commits were not available, so it was imported closed.*")
}

/// How an imported thread ends up closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    Open,
    Merged,
    Completed,
    NotPlanned,
}

/// Merged pull requests are merged; closed threads, and pull requests that cannot stay
/// open, are closed as completed unless the source says not planned or they were open
/// pull requests closed for lack of commits.
#[expect(clippy::fn_params_excessive_bools, reason = "the five facts the source and the copy report about a thread")]
pub fn closing(merged: bool, closed: bool, unopenable_pull: bool, not_planned: bool, noted: bool) -> Closing {
    if merged {
        Closing::Merged
    } else if closed || unopenable_pull {
        if not_planned || noted { Closing::NotPlanned } else { Closing::Completed }
    } else {
        Closing::Open
    }
}

/// A full 40-digit (or 64-digit) commit id.
pub fn is_full_id(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `value`, or `fallback` when it is blank.
pub fn non_empty(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() { fallback.to_owned() } else { value.to_owned() }
}

/// The head commit kept for a pull request that is only history.
pub fn history_head(sha: &str) -> String {
    if is_full_id(sha) { sha.to_owned() } else { "0".repeat(40) }
}

/// A comment's text before attribution.
pub fn comment_text(body: Option<&str>) -> &str {
    let body = body.unwrap_or_default();
    if body.trim().is_empty() { EMPTY_COMMENT } else { body }
}

/// A release's tag (`None`: skipped), name (the tag when blank) and notes.
pub fn release(tag: &str, name: Option<&str>, notes: Option<&str>, limits: Limits) -> Option<(String, String, String)> {
    let tag = tag.trim().to_owned();
    if tag.is_empty() {
        return None;
    }
    let name: String = truncate_chars(name.unwrap_or_default().trim(), limits.release_name_chars)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let name = if name.trim().is_empty() { tag.clone() } else { name.trim().to_owned() };
    let notes = truncate_chars(notes.unwrap_or_default(), limits.release_notes_chars).to_owned();
    Some((tag, name, notes))
}

/// Why a release file is skipped: too large here, or a name rbit refuses.
pub fn asset_refusal(tag: &str, name: &str, size: u64, max: u64, name_check: Result<(), String>) -> Option<String> {
    if size > max {
        return Some(format!("release {tag}: {name} was skipped: it is larger than this instance allows"));
    }
    name_check.err().map(|message| format!("release {tag}: {name} was skipped: {message}"))
}

/// Where a release file is downloaded from and with which `Accept`: GitHub's API URL
/// (works for private repositories), Gitea's download URL. `None`: no URL at all.
pub fn asset_download<'a>(gitea: bool, api_url: Option<&'a str>, download_url: Option<&'a str>) -> Option<(&'a str, &'static str)> {
    match (api_url, download_url) {
        (Some(url), _) if !gitea => Some((url, "application/octet-stream")),
        (_, Some(url)) => Some((url, "*/*")),
        (Some(url), None) => Some((url, "application/octet-stream")),
        (None, None) => None,
    }
}

/// The LFS batch endpoint of a clone URL.
pub fn lfs_batch_endpoint(clone_url: &str) -> String {
    let base = clone_url.trim_end_matches('/');
    format!("{}.git/info/lfs/objects/batch", base.strip_suffix(".git").unwrap_or(base))
}

/// The highest number among the source's newest issues and its pull requests: numbers
/// up to it are reserved so threads created here meanwhile cannot take one.
pub fn highest_number(numbers: impl IntoIterator<Item = u32>) -> Option<u32> {
    numbers.into_iter().max()
}

/// A merge commit worth keeping: a full commit id.
pub fn merge_commit(sha: Option<&str>) -> Option<String> {
    sha.filter(|sha| is_full_id(sha)).map(str::to_owned)
}

/// The commit a merged event names: the merge commit, or the head when there is none.
pub fn merged_event_commit(merge_sha: Option<&str>, head_sha: &str) -> String {
    merge_commit(merge_sha).unwrap_or_else(|| head_sha.to_owned())
}

/// Whether a pull request's head branch belongs to the source repository itself.
pub fn same_repository(head_repository: Option<&str>, source: &str) -> bool {
    head_repository.is_some_and(|name| name.eq_ignore_ascii_case(source))
}

/// Why a release file is removed again: it arrived incomplete.
pub fn incomplete_asset(tag: &str, name: &str, stored: u64, expected: u64) -> Option<String> {
    (expected > 0 && stored != expected).then(|| format!("release {tag}: {name} arrived with {stored} of {expected} bytes; removed"))
}

/// Warnings for single items that did not come along.
pub fn unknown_branches(number: u32) -> String {
    format!("#{number}: the pull request's branches are unknown; skipped")
}
pub fn thread_exists(number: u32) -> String {
    format!("#{number} already exists here; skipped")
}
pub fn thread_failed(number: u32) -> String {
    format!("#{number} could not be imported")
}
pub fn release_exists(tag: &str) -> String {
    format!("release {tag} already exists here; skipped")
}
pub fn release_failed(tag: &str) -> String {
    format!("release {tag} could not be imported")
}

#[cfg(test)]
#[path = "items_tests.rs"]
mod tests;
