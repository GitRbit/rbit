//! Who imported text is attributed to: a local account when the instance maps authors
//! (by verified email address, optionally by user name), otherwise the importer, with a
//! line naming and linking the original author.
use std::{collections::HashMap, future::Future};

/// How source authors become local accounts (the configured `author_mapping`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorRule {
    None,
    Email,
    EmailOrUsername,
}

/// Where authors are looked up, bound by composition.
pub trait AuthorDirectory: Sync {
    type Account: Copy + Send + Sync;
    /// The public email address of `login` on the source, if it shows one.
    fn source_email(&self, login: &str) -> impl Future<Output = Option<String>> + Send;
    /// The active local account whose verified address is `email`.
    fn verified_email(&self, email: &str) -> impl Future<Output = Option<Self::Account>> + Send;
    /// The local account called `login`.
    fn named(&self, login: &str) -> impl Future<Output = Option<Self::Account>> + Send;
}

/// One import's author lookups, each source login resolved once.
#[derive(Debug, Clone)]
pub struct AuthorMap<A> {
    rule: AuthorRule,
    known: HashMap<String, Option<A>>,
}

impl<A: Copy> AuthorMap<A> {
    pub fn new(rule: AuthorRule) -> Self {
        Self { rule, known: HashMap::new() }
    }

    /// The local account `login` (with the email the listing showed, if any) maps to.
    /// The source is asked for the email only when the listing had none.
    pub async fn resolve<D: AuthorDirectory<Account = A>>(&mut self, directory: &D, login: &str, email: Option<&str>) -> Option<A> {
        if self.rule == AuthorRule::None {
            return None;
        }
        let key = login.to_lowercase();
        if let Some(known) = self.known.get(&key) {
            return *known;
        }
        let email = match email.filter(|email| !email.is_empty()) {
            Some(email) => Some(email.to_owned()),
            None => directory.source_email(login).await.filter(|email| !email.is_empty()),
        };
        let mut found = match email {
            Some(email) => directory.verified_email(&email).await,
            None => None,
        };
        if found.is_none() && self.rule == AuthorRule::EmailOrUsername {
            found = directory.named(login).await;
        }
        self.known.insert(key, found);
        found
    }
}

/// A source author as listings show them.
#[derive(Debug, Clone, Copy)]
pub struct SourceAuthor<'a> {
    pub login: &'a str,
    pub html_url: Option<&'a str>,
}

/// `text` as written by `author`: unchanged when they map to a local account, otherwise
/// with a line naming and linking the original author (inside link text, so the
/// `@login` is never taken for a local mention). Deleted authors are `ghost`.
pub fn attributed(text: &str, author: Option<SourceAuthor<'_>>, mapped: bool, web_base: &str, platform: &str) -> String {
    if mapped {
        return text.to_owned();
    }
    let (login, url) = match author {
        Some(author) => {
            let url = author
                .html_url
                .filter(|url| url.starts_with("https://") || url.starts_with("http://"))
                .map_or_else(|| format!("{web_base}/{}", author.login), str::to_owned);
            (author.login, url)
        }
        None => ("ghost", format!("{web_base}/ghost")),
    };
    let login: String = login.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')).collect();
    let url: String = url.chars().filter(|c| !c.is_whitespace() && !matches!(c, '(' | ')' | '<' | '>')).collect();
    format!("*Originally by [@{login}]({url}) on {platform}.*\n\n{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unmapped_text_names_and_links_its_author_safely() {
        let author = SourceAuthor { login: "octo<cat>", html_url: Some("https://github.com/octo cat)") };
        assert_eq!(
            attributed("Hi", Some(author), false, "https://github.com", "GitHub"),
            "*Originally by [@octocat](https://github.com/octocat) on GitHub.*\n\nHi"
        );
        let relative = SourceAuthor { login: "bob", html_url: Some("javascript:alert(1)") };
        assert!(attributed("x", Some(relative), false, "https://g.example", "Gitea").contains("(https://g.example/bob)"));
        assert!(attributed("x", None, false, "https://g.example", "Gitea").contains("[@ghost](https://g.example/ghost)"));
        assert_eq!(attributed("Hi", Some(author), true, "", ""), "Hi");
    }
}
