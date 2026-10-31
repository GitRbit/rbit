//! Runtime localization publication and data-only language-pack administration.

use std::sync::{
    Arc, RwLock,
    atomic::{AtomicBool, Ordering},
};

use rbit_core::{Timestamp, User};
use rbit_messages::LocaleSnapshot;
use rbit_plugin_runtime::PluginStatus;
pub use rbit_storage::sqlite::language_packs::LanguagePackInfo;
use rbit_storage::sqlite::language_packs::MAX_LANGUAGE_PACK_BYTES;

use crate::{
    ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry, AuditService},
};

const I18N_ID: &str = "rbit.i18n";

#[cfg(feature = "i18n")]
use rbit_i18n::{LanguagePack, PackSet};
#[cfg(feature = "i18n")]
use rbit_messages::{English, MessageSchema, Messages};
#[cfg(feature = "i18n")]
use rbit_plugin_api::{DesiredState, PluginManifest, PluginState};
#[cfg(feature = "i18n")]
use rbit_plugin_runtime::{Plugin, PluginFuture, PluginManager, PluginScope, UiRegistration};
#[cfg(feature = "i18n")]
use rbit_storage::sqlite::language_packs::{LanguagePack as StoredPack, NewLanguagePack};
#[cfg(feature = "i18n")]
use std::{sync::Mutex, time::Duration};

#[derive(Debug)]
struct Publication {
    revision: u64,
    #[cfg(feature = "i18n")]
    live: Option<Arc<LiveCatalog>>,
}

#[cfg(feature = "i18n")]
#[derive(Debug)]
struct LiveCatalog {
    catalogs: PackSet,
    packs: Vec<Arc<LanguagePack>>,
    revision: u64,
}

#[derive(Debug)]
struct Inner {
    ctx: ServiceContext,
    publication: Arc<RwLock<Publication>>,
    status: RwLock<Option<PluginStatus>>,
    shutting_down: AtomicBool,
    controller: Arc<tokio::sync::Mutex<Controller>>,
    #[cfg(feature = "i18n")]
    ui_activation: Arc<Mutex<Option<(PluginScope, UiRegistration)>>>,
}

#[derive(Debug, Clone)]
pub struct I18nSlot {
    inner: Arc<Inner>,
}

#[cfg(not(feature = "i18n"))]
#[derive(Debug)]
struct Controller;

#[cfg(feature = "i18n")]
#[derive(Debug)]
struct Controller {
    manager: PluginManager,
    plugin: Arc<EmbeddedI18n>,
}

#[cfg(feature = "i18n")]
#[derive(Debug)]
struct EmbeddedI18n {
    manifest: PluginManifest,
    ctx: ServiceContext,
    publication: Arc<RwLock<Publication>>,
    staged: Arc<Mutex<Option<Arc<LiveCatalog>>>>,
    ui: rbit_plugin_runtime::UiRegistry,
    ui_activation: Arc<Mutex<Option<(PluginScope, UiRegistration)>>>,
}

#[cfg(feature = "i18n")]
impl Plugin for EmbeddedI18n {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn start(&self, scope: PluginScope) -> PluginFuture {
        let registration = self.ui.stage(self.manifest.id.clone(), &scope, scope.generation(), crate::ui::i18n());
        let ui_activation = Arc::clone(&self.ui_activation);
        let ctx = self.ctx.clone();
        let staged = Arc::clone(&self.staged);
        let publication = Arc::clone(&self.publication);
        Box::pin(async move {
            *ui_activation.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some((scope.clone(), registration.map_err(|error| error.to_string())?));
            let records = ctx.database.language_packs().list().await.map_err(|error| error.to_string())?;
            let revision = next_revision(&publication).map_err(|error| error.to_string())?;
            let lease = scope.try_acquire(scope.generation()).map_err(|error| error.to_string())?;
            let catalogs = tokio::task::spawn_blocking(move || {
                let _lease = lease;
                build_catalog(parse_records(records)?, revision)
            })
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?;
            *staged.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Arc::new(catalogs));
            Ok(())
        })
    }

    fn stop(&self) -> PluginFuture {
        self.staged.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        Box::pin(async { Ok(()) })
    }
}

