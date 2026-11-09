//! Ports for managing and applying rules. The rule store keeps race authority: the rule
//! limit, per-repository pattern uniqueness and organisation-only teams are settled in
//! the same transaction as the write.

use std::{fmt::Display, future::Future, str::FromStr};

use super::{ProtectionId, ProtectionRule, RuleChange, RuleInput};
use crate::teams::Unique;

/// Shorthand for the rule shape a store's identifiers produce.
pub type StoredRule<S> = ProtectionRule<<S as RuleRepository>::User, <S as RuleRepository>::Team, <S as RuleRepository>::Context>;
/// Shorthand for the written rule shape a store accepts.
pub type StoredInput<S> = RuleInput<<S as RuleRepository>::User, <S as RuleRepository>::Team, <S as RuleRepository>::Context>;

/// Stored rules with their display records. Record types pass through unchanged.
pub trait RuleRepository: Sync {
    type Error: Send;
    type Repo: Copy + Send + Sync;
    type User: Copy + Eq + Send + Sync;
    type Team: Copy + Eq + Send + Sync;
    /// A required status check context; its parse error text is shown to the user.
    type Context: FromStr<Err: Display> + Ord + Clone + Send + Sync;
    /// A stored rule with what its pages show.
    type Record: Send;

    fn rule(record: &Self::Record) -> &StoredRule<Self>;
    fn into_rule(record: Self::Record) -> StoredRule<Self>;
    /// The repository's rules, by pattern.
    fn list(&self, repo: Self::Repo) -> impl Future<Output = Result<Vec<Self::Record>, Self::Error>> + Send;
    fn get(&self, repo: Self::Repo, id: ProtectionId) -> impl Future<Output = Result<Option<Self::Record>, Self::Error>> + Send;
    /// The teams of `user` among those the repository's rules name.
    fn teams_of(&self, repo: Self::Repo, user: Self::User) -> impl Future<Output = Result<Vec<Self::Team>, Self::Error>> + Send;
    /// `Taken` when the repository already has a rule for this pattern.
    fn create(
        &self,
        repo: Self::Repo,
        input: &StoredInput<Self>,
        creator: Self::User,
    ) -> impl Future<Output = Result<Unique<RuleChange>, Self::Error>> + Send;
    /// `Taken` when another rule of the repository has this pattern.
    fn update(
        &self,
        repo: Self::Repo,
        id: ProtectionId,
        input: &StoredInput<Self>,
    ) -> impl Future<Output = Result<Unique<RuleChange>, Self::Error>> + Send;
    /// Whether a rule was deleted.
    fn delete(&self, repo: Self::Repo, id: ProtectionId) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

/// What a username resolves to for an allowed-pushers list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Writer<U> {
    /// No user account has this name.
    Unknown,
    /// The user (by their own spelling) lacks write access to the repository.
    WithoutWrite(String),
    Writer(U),
}

/// Name lookups scoped to one repository and its owner. Write access is decided by the
/// repository access policy behind this port, never here.
pub trait RuleDirectory: Sync {
    type Error: Send;
    type User;
    type Team;
    /// A team of the owning organisation, as the rule pages list them.
    type OwnerTeam: Send;

    fn writer(&self, name: &str) -> impl Future<Output = Result<Writer<Self::User>, Self::Error>> + Send;
    /// The owning organisation's team called `name`.
    fn team_named(&self, name: &str) -> impl Future<Output = Result<Option<Self::Team>, Self::Error>> + Send;
    /// The owning organisation's teams, by name.
    fn owner_teams(&self) -> impl Future<Output = Result<Vec<Self::OwnerTeam>, Self::Error>> + Send;
}
