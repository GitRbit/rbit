//! Scripted effects are lazy and expose exact selected-page work without a database.
use rbit_collaboration::{
    issues::{
        IssueMetadata, IssueMetadataPort, IssueReadPort, IssueSearchPort, MetadataNeeds, MetadataRows, OneIssueMetadata, SearchFilter,
    },
    labels::LabelSnapshot,
    threads::{Issue, IssueId},
};
use rbit_contracts::{AccountId, RepoId, Timestamp};
use rbit_identity_models::{Account, AccountKind, User};
use rbit_repositories::OwnedRepository;
use std::{collections::HashMap, sync::Mutex};
#[derive(Debug, Default)]
pub struct Port {
    pub log: Mutex<Vec<String>>,
    pub filters: Mutex<Vec<SearchFilter>>,
    pub inactive: bool,
    pub fail_name: bool,
}
impl Port {
    pub fn call(&self, s: impl Into<String>) {
        self.log.lock().unwrap().push(s.into());
    }
    pub fn calls(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}
pub fn user() -> User {
    User {
        account: Account {
            id: AccountId(4),
            kind: AccountKind::User,
            name: "viewer".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(1),
        },
        email: "viewer@example.test".parse().unwrap(),
        is_admin: false,
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl<'a> IssueReadPort for &'a Port {
    type Error = &'static str;
    type Search = &'a Port;
    type Metadata = &'a Port;
    fn search_store(&self) -> Self::Search {
        self.call("search_store");
        *self
    }
    fn metadata_store(&self) -> Self::Metadata {
        self.call("metadata_store");
        *self
    }
    async fn account_named(&self, name: &str) -> Result<Option<Account>, Self::Error> {
        self.call(format!("name:{name}"));
        if self.fail_name { Err("name") } else { Ok(None) }
    }
    async fn active_user(&self, _: AccountId) -> Result<Option<User>, Self::Error> {
        self.call("active");
        Ok((!self.inactive).then(user))
    }
    async fn accounts(&self, ids: &[AccountId]) -> Result<Vec<Account>, Self::Error> {
        self.call(format!("accounts:{ids:?}"));
        Ok(Vec::new())
    }
    async fn hydrate(
        &self,
        issues: &[IssueId],
        authors: &[AccountId],
        needs: MetadataNeeds,
    ) -> Result<(Vec<Account>, HashMap<IssueId, IssueMetadata>), Self::Error> {
        self.call(format!("hydrate:{issues:?}:{authors:?}:{needs:?}"));
        Ok((Vec::new(), HashMap::new()))
    }
    async fn readable_repositories(&self, ids: &[RepoId], viewer: &User) -> Result<HashMap<RepoId, OwnedRepository>, Self::Error> {
        self.call(format!("readable:{ids:?}:{}", viewer.is_admin));
        Ok(HashMap::new())
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl IssueSearchPort for &Port {
    type Error = &'static str;
    async fn state_counts(&self, filter: &SearchFilter) -> Result<(u64, u64), Self::Error> {
        self.call(format!("counts:{}:{}", filter.repo.0, filter.dashboard.is_some()));
        self.filters.lock().unwrap().push(filter.clone());
        Ok((26, 3))
    }
    async fn search(&self, _: &SearchFilter, limit: u32, offset: u64) -> Result<Vec<Issue>, Self::Error> {
        self.call(format!("rows:{limit}:{offset}"));
        Ok(vec![super::fixtures::issue()])
    }
}
#[allow(clippy::unused_async_trait_impl, reason = "scripted persistence effects stay lazy until poll like database queries")]
impl IssueMetadataPort for &Port {
    type Error = &'static str;
    async fn one(&self, issue: IssueId) -> Result<OneIssueMetadata, Self::Error> {
        self.call(format!("one:{}", issue.0));
        Ok(OneIssueMetadata::default())
    }
    async fn batch(&self, issues: &[IssueId]) -> Result<MetadataRows, Self::Error> {
        self.call(format!("batch:{issues:?}"));
        Ok((Vec::new(), Vec::new(), Vec::new()))
    }
    async fn labels(&self, issues: &[IssueId]) -> Result<Vec<(IssueId, LabelSnapshot)>, Self::Error> {
        self.call(format!("labels:{issues:?}"));
        Ok(Vec::new())
    }
}
