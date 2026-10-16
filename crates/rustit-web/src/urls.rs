//! URL construction in one place, so templates never concatenate paths by hand and
//! every user-controlled segment is percent-encoded exactly once.

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

/// Characters escaped inside a single path segment (RFC 3986 `pchar` minus a few
/// characters that are legal but confusing in file names).
const SEGMENT: &AsciiSet = &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'%').add(b'<').add(b'>').add(b'?').add(b'`').add(b'{').add(b'}').add(b'/');

/// Encodes a slash-separated path, keeping the slashes.
pub fn encode_path(path: &str) -> String {
    path.split('/').map(|segment| utf8_percent_encode(segment, SEGMENT).to_string()).collect::<Vec<_>>().join("/")
}

/// URLs scoped to one repository.
#[derive(Debug, Clone)]
pub struct RepoUrls {
    base: String,
}

impl RepoUrls {
    pub fn new(owner: &str, repo: &str) -> Self {
        Self { base: format!("/{owner}/{repo}") }
    }

    pub fn home(&self) -> String {
        self.base.clone()
    }

    pub fn tree(&self, revision: &str, path: &str) -> String {
        join(&self.base, "tree", revision, path)
    }

    pub fn blob(&self, revision: &str, path: &str) -> String {
        join(&self.base, "blob", revision, path)
    }

    pub fn raw(&self, revision: &str, path: &str) -> String {
        join(&self.base, "raw", revision, path)
    }

    pub fn commits(&self, revision: &str) -> String {
        join(&self.base, "commits", revision, "")
    }

    pub fn commit(&self, id: &str) -> String {
        join(&self.base, "commit", id, "")
    }

    pub fn branches(&self) -> String {
        format!("{}/branches", self.base)
    }

    pub fn tags(&self) -> String {
        format!("{}/tags", self.base)
    }
}

fn join(base: &str, kind: &str, revision: &str, path: &str) -> String {
    let mut url = format!("{base}/{kind}/{}", encode_path(revision));
    if !path.is_empty() {
        url.push('/');
        url.push_str(&encode_path(path));
    }
    url
}

/// Accepts a post-login redirect target only if it is a local absolute path, preventing
/// open redirects such as `//evil.example` or `https://evil.example`.
pub fn safe_redirect_target(next: Option<&str>) -> &str {
    match next {
        Some(target) if target.starts_with('/') && !target.starts_with("//") && !target.contains('\\') => target,
        _ => "/",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_special_characters_but_keeps_slashes() {
        assert_eq!(encode_path("docs/my file#1.md"), "docs/my%20file%231.md");
    }

    #[test]
    fn rejects_open_redirects() {
        assert_eq!(safe_redirect_target(Some("//evil.example")), "/");
        assert_eq!(safe_redirect_target(Some("https://evil.example")), "/");
        assert_eq!(safe_redirect_target(Some("/\\evil.example")), "/");
        assert_eq!(safe_redirect_target(Some("/alice/repo")), "/alice/repo");
    }
}