impl I18nSlot {
    pub(crate) async fn new(ctx: ServiceContext, ui: rbit_plugin_runtime::UiRegistry) -> Result<Self, ServiceError> {
        let revision = initial_revision()?;
        let enabled = ctx.database.plugin_settings().desired_enabled(I18N_ID).await?.unwrap_or(ctx.config.i18n.enabled);
        #[cfg(not(feature = "i18n"))]
        {
            drop(ui);
            if enabled {
                return Err(ServiceError::Invalid("this distribution was built without i18n".to_owned()));
            }
            Ok(Self {
                inner: Arc::new(Inner {
                    ctx,
                    publication: Arc::new(RwLock::new(Publication { revision })),
                    status: RwLock::new(None),
                    shutting_down: AtomicBool::new(false),
                    controller: Arc::new(tokio::sync::Mutex::new(Controller)),
                }),
            })
        }
        #[cfg(feature = "i18n")]
        {
            // No workload credentials exist for this data-only provider. Previous process
            // memory is gone, and English stays published until a new catalog is ready.
            ctx.database.plugin_settings().complete_disable(I18N_ID).await?;
            let publication = Arc::new(RwLock::new(Publication { revision, live: None }));
            let ui_activation = Arc::new(Mutex::new(None));
            let plugin = Arc::new(EmbeddedI18n {
                manifest: PluginManifest {
                    id: I18N_ID.parse().map_err(ServiceError::internal)?,
                    version: env!("CARGO_PKG_VERSION").parse().map_err(ServiceError::internal)?,
                    host_api: "^1".parse().map_err(ServiceError::internal)?,
                    requires: Vec::new(),
                    provides: Vec::new(),
                    capabilities: Vec::new(),
                },
                ctx: ctx.clone(),
                publication: Arc::clone(&publication),
                staged: Arc::new(Mutex::new(None)),
                ui,
                ui_activation: Arc::clone(&ui_activation),
            });
            let mut manager = PluginManager::new(semver::Version::new(1, 0, 0), Duration::from_secs(30));
            manager.install(plugin.clone()).map_err(ServiceError::internal)?;
            let slot = Self {
                inner: Arc::new(Inner {
                    ctx,
                    publication,
                    status: RwLock::new(manager.status(&plugin.manifest.id)),
                    shutting_down: AtomicBool::new(false),
                    controller: Arc::new(tokio::sync::Mutex::new(Controller { manager, plugin })),
                    ui_activation,
                }),
            };
            if enabled {
                let mut controller = slot.inner.controller.lock().await;
                let id = controller.plugin.manifest.id.clone();
                match controller.manager.enable(&id).await {
                    Ok(()) => slot.publish_staged(&controller)?,
                    Err(error) => tracing::warn!(%error, "i18n could not activate; English remains available"),
                }
                slot.cache_status(controller.manager.status(&id));
            }
            Ok(slot)
        }
    }

    pub fn revision(&self) -> u64 {
        self.inner.publication.read().unwrap_or_else(std::sync::PoisonError::into_inner).revision
    }

    #[allow(clippy::unused_self)] // Uniform availability query when the optional provider is absent.
    pub fn is_enabled(&self) -> bool {
        #[cfg(feature = "i18n")]
        {
            self.inner.publication.read().unwrap_or_else(std::sync::PoisonError::into_inner).live.is_some()
        }
        #[cfg(not(feature = "i18n"))]
        {
            false
        }
    }

    #[cfg(not(feature = "i18n"))]
    pub fn snapshot(&self, _account: Option<&str>, _cookie: Option<&str>, _accept: Option<&str>) -> LocaleSnapshot {
        LocaleSnapshot::english(self.revision())
    }

