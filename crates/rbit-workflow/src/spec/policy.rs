//! Spec policy contracts and original validation.
use super::runtime::ActionSource;
use serde::{Deserialize, Serialize};

// Actions: settings API.
/// Which actions a job may use besides rbit's built-ins and the workspace's own, from its
/// repository's and owner's `allowed_actions`: an action runs only when every rule allows
/// it, on top of the runner's own `[actions] allow`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionsPolicy {
    pub rules: Vec<ActionsRule>,
}

/// One owner's or repository's rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ActionsRule {
    /// No action from outside the repository.
    LocalOnly,
    /// GitHub's own actions (`actions/*`, `github/*`) when `github_owned`, and those a
    /// pattern matches: `owner/repo[/path]@ref` or, without `@`, any ref; `*` matches any
    /// run of characters; `docker://image` patterns match Docker images. Case is ignored.
    Selected { github_owned: bool, patterns: Vec<String> },
}

impl ActionsPolicy {
    pub fn is_open(&self) -> bool {
        self.rules.is_empty()
    }

    /// The first rule that refuses `source`, or `None` when it may run. The workspace's
    /// own actions always may.
    pub fn refuses(&self, source: &ActionSource) -> Option<&ActionsRule> {
        self.rules.iter().find(|rule| !rule.allows(source))
    }
}

impl ActionsRule {
    fn allows(&self, source: &ActionSource) -> bool {
        match (self, source) {
            (_, ActionSource::Local { .. }) => true,
            (Self::LocalOnly, _) => false,
            (Self::Selected { github_owned, patterns }, ActionSource::Repository { owner, repo, path, reference }) => {
                let owner = owner.to_ascii_lowercase();
                if *github_owned && (owner == "actions" || owner == "github") {
                    return true;
                }
                let name = if path.is_empty() { format!("{owner}/{repo}") } else { format!("{owner}/{repo}/{path}") }.to_ascii_lowercase();
                let full = format!("{name}@{}", reference.to_ascii_lowercase());
                patterns.iter().map(|pattern| pattern.to_ascii_lowercase()).any(|pattern| {
                    !pattern.starts_with("docker://")
                        && if pattern.contains('@') { wildcard(&pattern, &full) } else { wildcard(&pattern, &name) }
                })
            }
            (Self::Selected { patterns, .. }, ActionSource::Docker { image }) => {
                let image = format!("docker://{}", image.to_ascii_lowercase());
                patterns.iter().any(|pattern| wildcard(&pattern.to_ascii_lowercase(), &image))
            }
        }
    }
}

/// Whether `text` matches `pattern`, where `*` stands for any run of characters.
fn wildcard(pattern: &str, text: &str) -> bool {
    let (pattern, text) = (pattern.as_bytes(), text.as_bytes());
    // The classic greedy match with one backtrack point: linear for these short strings.
    let (mut p, mut t, mut star, mut mark) = (0, 0, None, 0);
    while t < text.len() {
        if p < pattern.len() && pattern[p] != b'*' && pattern[p] == text[t] {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            p += 1;
            mark = t;
        } else if let Some(at) = star {
            p = at + 1;
            mark += 1;
            t = mark;
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&byte| byte == b'*')
}
