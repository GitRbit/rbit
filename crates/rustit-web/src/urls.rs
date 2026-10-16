//! URL construction in one place, so templates never concatenate paths by hand and
//! every user-controlled segment is percent-encoded exactly once.

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

/// Characters escaped inside a single path segment (RFC 3986 `pchar` minus a few
/// characters that are legal but confusing in file names).
const SEGMENT: &AsciiSet =
    &CONTROLS.add(b' ').add(b'"').add(b'#').add(b'%').add(b'<').add(b'>').add(b'?').add(b'`').add(b'{').add(b'}').add(b'/');

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

    pub fn blame(&self, revision: &str, path: &str) -> String {
        join(&self.base, "blame", revision, path)
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

    /// The comparison of `head` against `base`.
    pub fn compare(&self, base: &str, head: &str) -> String {
        format!("{}/compare/{}...{}", self.base, encode_path(base), encode_path(head))
    }

    /// A source archive of `revision`; `extension` is `.zip` or `.tar.gz`.
    pub fn archive(&self, revision: &str, extension: &str) -> String {
        format!("{}/archive/{}{extension}", self.base, encode_path(revision))
    }
}

/// What a link inside a rendered document points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// `<a href>`: a file (`/blob/…`) or directory (`/tree/…`) page.
    Page,
    /// `<img src>`: the file's bytes (`/raw/…`).
    Image,
}

/// The outcome of resolving one link target of a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedLink {
    /// Absolute URLs, `#anchors`, `mailto:` and the like are left as written.
    Unchanged,
    /// A relative target, rewritten to a repository URL.
    Rewritten(String),
    /// A relative target that escapes the repository root; the link is dropped.
    Dropped,
}

/// Where a rendered document (a README, a Markdown file, release notes) lives, so its
/// relative links resolve against the document's directory at one revision.
#[derive(Debug, Clone)]
pub struct DocumentBase {
    urls: RepoUrls,
    revision: String,
    directory: String,
}

impl DocumentBase {
    /// A document in `directory` (empty for the root) of the repository at `revision`.
    pub fn new(urls: RepoUrls, revision: impl Into<String>, directory: impl Into<String>) -> Self {
        Self { urls, revision: revision.into(), directory: directory.into() }
    }

    /// Resolves `target` as written in the document.
    ///
    /// Relative paths resolve against the document's directory and root-relative ones
    /// (`/docs/x.md`) against the repository root, like GitHub; `..` is normalised and a
    /// path that climbs above the root is [`ResolvedLink::Dropped`]. A trailing `/` (or a
    /// path that normalises to the root) names a directory. Queries are discarded and
    /// fragments kept on page links.
    pub fn resolve(&self, target: &str, kind: LinkKind) -> ResolvedLink {
        let target = target.trim();
        if target.is_empty() || target.starts_with(['#', '?']) || target.starts_with("//") || has_scheme(target) {
            return ResolvedLink::Unchanged;
        }
        let (path, fragment) = target.split_once('#').map_or((target, None), |(path, fragment)| (path, Some(fragment)));
        let path = path.split_once('?').map_or(path, |(path, _)| path);
        let decoded = percent_encoding::percent_decode_str(path).decode_utf8_lossy().replace('\\', "/");

        let mut segments: Vec<&str> =
            if decoded.starts_with('/') { Vec::new() } else { self.directory.split('/').filter(|s| !s.is_empty()).collect() };
        for segment in decoded.split('/') {
            match segment {
                "" | "." => {}
                ".." => {
                    if segments.pop().is_none() {
                        return ResolvedLink::Dropped;
                    }
                }
                name => segments.push(name),
            }
        }
        let resolved = segments.join("/");
        let is_directory = resolved.is_empty() || decoded.ends_with('/') || decoded.ends_with("/.") || decoded.ends_with("/..");

        let url = match kind {
            LinkKind::Image if is_directory => return ResolvedLink::Dropped,
            LinkKind::Image => return ResolvedLink::Rewritten(self.urls.raw(&self.revision, &resolved)),
            LinkKind::Page if is_directory => self.urls.tree(&self.revision, &resolved),
            LinkKind::Page => self.urls.blob(&self.revision, &resolved),
        };
        ResolvedLink::Rewritten(match fragment {
            Some(fragment) if !fragment.is_empty() => format!("{url}#{fragment}"),
            _ => url,
        })
    }
}

/// Whether `target` starts with a URL scheme (`https:`, `mailto:`, `data:` …).
fn has_scheme(target: &str) -> bool {
    let Some((scheme, _)) = target.split_once(':') else { return false };
    let mut chars = scheme.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
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
    fn document_links_resolve_against_the_document_directory() {
        let base = DocumentBase::new(RepoUrls::new("alice", "proj"), "main", "docs");
        let cases: &[(&str, LinkKind, ResolvedLink)] = &[
            ("guide.md", LinkKind::Page, ResolvedLink::Rewritten("/alice/proj/blob/main/docs/guide.md".into())),
            ("./guide.md#setup", LinkKind::Page, ResolvedLink::Rewritten("/alice/proj/blob/main/docs/guide.md#setup".into())),
            ("../src/main.rs", LinkKind::Page, ResolvedLink::Rewritten("/alice/proj/blob/main/src/main.rs".into())),
            ("../src/", LinkKind::Page, ResolvedLink::Rewritten("/alice/proj/tree/main/src".into())),
            ("..", LinkKind::Page, ResolvedLink::Rewritten("/alice/proj/tree/main".into())),
            ("/LICENSE", LinkKind::Page, ResolvedLink::Rewritten("/alice/proj/blob/main/LICENSE".into())),
            ("my%20file.md?plain=1", LinkKind::Page, ResolvedLink::Rewritten("/alice/proj/blob/main/docs/my%20file.md".into())),
            ("img/logo.png", LinkKind::Image, ResolvedLink::Rewritten("/alice/proj/raw/main/docs/img/logo.png".into())),
            ("../../etc/passwd", LinkKind::Page, ResolvedLink::Dropped),
            ("/../secret", LinkKind::Image, ResolvedLink::Dropped),
            ("../", LinkKind::Image, ResolvedLink::Dropped),
            ("https://example.com/x", LinkKind::Page, ResolvedLink::Unchanged),
            ("//cdn.example/x.png", LinkKind::Image, ResolvedLink::Unchanged),
            ("mailto:ada@example.com", LinkKind::Page, ResolvedLink::Unchanged),
            ("#install", LinkKind::Page, ResolvedLink::Unchanged),
            ("", LinkKind::Page, ResolvedLink::Unchanged),
        ];
        for (target, kind, expected) in cases {
            assert_eq!(&base.resolve(target, *kind), expected, "{target:?}");
        }
    }

    #[test]
    fn document_links_keep_revisions_with_slashes() {
        let base = DocumentBase::new(RepoUrls::new("alice", "proj"), "feature/x", "");
        assert_eq!(base.resolve("a b.md", LinkKind::Page), ResolvedLink::Rewritten("/alice/proj/blob/feature/x/a%20b.md".into()));
    }

    #[test]
    fn rejects_open_redirects() {
        assert_eq!(safe_redirect_target(Some("//evil.example")), "/");
        assert_eq!(safe_redirect_target(Some("https://evil.example")), "/");
        assert_eq!(safe_redirect_target(Some("/\\evil.example")), "/");
        assert_eq!(safe_redirect_target(Some("/alice/repo")), "/alice/repo");
    }
}
