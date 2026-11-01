//! Host-owned factory blueprints attached to the actual Actions activation scope.

use std::sync::{Arc, Mutex};

#[cfg(feature = "actions")]
use rbit_plugin_runtime::PluginScope;
use rbit_plugin_web::{MutationRegistration, MutationRegistry, PageRegistration, PageRegistry};
#[cfg(feature = "actions")]
use rbit_plugin_web::{MutationRouteRegistration, PageRouteRegistration};

use crate::ServiceError;

/// Reusable compiled factories. Capture weak host references; a blueprint must
/// not retain the Services composition root that owns its route registry.
#[derive(Debug, Clone, Default)]
pub struct ActionsRouteBundle {
    pub pages: Vec<PageRegistration>,
    pub mutations: Vec<MutationRegistration>,
}

#[cfg(feature = "actions")]
#[derive(Debug, Clone)]
struct Activation {
    scope: PluginScope,
    pages: Option<PageRouteRegistration>,
    mutations: Option<MutationRouteRegistration>,
}

#[cfg(feature = "actions")]
impl Activation {
    fn publish(&self) -> Result<(), ServiceError> {
        if let Some(registration) = &self.mutations {
            registration.publish().map_err(ServiceError::internal)?;
        }
        if let Some(registration) = &self.pages {
            registration.publish().map_err(ServiceError::internal)?;
        }
        Ok(())
    }

    fn withdraw(&self) {
        if let Some(registration) = &self.pages {
            registration.unpublish();
        }
        if let Some(registration) = &self.mutations {
            registration.unpublish();
        }
    }
}

#[derive(Debug, Default)]
struct State {
    closed: bool,
    blueprint: Option<ActionsRouteBundle>,
    #[cfg(feature = "actions")]
    active: Option<Activation>,
}

#[derive(Debug, Clone)]
pub(crate) struct ActionsRoutes {
    #[cfg(feature = "actions")]
    pages: PageRegistry,
    #[cfg(feature = "actions")]
    mutations: MutationRegistry,
    state: Arc<Mutex<State>>,
}

impl ActionsRoutes {
    pub(crate) fn new(pages: PageRegistry, mutations: MutationRegistry) -> Self {
        #[cfg(not(feature = "actions"))]
        drop((pages, mutations));
        Self {
            #[cfg(feature = "actions")]
            pages,
            #[cfg(feature = "actions")]
            mutations,
            state: Arc::new(Mutex::new(State::default())),
        }
    }

    /// Serialized by the Actions controller when present. Unavailable builds
    /// deliberately discard native factories, matching the i18n binding contract.
    pub(crate) fn bind(&self, bundle: ActionsRouteBundle) -> Result<(), ServiceError> {
        #[cfg(not(feature = "actions"))]
        let bundle = {
            drop(bundle);
            ActionsRouteBundle::default()
        };
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            return Err(ServiceError::Conflict("server shutdown"));
        }
        if state.blueprint.is_some() {
            return Err(ServiceError::Conflict("Actions routes already bound"));
        }
        state.blueprint = Some(bundle);
        Ok(())
    }

    #[cfg(feature = "actions")]
    fn stage(&self, scope: &PluginScope, bundle: ActionsRouteBundle) -> Result<Activation, ServiceError> {
        let owner: rbit_plugin_api::PluginId = "rbit.actions".parse().map_err(ServiceError::internal)?;
        let pages = if bundle.pages.is_empty() {
            None
        } else {
            Some(self.pages.stage(owner.clone(), scope, scope.generation(), bundle.pages).map_err(ServiceError::internal)?)
        };
        let mutations = if bundle.mutations.is_empty() {
            None
        } else {
            match self.mutations.stage(owner, scope, scope.generation(), bundle.mutations) {
                Ok(registration) => Some(registration),
                Err(error) => {
                    if let Some(pages) = &pages {
                        pages.unpublish();
                    }
                    return Err(ServiceError::internal(error));
                }
            }
        };
        Ok(Activation { scope: scope.clone(), pages, mutations })
    }

    #[cfg(feature = "actions")]
    pub(crate) fn stage_active(&self, scope: &PluginScope) -> Result<(), ServiceError> {
        let blueprint = {
            let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.closed {
                return Err(ServiceError::Conflict("server shutdown"));
            }
            state.blueprint.clone()
        };
        let Some(blueprint) = blueprint else { return Ok(()) };
        let activation = self.stage(scope, blueprint)?;
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            drop(state);
            activation.withdraw();
            return Err(ServiceError::Conflict("server shutdown"));
        }
        let old = state.active.replace(activation);
        drop(state);
        if let Some(old) = old {
            old.withdraw();
        }
        Ok(())
    }

    #[cfg(feature = "actions")]
    pub(crate) fn publish_active(&self) -> Result<(), ServiceError> {
        let activation = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).active.clone();
        if let Some(activation) = activation {
            activation.publish()?;
        }
        Ok(())
    }

    #[cfg(feature = "actions")]
    pub(crate) fn withdraw_active(&self) {
        let activation = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).active.take();
        if let Some(activation) = activation {
            activation.withdraw();
        }
    }

    pub(crate) fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        let blueprint = state.blueprint.take();
        #[cfg(feature = "actions")]
        let activation = state.active.take();
        drop(state);
        drop(blueprint);
        #[cfg(feature = "actions")]
        if let Some(activation) = activation {
            activation.scope.quiesce();
            activation.withdraw();
        }
    }
}
