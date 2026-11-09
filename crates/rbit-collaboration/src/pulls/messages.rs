//! Default merge messages, the noreply identities rbit writes into the commits it makes,
//! and head specifications of compare URLs.

use crate::threads::IssueNumber;

/// The default title of a merge commit: `Merge pull request #n from owner/branch`.
pub fn default_merge_title(number: IssueNumber, head_label: &str) -> String {
    format!("Merge pull request #{} from {head_label}", number.get())
}

/// The default title of a squash commit: `title (#n)`.
pub fn default_squash_title(title: &str, number: IssueNumber) -> String {
    format!("{title} (#{})", number.get())
}

/// The default message body of a squash commit: the squashed commits' summaries, oldest first.
pub fn default_squash_body<'a>(summaries: impl IntoIterator<Item = &'a str>) -> String {
    summaries.into_iter().map(|summary| format!("* {summary}")).collect::<Vec<_>>().join("\n")
}

/// A full commit message: the title, then (if any) a blank line and the body.
pub fn commit_message(title: &str, body: &str) -> String {
    let (title, body) = (title.trim(), body.trim());
    if body.is_empty() { format!("{title}\n") } else { format!("{title}\n\n{body}\n") }
}

/// The domain of the addresses in commits rbit makes: the configured one, else
/// `noreply.<host of public_url>`.
pub fn noreply_domain(configured: &str, public_url: &str) -> String {
    let configured = configured.trim();
    if !configured.is_empty() {
        return configured.to_owned();
    }
    let rest = public_url.split_once("://").map_or(public_url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit_once('@').map_or(authority, |(_, host)| host);
    let host = if host.starts_with('[') {
        host.split_once(']').map_or(host, |(inner, _)| inner.trim_start_matches('['))
    } else {
        host.split(':').next().unwrap_or(host)
    };
    let host = if host.is_empty() { "localhost" } else { host };
    format!("noreply.{host}")
}

/// `username@domain`: rbit never writes a real email address into a commit.
pub fn noreply_email(username: &str, domain: &str) -> String {
    format!("{username}@{domain}")
}

/// Who appears in a commit rbit makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitIdentity {
    pub name: String,
    pub email: String,
}

impl CommitIdentity {
    /// The display name (or the username) with the noreply address.
    pub fn noreply(username: &str, display_name: Option<&str>, domain: &str) -> Self {
        let name = display_name.map(str::trim).filter(|name| !name.is_empty()).unwrap_or(username);
        // Git refuses `<`, `>` and newlines in identities.
        let name: String = name.chars().filter(|c| !matches!(c, '<' | '>' | '\n' | '\r' | '\0')).collect();
        Self { name: if name.trim().is_empty() { username.to_owned() } else { name }, email: noreply_email(username, domain) }
    }
}

// ------------------------------------------------------------------ compare specs

/// The head side of a compare URL or a new pull request: `branch`, `owner:branch` or
/// `owner/name:branch`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadSpec {
    /// `None`: the base repository itself.
    pub owner: Option<String>,
    /// `None`: the owner's repository in the fork network.
    pub repo: Option<String>,
    pub branch: String,
}

impl HeadSpec {
    /// Parses a head spec. Branch names cannot contain `:` (Git forbids it), so the first
    /// `:` separates the repository part.
    pub fn parse(spec: &str) -> Option<Self> {
        let spec = spec.trim();
        let (repository, branch) = match spec.split_once(':') {
            Some((repository, branch)) => (Some(repository), branch),
            None => (None, spec),
        };
        if branch.is_empty() {
            return None;
        }
        let (owner, repo) = match repository {
            None => (None, None),
            Some(repository) => match repository.split_once('/') {
                Some((owner, repo)) if !owner.is_empty() && !repo.is_empty() && !repo.contains('/') => {
                    (Some(owner.to_owned()), Some(repo.to_owned()))
                }
                Some(_) => return None,
                None if repository.is_empty() => return None,
                None => (Some(repository.to_owned()), None),
            },
        };
        Some(Self { owner, repo, branch: branch.to_owned() })
    }
}
