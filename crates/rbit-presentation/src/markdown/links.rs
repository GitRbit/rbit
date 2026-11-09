//! Link and image targets inside a document being sanitised: the [`LinkResolver`] port,
//! in-document fragments and `srcset` candidates.

use std::borrow::Cow;

use super::ID_PREFIX;

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

/// Where a document's relative links lead. The sanitiser calls it only for targets that
/// survived its scheme checks, during one synchronous sanitising call.
pub trait LinkResolver {
    /// Resolves `target` as written in the document.
    fn resolve(&self, target: &str, kind: LinkKind) -> ResolvedLink;

    /// An owned copy, lent to the sanitiser's attribute filter for one document.
    fn boxed(&self) -> Box<dyn LinkResolver>;
}

/// Rewrites relative link and image targets; every other attribute passes unchanged.
pub(super) fn resolve_attribute<'u>(base: &dyn LinkResolver, element: &str, attribute: &str, value: &'u str) -> Option<Cow<'u, str>> {
    if matches!(element, "img" | "source") && attribute == "srcset" {
        return Some(Cow::Owned(resolve_srcset(Some(base), value)));
    }
    let kind = match (element, attribute) {
        ("a", "href") => LinkKind::Page,
        ("img", "src") => LinkKind::Image,
        _ => return Some(Cow::Borrowed(value)),
    };
    match base.resolve(value, kind) {
        ResolvedLink::Unchanged => Some(Cow::Borrowed(value)),
        ResolvedLink::Rewritten(url) => Some(Cow::Owned(url)),
        // Without its target the link renders as plain text (and the image as nothing).
        ResolvedLink::Dropped => None,
    }
}

/// `#x` → `#user-content-x` for in-document links, so they reach the prefixed ids.
pub(super) fn prefixed_fragment(element: &str, attribute: &str, value: &str) -> Option<String> {
    let fragment = value.strip_prefix('#').filter(|fragment| !fragment.is_empty())?;
    (element == "a" && attribute == "href" && !fragment.starts_with(ID_PREFIX)).then(|| format!("#{ID_PREFIX}{fragment}"))
}

/// URLs in srcset may contain commas (notably data URLs), so a comma alone is not
/// a separator until after the URL token. Preserve density/width descriptors.
pub(super) fn resolve_srcset(base: Option<&dyn LinkResolver>, value: &str) -> String {
    let mut rest = value;
    let mut candidates = Vec::new();
    while !rest.is_empty() {
        rest = rest.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == ',');
        if rest.is_empty() {
            break;
        }
        let end = rest.find(|c: char| c.is_ascii_whitespace()).unwrap_or(rest.len());
        let token = &rest[..end];
        rest = &rest[end..];
        let url = token.trim_end_matches(',');
        let descriptor = if token.ends_with(',') {
            ""
        } else {
            let end = rest.find(',').unwrap_or(rest.len());
            let descriptor = rest[..end].trim();
            rest = &rest[end..];
            descriptor
        };
        // Unlike src, ammonia does not validate each URL inside srcset.
        let scheme = url.split_once(':').map(|(scheme, _)| scheme.to_ascii_lowercase());
        if scheme.as_deref().is_some_and(|scheme| !matches!(scheme, "http" | "https") && !scheme.contains('/')) {
            continue;
        }
        let resolved = match base.map_or(ResolvedLink::Unchanged, |base| base.resolve(url, LinkKind::Image)) {
            ResolvedLink::Unchanged => url.to_owned(),
            ResolvedLink::Rewritten(url) => url,
            ResolvedLink::Dropped => continue,
        };
        candidates.push(if descriptor.is_empty() { resolved } else { format!("{resolved} {descriptor}") });
    }
    candidates.join(", ")
}
