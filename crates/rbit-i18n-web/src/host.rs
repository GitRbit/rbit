//! The host owns authorization, persistence and provider lifecycle.
use rbit_plugin_web::{MutationError, PageError};
use std::future::Future;

/// Passive inventory projection; compressed archives and storage handles stay host-owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackSummary {
    pub id: String,
    pub locale: String,
    pub version: String,
    pub message_count: usize,
    pub message_total: usize,
}
#[derive(Debug)]
pub enum InstallError<Packs> {
    Invalid { message: String, packs: Packs },
    Forbidden,
    Unavailable,
}
/// Long-lived blueprints hold only this cloneable weak host adapter.
/// Connecting retains a host for one callback, never for the blueprint lifetime.
pub trait LanguageHost: Clone + Send + Sync + 'static {
    type Request: LanguageRequest;
    fn connect(&self) -> Result<Self::Request, PageError>;
}
/// Request-scoped host ports. Implementations must reload the active actor and
/// authorize every account/admin write and inventory read; route audience alone
/// never authorizes these operations. Upload bytes remain borrowed and bounded.
/// Returned futures are concrete and introduce no additional async boxing.
pub trait LanguageRequest: Send + Sync + 'static {
    /// Owned exact-size projection of the bounded host inventory. A concrete
    /// lazy iterator moves each item's fields directly into the final display
    /// rows without collecting an intermediate inventory vector. It carries no
    /// storage handle or request borrow; the host retains authority and quotas.
    type Packs: ExactSizeIterator<Item = PackSummary> + Send + 'static;
    fn available_locales(&self) -> Vec<String>;
    fn is_enabled(&self) -> bool;
    fn language_packs(&self, actor: i64) -> impl Future<Output = Result<Self::Packs, PageError>> + Send;
    fn install_language_pack(&self, actor: i64, bytes: &[u8]) -> impl Future<Output = Result<(), InstallError<Self::Packs>>> + Send;
    fn remove_language_pack(&self, actor: i64, id: &str) -> impl Future<Output = Result<(), MutationError>> + Send;
    fn set_account_locale(&self, actor: i64, locale: Option<&str>) -> impl Future<Output = Result<(), MutationError>> + Send;
}
