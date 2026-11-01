//! Narrow authoritative ports for compiled language fragments.
use crate::{AppState, state::WeakAppState};
use rbit_i18n_web::{InstallError, LanguageHost, LanguageRequest, PackSummary};
use rbit_services::{
    I18nRouteBundle, ServiceError,
    plugin_pages::{MutationError, PageError},
};

#[derive(Clone)]
struct Host(WeakAppState);
struct Request(AppState);
type Packs = std::iter::Map<std::vec::IntoIter<rbit_services::LanguagePackInfo>, fn(rbit_services::LanguagePackInfo) -> PackSummary>;
impl LanguageHost for Host {
    type Request = Request;
    fn connect(&self) -> Result<Request, PageError> {
        self.0.upgrade().map(Request).ok_or(PageError::Unavailable)
    }
}
impl LanguageRequest for Request {
    type Packs = Packs;
    fn available_locales(&self) -> Vec<String> {
        self.0.services.available_locales()
    }
    fn is_enabled(&self) -> bool {
        self.0.services.i18n_enabled()
    }
    async fn language_packs(&self, actor: i64) -> Result<Self::Packs, PageError> {
        let actor = super::actor(&self.0, actor).await.map_err(|_| PageError::Forbidden)?;
        self.0.services.language_packs(&actor).await.map(project_packs).map_err(|_| PageError::Unavailable)
    }
    async fn install_language_pack(&self, actor: i64, bytes: &[u8]) -> Result<(), InstallError<Self::Packs>> {
        let actor = super::actor(&self.0, actor).await.map_err(|error| match error {
            MutationError::Forbidden => InstallError::Forbidden,
            _ => InstallError::Unavailable,
        })?;
        match self.0.services.install_language_pack(&actor, bytes).await {
            Ok(_) => Ok(()),
            Err(ServiceError::Invalid(message)) => {
                let packs = self.0.services.language_packs(&actor).await.map_err(|_| InstallError::Unavailable)?;
                Err(InstallError::Invalid { message, packs: project_packs(packs) })
            }
            Err(ServiceError::Forbidden) => Err(InstallError::Forbidden),
            Err(_) => Err(InstallError::Unavailable),
        }
    }
    async fn remove_language_pack(&self, actor: i64, id: &str) -> Result<(), MutationError> {
        let actor = super::actor(&self.0, actor).await?;
        self.0
            .services
            .remove_language_pack(&actor, id)
            .await
            .map(|_| ())
            .map_err(|error| if matches!(error, ServiceError::Forbidden) { MutationError::Forbidden } else { MutationError::Unavailable })
    }
    async fn set_account_locale(&self, actor: i64, locale: Option<&str>) -> Result<(), MutationError> {
        let actor = super::actor(&self.0, actor).await?;
        self.0.services.set_account_locale(&actor, locale).await.map_err(|_| MutationError::Unavailable)
    }
}
pub(super) fn routes(weak: WeakAppState) -> Result<(I18nRouteBundle, I18nRouteBundle), ServiceError> {
    let (active, management) = rbit_i18n_web::routes(Host(weak)).map_err(ServiceError::Invalid)?;
    Ok((
        I18nRouteBundle { pages: active.pages, mutations: active.mutations },
        I18nRouteBundle { pages: management.pages, mutations: management.mutations },
    ))
}

fn summary(pack: rbit_services::LanguagePackInfo) -> PackSummary {
    PackSummary {
        id: pack.id,
        locale: pack.locale,
        version: pack.version,
        message_count: pack.message_count,
        message_total: pack.message_total,
    }
}

fn project_packs(packs: Vec<rbit_services::LanguagePackInfo>) -> Packs {
    packs.into_iter().map(summary as fn(rbit_services::LanguagePackInfo) -> PackSummary)
}
