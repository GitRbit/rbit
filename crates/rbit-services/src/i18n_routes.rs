//! Host-supplied compiled factories, attached to installed or active i18n scopes.

use std::sync::{Arc, Mutex};

use rbit_plugin_api::Generation;
use rbit_plugin_runtime::PluginScope;
use rbit_plugin_web::{
    MutationRegistration, MutationRegistry, MutationRouteRegistration, PageRegistration, PageRegistry, PageRouteRegistration,
};

use crate::ServiceError;

/// Reusable compiled factory blueprints. Capture weak host references so the
/// composition root does not retain itself through these long-lived callbacks.
#[derive(Debug, Clone, Default)]
pub struct I18nRouteBundle {
    pub pages: Vec<PageRegistration>,
    pub mutations: Vec<MutationRegistration>,
}

#[derive(Debug, Clone)]
struct Activation {
    scope: PluginScope,
    pages: Option<PageRouteRegistration>,
    mutations: Option<MutationRouteRegistration>,
}

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
    blueprint: Option<I18nRouteBundle>,
    active: Option<Activation>,
    management: Option<Activation>,
}

#[derive(Debug, Clone)]
pub(crate) struct I18nRoutes {
    pages: PageRegistry,
    mutations: MutationRegistry,
    state: Arc<Mutex<State>>,
}

impl I18nRoutes {
    pub(crate) fn new(pages: PageRegistry, mutations: MutationRegistry) -> Self {
        Self { pages, mutations, state: Arc::new(Mutex::new(State::default())) }
    }

    fn stage(&self, owner: &str, scope: &PluginScope, bundle: I18nRouteBundle) -> Result<Activation, ServiceError> {
        let owner: rbit_plugin_api::PluginId = owner.parse().map_err(ServiceError::internal)?;
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

    /// Called under the slot's controller lock; no callback code runs under state locks.
    pub(crate) fn bind(&self, active: I18nRouteBundle, management: I18nRouteBundle) -> Result<(), ServiceError> {
        {
            let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.closed {
                return Err(ServiceError::Conflict("server shutdown"));
            }
            if state.blueprint.is_some() {
                return Err(ServiceError::Conflict("i18n routes already bound"));
            }
        }
        let scope = PluginScope::new(Generation::INITIAL);
        let activation = self.stage("rbit.i18n-admin", &scope, management)?;
        if let Err(error) = activation.publish() {
            activation.withdraw();
            scope.quiesce();
            return Err(error);
        }
        let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.closed {
            drop(state);
            activation.withdraw();
            scope.quiesce();
            return Err(ServiceError::Conflict("server shutdown"));
        }
        state.blueprint = Some(active);
        state.management = Some(activation);
        Ok(())
    }

    #[cfg(feature = "i18n")]
    pub(crate) fn stage_active(&self, scope: &PluginScope) -> Result<(), ServiceError> {
        let blueprint = {
            let state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if state.closed {
                return Err(ServiceError::Conflict("server shutdown"));
            }
            state.blueprint.clone()
        };
        let Some(blueprint) = blueprint else { return Ok(()) };
        let activation = self.stage("rbit.i18n", scope, blueprint)?;
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

    #[cfg(feature = "i18n")]
    pub(crate) fn publish_active(&self) -> Result<(), ServiceError> {
        let activation = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).active.clone();
        if let Some(activation) = activation {
            activation.publish()?;
        }
        Ok(())
    }

    pub(crate) fn withdraw_active(&self) {
        let activation = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner).active.take();
        if let Some(activation) = activation {
            activation.withdraw();
        }
    }

    pub(crate) fn close(&self) {
        let (active, management, blueprint) = {
            let mut state = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            state.closed = true;
            (state.active.take(), state.management.take(), state.blueprint.take())
        };
        drop(blueprint);
        for activation in [active, management].into_iter().flatten() {
            activation.scope.quiesce();
            activation.withdraw();
        }
    }
}
