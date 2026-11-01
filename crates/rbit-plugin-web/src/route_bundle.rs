//! Shared registration ownership, independent of feature-specific controllers.
use rbit_plugin_api::PluginId;
use rbit_plugin_runtime::PluginScope;

use crate::{
    MutationError, MutationRegistration, MutationRegistry, MutationRouteRegistration, PageError, PageRegistration, PageRegistry,
    PageRouteRegistration,
};

/// Reusable trusted compiled factory blueprints, capturing weak host references.
///
/// Staging consumes these vectors; cloning a blueprint retains the existing callback
/// handles. The host owns controller locks, lifecycle intent, scope admission,
/// authentication and disposal. This value grants no executable/package authority.
#[derive(Debug, Clone, Default)]
pub struct RouteBundle {
    pub pages: Vec<PageRegistration>,
    pub mutations: Vec<MutationRegistration>,
}

#[derive(Debug, thiserror::Error)]
pub enum RouteBundleError {
    #[error(transparent)]
    Page(#[from] PageError),
    #[error(transparent)]
    Mutation(#[from] MutationError),
}

impl RouteBundle {
    /// Stages pages first, then mutations, without publishing either batch.
    /// A second-stage failure explicitly removes the first reservation: dropping
    /// its handle alone would leave the batch owned by its PluginScope.
    pub fn stage(
        self,
        owner: PluginId,
        scope: &PluginScope,
        pages: &PageRegistry,
        mutations: &MutationRegistry,
    ) -> Result<StagedRouteActivation, RouteBundleError> {
        let pages = if self.pages.is_empty() { None } else { Some(pages.stage(owner.clone(), scope, scope.generation(), self.pages)?) };
        let mutations = if self.mutations.is_empty() {
            None
        } else {
            match mutations.stage(owner, scope, scope.generation(), self.mutations) {
                Ok(registration) => Some(registration),
                Err(error) => {
                    if let Some(pages) = &pages {
                        pages.unpublish();
                    }
                    return Err(error.into());
                }
            }
        };
        Ok(StagedRouteActivation { pages, mutations })
    }
}

/// Scoped page/mutation registration handles, not a lifecycle coordinator.
///
/// The host retains the scope, controller serialization and admission fence.
/// Publication is sequential with explicit rollback, NOT an atomic transaction
/// across registries: observers may see the intermediate publication. A failure
/// withdraws this activation's batches but never closes the caller's scope.
/// Registration rollback does not undo effects of already admitted operations.
///
/// Cloning/dropping handles preserves existing registry semantics: the scope owns
/// registrations even if all local handles are dropped. Use withdraw or quiesce
/// the scope explicitly; this type never introduces implicit Drop cleanup.
#[derive(Debug, Clone)]
pub struct StagedRouteActivation {
    pages: Option<PageRouteRegistration>,
    mutations: Option<MutationRouteRegistration>,
}
impl StagedRouteActivation {
    /// Publishes mutations before pages, matching the original host adapters.
    /// On either error, withdraws both batches before returning. Withdrawal is
    /// terminal for these handles; retry by staging a fresh bundle.
    pub fn publish(&self) -> Result<(), RouteBundleError> {
        if let Some(registration) = &self.mutations
            && let Err(error) = registration.publish()
        {
            self.withdraw();
            return Err(error.into());
        }
        if let Some(registration) = &self.pages
            && let Err(error) = registration.publish()
        {
            self.withdraw();
            return Err(error.into());
        }
        Ok(())
    }

    /// Revokes pages before mutations. Retained handles must obtain fresh
    /// admission and cannot regain authority after this explicit withdrawal.
    pub fn withdraw(&self) {
        if let Some(registration) = &self.pages {
            registration.unpublish();
        }
        if let Some(registration) = &self.mutations {
            registration.unpublish();
        }
    }
}
