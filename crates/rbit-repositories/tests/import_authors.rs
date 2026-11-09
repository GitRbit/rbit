use rbit_repositories::imports::authors::{AuthorDirectory, AuthorMap, AuthorRule};
use std::{
    future::{Future, ready},
    sync::Mutex,
};

/// A directory where `ada` shows `ada@example.com` on the source and only that address
/// and the name `bob` exist locally.
#[derive(Default)]
struct Directory {
    trace: Mutex<Vec<String>>,
}
impl Directory {
    fn push(&self, entry: String) {
        self.trace.lock().unwrap().push(entry);
    }
    fn trace(&self) -> Vec<String> {
        self.trace.lock().unwrap().clone()
    }
}
impl AuthorDirectory for Directory {
    type Account = u32;
    fn source_email(&self, login: &str) -> impl Future<Output = Option<String>> + Send {
        self.push(format!("profile:{login}"));
        ready((login == "ada").then(|| "ada@example.com".to_owned()))
    }
    fn verified_email(&self, email: &str) -> impl Future<Output = Option<u32>> + Send {
        self.push(format!("email:{email}"));
        ready((email == "ada@example.com").then_some(1))
    }
    fn named(&self, login: &str) -> impl Future<Output = Option<u32>> + Send {
        self.push(format!("named:{login}"));
        ready((login == "bob").then_some(2))
    }
}

#[tokio::test]
async fn nobody_is_mapped_without_a_rule_and_nothing_is_asked() {
    let directory = Directory::default();
    let mut authors = AuthorMap::new(AuthorRule::None);
    assert_eq!(authors.resolve(&directory, "ada", Some("ada@example.com")).await, None);
    assert_eq!(directory.trace(), Vec::<String>::new());
}

#[tokio::test]
async fn emails_come_from_listings_or_profiles_and_each_login_is_resolved_once() {
    let directory = Directory::default();
    let mut authors = AuthorMap::new(AuthorRule::Email);
    assert_eq!(authors.resolve(&directory, "ada", None).await, Some(1));
    assert_eq!(authors.resolve(&directory, "ADA", None).await, Some(1), "cached case-insensitively");
    assert_eq!(authors.resolve(&directory, "bob", Some("")).await, None, "no user-name fallback for the email rule");
    assert_eq!(directory.trace(), ["profile:ada", "email:ada@example.com", "profile:bob"]);
}

#[tokio::test]
async fn the_user_name_is_the_fallback_only_when_the_rule_allows_it() {
    let directory = Directory::default();
    let mut authors = AuthorMap::new(AuthorRule::EmailOrUsername);
    assert_eq!(authors.resolve(&directory, "bob", Some("bob@example.com")).await, Some(2));
    assert_eq!(directory.trace(), ["email:bob@example.com", "named:bob"], "a listed email skips the profile");
}