    #[cfg(feature = "i18n")]
    pub fn snapshot(&self, account: Option<&str>, cookie: Option<&str>, accept: Option<&str>) -> LocaleSnapshot {
        let publication = self.inner.publication.read().unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(feature = "i18n")]
        if let Some(live) = &publication.live {
            return live.catalogs.negotiate(account, cookie, accept, &self.inner.ctx.config.i18n.default_locale);
        }
        LocaleSnapshot::english(publication.revision)
    }

    #[allow(clippy::unused_self)] // The core-only language list contains English without a provider.
    pub fn available_locales(&self) -> Vec<String> {
        let mut locales = vec!["en".to_owned()];
        #[cfg(feature = "i18n")]
        if let Some(live) = &self.inner.publication.read().unwrap_or_else(std::sync::PoisonError::into_inner).live {
            locales.extend(live.catalogs.locales().into_iter().map(|info| info.locale));
        }
        locales.sort();
        locales.dedup();
        locales
    }

    pub fn status(&self) -> Option<PluginStatus> {
        self.inner.status.read().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    #[cfg(feature = "i18n")]
    fn cache_status(&self, status: Option<PluginStatus>) {
        *self.inner.status.write().unwrap_or_else(std::sync::PoisonError::into_inner) = status;
    }

    pub(crate) async fn set_enabled(&self, actor: &User, enabled: bool) -> Result<(), ServiceError> {
        self.check_running()?;
        let mut controller =
            Arc::clone(&self.inner.controller).try_lock_owned().map_err(|_| ServiceError::Conflict("plugin lifecycle operation"))?;
        let slot = self.clone();
        let actor = actor.clone();
        tokio::spawn(async move {
            slot.check_running()?;
            #[cfg(not(feature = "i18n"))]
            {
                let _controller = &mut controller;
                if enabled {
                    return Err(ServiceError::Invalid("this distribution was built without i18n".to_owned()));
                }
                slot.inner.ctx.database.plugin_settings().set_desired_enabled(I18N_ID, false, Some(actor.id()), Timestamp::now()).await?;
                slot.unpublish();
                slot.inner.ctx.database.plugin_settings().complete_disable(I18N_ID).await?;
                slot.audit(&actor, I18N_ID, "i18n disabled").await;
                Ok(())
            }
            #[cfg(feature = "i18n")]
            {
                slot.inner.ctx.database.plugin_settings().set_desired_enabled(I18N_ID, enabled, Some(actor.id()), Timestamp::now()).await?;
                let id = controller.plugin.manifest.id.clone();
                let mut status = controller.manager.status(&id);
                if let Some(status) = &mut status {
                    status.desired = if enabled { DesiredState::Enabled } else { DesiredState::Disabled };
                    status.state = if enabled { PluginState::Preparing } else { PluginState::Quiescing };
                }
                slot.cache_status(status);
                if !enabled {
                    slot.unpublish();
                }
                slot.audit(&actor, I18N_ID, if enabled { "i18n enable requested" } else { "i18n disable requested" }).await;
                let result = if enabled {
                    controller
                        .manager
                        .enable(&id)
                        .await
                        .map_err(ServiceError::internal)
                        .and_then(|()| if slot.is_enabled() { Ok(()) } else { slot.publish_staged(&controller) })
                } else {
                    match controller.manager.disable(&id).await {
                        Ok(()) => slot.inner.ctx.database.plugin_settings().complete_disable(I18N_ID).await.map_err(Into::into),
                        Err(error) => Err(ServiceError::internal(error)),
                    }
                };
                slot.cache_status(controller.manager.status(&id));
                result
            }
        })
        .await
        .map_err(ServiceError::internal)?
    }

    #[cfg(not(feature = "i18n"))]
    pub(crate) fn install(&self, _actor: &User, bytes: &[u8]) -> std::future::Ready<Result<LanguagePackInfo, ServiceError>> {
        let result = self.check_running().and_then(|()| {
            if bytes.is_empty() || bytes.len() > MAX_LANGUAGE_PACK_BYTES {
                Err(ServiceError::Invalid("a language pack must contain between 1 byte and 2 MiB of compressed data".to_owned()))
            } else {
                Err(ServiceError::Invalid("this distribution was built without i18n".to_owned()))
            }
        });
        std::future::ready(result)
    }

    #[cfg(feature = "i18n")]
    pub(crate) async fn install(&self, actor: &User, bytes: &[u8]) -> Result<LanguagePackInfo, ServiceError> {
        self.check_running()?;
        if bytes.is_empty() || bytes.len() > MAX_LANGUAGE_PACK_BYTES {
            return Err(ServiceError::Invalid("a language pack must contain between 1 byte and 2 MiB of compressed data".to_owned()));
        }
        #[cfg(feature = "i18n")]
        {
            let controller =
                Arc::clone(&self.inner.controller).try_lock_owned().map_err(|_| ServiceError::Conflict("plugin lifecycle operation"))?;
            let slot = self.clone();
            let actor = actor.clone();
            let bytes = bytes.to_vec();
            tokio::spawn(async move {
                slot.check_running()?;
                let input = bytes.clone();
                let incoming = tokio::task::spawn_blocking(move || LanguagePack::parse(&input, &MessageSchema::builtin()))
                    .await
                    .map_err(ServiceError::internal)?
                    .map_err(|error| ServiceError::Invalid(error.to_string()))?;
                let incoming = Arc::new(incoming);
                if let Some(existing) = slot.current_installation(&actor, &incoming, &bytes).await? {
                    return Ok(existing);
                }
                let mut packs = slot.load_packs_excluding(Some(&incoming.manifest().id)).await?;
                packs.push(Arc::clone(&incoming));
                let staged = prepare_catalog(packs, next_revision(&slot.inner.publication)?).await?;
                let manifest = incoming.manifest();
                let version = manifest.version.to_string();
                let now = Timestamp::now();
                let stored = NewLanguagePack {
                    id: &manifest.id,
                    locale: &manifest.locale,
                    version: &version,
                    bytes: &bytes,
                    digest: incoming.digest(),
                    message_count: incoming.message_count(),
                    message_total: MessageSchema::builtin().len(),
                    installed_by: Some(actor.id()),
                    updated_at: now,
                };
                if !slot.inner.ctx.database.language_packs().put(&stored).await? {
                    return Err(ServiceError::Invalid("at most 64 language packs may be installed".to_owned()));
                }
                let published = slot.apply_catalog(staged, &controller);
                slot.audit(&actor, &manifest.id, "language pack installed or updated").await;
                published?;
                Ok(LanguagePackInfo {
                    id: manifest.id.clone(),
                    locale: manifest.locale.clone(),
                    version,
                    digest: incoming.digest().to_owned(),
                    bytes: bytes.len(),
                    message_count: incoming.message_count(),
                    message_total: MessageSchema::builtin().len(),
                    updated_at: now,
                })
            })
            .await
            .map_err(ServiceError::internal)?
        }
    }

    pub(crate) async fn remove(&self, actor: &User, id: &str) -> Result<bool, ServiceError> {
        self.check_running()?;
        let controller =
            Arc::clone(&self.inner.controller).try_lock_owned().map_err(|_| ServiceError::Conflict("plugin lifecycle operation"))?;
        let slot = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        tokio::spawn(async move {
            slot.check_running()?;
            if slot.inner.ctx.database.language_packs().get(&id).await?.is_none() {
                return Ok(false);
            }
            #[cfg(feature = "i18n")]
            let staged = if slot.is_enabled() {
                let packs = slot.load_packs_excluding(Some(&id)).await?;
                Some(prepare_catalog(packs, next_revision(&slot.inner.publication)?).await?)
            } else {
                // An inactive or failed provider must remain repairable even if several
                // independent stored archives are corrupt or have missing parents.
                None
            };
            let removed = slot.inner.ctx.database.language_packs().remove(&id).await?;
            #[cfg(feature = "i18n")]
            let published = if let Some(staged) = staged {
                slot.apply_catalog(staged, &controller)
            } else {
                slot.unpublish();
                Ok(())
            };
            #[cfg(not(feature = "i18n"))]
            let published = {
                let _controller = &controller;
                slot.unpublish();
                Ok::<(), ServiceError>(())
            };
            slot.audit(&actor, &id, "language pack removed").await;
            published?;
            Ok(removed)
        })
        .await
        .map_err(ServiceError::internal)?
    }

    #[cfg(feature = "i18n")]
    async fn current_installation(
        &self,
        actor: &User,
        incoming: &LanguagePack,
        bytes: &[u8],
    ) -> Result<Option<LanguagePackInfo>, ServiceError> {
        let Some(existing) = self.inner.ctx.database.language_packs().get(&incoming.manifest().id).await? else { return Ok(None) };
        let current: semver::Version = existing.version.parse().map_err(|_| {
            ServiceError::Invalid(
                "stored package version is invalid; disable i18n and remove the corrupt package before reinstalling".to_owned(),
            )
        })?;
        match incoming.manifest().version.cmp_precedence(&current) {
            std::cmp::Ordering::Less => {
                Err(ServiceError::Invalid("language-pack downgrades require removal of the installed package first".to_owned()))
            }
            std::cmp::Ordering::Equal => {
                if incoming.manifest().version.to_string() != existing.version
                    || incoming.digest() != existing.digest
                    || bytes != existing.bytes
                    || incoming.manifest().locale != existing.locale
                {
                    return Err(ServiceError::Invalid(
                        "installed language-pack versions are immutable; install a higher version to replace or repair this package"
                            .to_owned(),
                    ));
                }
                let metadata = self
                    .inner
                    .ctx
                    .database
                    .language_packs()
                    .metadata()
                    .await?
                    .into_iter()
                    .find(|record| record.id == existing.id)
                    .ok_or(ServiceError::Conflict("language pack inventory"))?;
                self.audit(actor, &existing.id, "language pack installation already current").await;
                Ok(Some(metadata))
            }
            std::cmp::Ordering::Greater => Ok(None),
        }
    }

    fn check_running(&self) -> Result<(), ServiceError> {
        if self.inner.shutting_down.load(Ordering::Acquire) { Err(ServiceError::Conflict("server shutdown")) } else { Ok(()) }
    }

    async fn audit(&self, actor: &User, target: &str, detail: &str) {
        AuditService::from_store(self.inner.ctx.database.audit())
            .record(AuditEntry::new(AuditAction::InstanceSettingsChanged).by(actor).on(target).detail(detail))
            .await;
    }

    fn unpublish(&self) {
        #[cfg(feature = "i18n")]
        if let Some((scope, registration)) = &*self.inner.ui_activation.lock().unwrap_or_else(std::sync::PoisonError::into_inner) {
            registration.unpublish();
            scope.quiesce();
        }
        let mut publication = self.inner.publication.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        #[cfg(feature = "i18n")]
        {
            publication.live = None;
        }
        publication.revision = publication.revision.saturating_add(1);
    }

    #[cfg(feature = "i18n")]
    fn publish_staged(&self, controller: &Controller) -> Result<(), ServiceError> {
        let staged = controller
            .plugin
            .staged
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or_else(|| ServiceError::internal(std::io::Error::other("i18n activated without prepared catalogs")))?;
        self.publish(staged)
    }

    #[cfg(feature = "i18n")]
    fn publish(&self, staged: Arc<LiveCatalog>) -> Result<(), ServiceError> {
        let mut publication = self.inner.publication.write().unwrap_or_else(std::sync::PoisonError::into_inner);
        self.check_running()?;
        if publication.revision.checked_add(1) != Some(staged.revision) {
            return Err(ServiceError::Conflict("catalog revision"));
        }
        publication.revision = staged.revision;
        publication.live = Some(staged);
        drop(publication);
        let registration = self
            .inner
            .ui_activation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|(_, registration)| registration.clone());
        if let Err(error) = registration
            .ok_or_else(|| ServiceError::internal(std::io::Error::other("i18n activated without staged UI")))
            .and_then(|registration| registration.publish().map_err(ServiceError::internal))
        {
            self.unpublish();
            return Err(error);
        }
        Ok(())
    }

    #[cfg(feature = "i18n")]
    fn apply_catalog(&self, staged: Arc<LiveCatalog>, controller: &Controller) -> Result<(), ServiceError> {
        if self.is_enabled() {
            self.publish(Arc::clone(&staged))?;
            *controller.plugin.staged.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(staged);
        } else {
            self.unpublish();
        }
        Ok(())
    }

    #[cfg(feature = "i18n")]
    async fn load_packs_excluding(&self, exclude: Option<&str>) -> Result<Vec<Arc<LanguagePack>>, ServiceError> {
        if let Some(live) = &self.inner.publication.read().unwrap_or_else(std::sync::PoisonError::into_inner).live {
            return Ok(live.packs.iter().filter(|pack| Some(pack.manifest().id.as_str()) != exclude).cloned().collect());
        }
        let mut records = self.inner.ctx.database.language_packs().list().await?;
        records.retain(|record| Some(record.id.as_str()) != exclude);
        tokio::task::spawn_blocking(move || parse_records(records)).await.map_err(ServiceError::internal)?
    }

    pub(crate) async fn shutdown(&self) {
        self.inner.shutting_down.store(true, Ordering::Release);
        self.unpublish();
        let mut controller = self.inner.controller.lock().await;
        self.unpublish();
        #[cfg(feature = "i18n")]
        {
            let id = controller.plugin.manifest.id.clone();
            if let Err(error) = controller.manager.disable(&id).await {
                tracing::warn!(%error, "i18n cleanup remained incomplete");
            }
            self.cache_status(controller.manager.status(&id));
        }
        #[cfg(not(feature = "i18n"))]
        let _controller = &mut controller;
    }
}

#[cfg(feature = "i18n")]
fn next_revision(publication: &RwLock<Publication>) -> Result<u64, ServiceError> {
    publication
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .revision
        .checked_add(1)
        .ok_or_else(|| ServiceError::Invalid("catalog revision capacity exhausted".to_owned()))
}

#[cfg(feature = "i18n")]
fn parse_records(records: Vec<StoredPack>) -> Result<Vec<Arc<LanguagePack>>, ServiceError> {
    let schema = MessageSchema::builtin();
    records
        .into_iter()
        .map(|record| {
            let pack = LanguagePack::parse(&record.bytes, &schema).map_err(|error| ServiceError::Invalid(error.to_string()))?;
            let manifest = pack.manifest();
            if manifest.id != record.id
                || manifest.locale != record.locale
                || manifest.version.to_string() != record.version
                || pack.digest() != record.digest
            {
                return Err(ServiceError::Invalid("stored language-pack metadata or checksum does not match its archive".to_owned()));
            }
            Ok(Arc::new(pack))
        })
        .collect()
}

#[cfg(feature = "i18n")]
fn build_catalog(packs: Vec<Arc<LanguagePack>>, revision: u64) -> Result<LiveCatalog, ServiceError> {
    let schema = MessageSchema::builtin();
    let english: Arc<dyn Messages> = Arc::new(English::new(Arc::clone(&schema)));
    let catalogs = PackSet::new(packs.clone(), schema, english, revision).map_err(|error| ServiceError::Invalid(error.to_string()))?;
    Ok(LiveCatalog { catalogs, packs, revision })
}

#[cfg(feature = "i18n")]
async fn prepare_catalog(packs: Vec<Arc<LanguagePack>>, revision: u64) -> Result<Arc<LiveCatalog>, ServiceError> {
    tokio::task::spawn_blocking(move || build_catalog(packs, revision)).await.map_err(ServiceError::internal)?.map(Arc::new)
}

/// A new host must never reuse the identity of a browser-cached catalog from another
/// process, including English-only publications. Keep the high bit clear for revision
/// increments and the composition root's combined plugin revision.
fn initial_revision() -> Result<u64, ServiceError> {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).map_err(|error| ServiceError::internal(std::io::Error::other(error.to_string())))?;
    Ok((u64::from_le_bytes(bytes) & (u64::MAX >> 1)).max(1))
}
