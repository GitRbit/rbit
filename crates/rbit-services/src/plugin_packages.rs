//! Host package installation coordination, without activation or authority grants.
//!
//! The host must durably provision the fixed private package root and protect its
//! ancestors/ACL before installation. Construction performs no filesystem work or
//! database query. Owned workers retain admission until blocking work and durable
//! journal settlement finish; cancellation cannot interrupt an OS read or fsync.

use std::{
    collections::BTreeMap,
    io::Read,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rbit_core::{Timestamp, User};
use rbit_plugin_packages::{Durability, MAX_ARCHIVE_WIRE_BYTES, PackageReceipt, PackageStore, PreparedPackage, StageControl, StoreError};
use rbit_services_base::{
    AuditService, ServiceContext, ServiceError,
    audit::{AuditAction, AuditEntry},
};
use rbit_storage::{
    Database, StorageError,
    sqlite::plugin_installations::{
        Installation, InstallationDescriptor, InstallationOperation, OperationError, OperationId, OperationStage,
    },
};
use tokio::sync::oneshot;
use tokio_util::task::{TaskTracker, task_tracker::TaskTrackerToken};

const MAX_OPERATIONS: usize = 2;
const MAX_DURATION: Duration = Duration::from_mins(1);

#[derive(Debug)]
pub enum PackageInstallOutcome {
    /// Recorded without changing enabled intent or approving execution rights.
    Installed(Installation),
    /// Matching bytes exist, but namespace durability has not been established.
    AwaitingDurability { operation: InstallationOperation, receipt: PackageReceipt },
}

#[derive(Debug, Clone)]
pub struct PluginPackageService {
    inner: Arc<Inner>,
}
#[derive(Debug)]
struct Inner {
    root: PathBuf,
    database: Database,
    audit: AuditService,
    admission: Mutex<Admission>,
    tasks: TaskTracker,
}
#[derive(Debug, Default)]
struct Admission {
    closed: bool,
    active: BTreeMap<String, StageControl>,
}
struct OwnedOperation {
    inner: Arc<Inner>,
    id: String,
    _token: TaskTrackerToken,
}
impl Drop for OwnedOperation {
    fn drop(&mut self) {
        self.inner.admission.lock().unwrap_or_else(std::sync::PoisonError::into_inner).active.remove(&self.id);
    }
}
struct WaiterCancellation(StageControl);
impl Drop for WaiterCancellation {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl PluginPackageService {
    pub(crate) fn new(context: &ServiceContext) -> Self {
        Self {
            inner: Arc::new(Inner {
                root: context.config.storage.data_dir.join("plugins").join("packages"),
                database: context.database.clone(),
                audit: AuditService::from_store(context.database.audit()),
                admission: Mutex::new(Admission::default()),
                tasks: TaskTracker::new(),
            }),
        }
    }

    /// Inspect only through authenticated host administration. No package root is
    /// opened and no package-supplied metadata is trusted by this query.
    pub async fn operation(&self, actor: &User, id: &OperationId) -> Result<Option<InstallationOperation>, ServiceError> {
        administrator(actor)?;
        self.check_running()?;
        self.authorize_current(actor).await?;
        Ok(self.inner.database.plugin_installations().operation(id).await?)
    }

    /// The reader must have host-enforced transport deadlines: cancellation is
    /// cooperative and cannot interrupt a blocking Read/fsync. The operation token
    /// is chosen by authenticated host administration/CLI before upload, never by
    /// package/plugin metadata. A dropped HTTP waiter cancels, but never aborts its owner.
    pub async fn install(
        &self,
        actor: &User,
        id: OperationId,
        reader: impl Read + Send + 'static,
        deadline: Instant,
        wire_quota: u64,
    ) -> Result<PackageInstallOutcome, ServiceError> {
        administrator(actor)?;
        self.check_running()?;
        self.authorize_current(actor).await?;
        let now = Instant::now();
        if deadline <= now || wire_quota == 0 || wire_quota > MAX_ARCHIVE_WIRE_BYTES {
            return Err(ServiceError::Invalid("invalid package operation deadline or wire quota".to_owned()));
        }
        let control = StageControl::new(deadline.min(now + MAX_DURATION), wire_quota).map_err(ServiceError::internal)?;
        let owned = {
            let mut admission = self.inner.admission.lock().map_err(|_| ServiceError::Conflict("package coordinator unavailable"))?;
            if admission.closed {
                return Err(ServiceError::Conflict("server shutdown"));
            }
            if admission.active.contains_key(id.as_str()) {
                return Err(ServiceError::Conflict("package operation already active"));
            }
            if admission.active.len() >= MAX_OPERATIONS {
                return Err(ServiceError::Conflict("package operation admission"));
            }
            let token = self.inner.tasks.token();
            admission.active.insert(id.as_str().to_owned(), control.clone());
            OwnedOperation { inner: Arc::clone(&self.inner), id: id.as_str().to_owned(), _token: token }
        };
        let waiter = WaiterCancellation(control.clone());
        let (sender, receiver) = oneshot::channel();
        let service = self.clone();
        let actor = actor.clone();
        tokio::spawn(async move {
            let result = service.run(&actor, &id, reader, control).await;
            // Release admission before notifying a waiter that could immediately
            // retry. The task token remains owned throughout all real work.
            drop(owned);
            let _ = sender.send(result);
        });
        let result = receiver.await.map_err(ServiceError::internal)?;
        drop(waiter);
        result
    }

    async fn run(
        &self,
        actor: &User,
        id: &OperationId,
        reader: impl Read + Send + 'static,
        control: StageControl,
    ) -> Result<PackageInstallOutcome, ServiceError> {
        let (prepared, descriptor) = self.prepare(actor, reader, control).await?;
        let ledger = self.inner.database.plugin_installations();
        let mut operation = match ledger.begin(id, &descriptor, Some(actor.id()), Timestamp::now()).await {
            Ok(operation) => operation,
            Err(error) => {
                discard_prepared(prepared).await;
                return Err(error.into());
            }
        };
        if matches!(operation.stage, OperationStage::Error(_)) {
            operation = match ledger.transition(id, operation.revision, OperationStage::Pending, Timestamp::now()).await {
                Ok(operation) => operation,
                Err(error) => {
                    discard_prepared(prepared).await;
                    return Err(error.into());
                }
            };
        }
        // Journal acquisition may itself wait for another writer. Recheck at
        // the publication boundary after that wait, then finish any commit once
        // publication succeeds rather than pretending visible bytes rolled back.
        if let Err(error) = self.authorize_current(actor).await {
            self.record_failure(&operation, OperationError::Interrupted).await;
            discard_prepared(prepared).await;
            return Err(error);
        }
        let published = match tokio::task::spawn_blocking(move || prepared.publish()).await {
            Ok(published) => published,
            Err(error) => {
                self.record_failure(&operation, OperationError::Publication).await;
                return Err(ServiceError::internal(error));
            }
        };
        let receipt = match published {
            Ok(receipt) => receipt,
            Err(failure) => {
                let code = match failure.error() {
                    StoreError::Cancelled | StoreError::Deadline => OperationError::Interrupted,
                    StoreError::Sync | StoreError::DirectorySync => OperationError::Durability,
                    _ => OperationError::Publication,
                };
                // The original typed failure keeps its receipt/visibility/cleanup
                // state. Never delete a published object or mask it with DB errors.
                self.record_failure(&operation, code).await;
                return Err(ServiceError::internal(failure));
            }
        };
        if receipt.durability() == Durability::Unconfirmed {
            let prior = ledger.installation(&descriptor.manifest().plugin.id, &descriptor.manifest().plugin.version).await?;
            if !prior.is_some_and(|installed| installed.descriptor == descriptor) {
                let awaiting = ledger.transition(id, operation.revision, OperationStage::AwaitingDurability, Timestamp::now()).await?;
                return Ok(PackageInstallOutcome::AwaitingDurability { operation: awaiting, receipt });
            }
            // Only an exact prior durable host ledger assertion can reconcile
            // this Windows existing-object receipt. Existence alone never does.
        }
        let installed = match ledger.complete(id, operation.revision, Timestamp::now()).await {
            Ok(installed) => installed,
            Err(error @ StorageError::Conflict(_)) => {
                // One bounded requery resolves an already completed identical
                // operation; never retry commits blindly or overwrite its state.
                let Some(current) = ledger.operation(id).await? else {
                    return Err(ServiceError::Conflict("package operation revision"));
                };
                if current.stage != OperationStage::Complete || current.descriptor != descriptor || current.actor != Some(actor.id()) {
                    self.record_failure(&operation, OperationError::Conflict).await;
                    return Err(error.into());
                }
                ledger
                    .installation(&descriptor.manifest().plugin.id, &descriptor.manifest().plugin.version)
                    .await?
                    .filter(|installed| installed.descriptor == descriptor)
                    .ok_or(ServiceError::Conflict("package installation ledger"))?
            }
            Err(error) => {
                // A failed COMMIT is uncertain. Leave the journal queryable,
                // including a completion that might already be durable.
                return Err(error.into());
            }
        };
        self.inner
            .audit
            .record(
                AuditEntry::new(AuditAction::InstanceSettingsChanged)
                    .by(actor)
                    .on(descriptor.manifest().plugin.id.as_str())
                    .detail("package recorded; enabled intent unchanged; no trust or grants approved"),
            )
            .await;
        Ok(PackageInstallOutcome::Installed(installed))
    }

    async fn prepare(
        &self,
        actor: &User,
        reader: impl Read + Send + 'static,
        control: StageControl,
    ) -> Result<(PreparedPackage, InstallationDescriptor), ServiceError> {
        let root = self.inner.root.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            // Provisioning and ACL durability are explicit host preconditions;
            // package operations never create or relax the configured store root.
            let store = PackageStore::open(root).map_err(ServiceError::internal)?;
            store.stage(reader, control).map_err(ServiceError::internal)
        })
        .await
        .map_err(ServiceError::internal)??;
        // Administrator authority can be revoked while an owned reader blocks.
        // Recheck the live account before recording intent or publishing bytes.
        if let Err(error) = self.authorize_current(actor).await {
            discard_prepared(prepared).await;
            return Err(error);
        }
        match InstallationDescriptor::new(prepared.manifest().clone(), prepared.digest().clone(), prepared.wire_bytes()) {
            Ok(descriptor) => Ok((prepared, descriptor)),
            Err(error) => {
                discard_prepared(prepared).await;
                Err(error.into())
            }
        }
    }

    async fn record_failure(&self, operation: &InstallationOperation, error: OperationError) {
        if operation.stage == OperationStage::Complete {
            return;
        }
        let ledger = self.inner.database.plugin_installations();
        if let Err(error) = ledger.transition(&operation.id, operation.revision, OperationStage::Error(error), Timestamp::now()).await {
            tracing::warn!(error = %error, "package operation failure journal update did not complete");
        }
    }

    async fn authorize_current(&self, actor: &User) -> Result<(), ServiceError> {
        let current = self.inner.database.accounts().find_active_user(actor.id()).await?.ok_or(ServiceError::Forbidden)?;
        administrator(&current)
    }

    fn check_running(&self) -> Result<(), ServiceError> {
        let admission = self.inner.admission.lock().map_err(|_| ServiceError::Conflict("package coordinator unavailable"))?;
        if admission.closed { Err(ServiceError::Conflict("server shutdown")) } else { Ok(()) }
    }

    pub(crate) fn prepare_shutdown(&self) {
        let mut admission = self.inner.admission.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        admission.closed = true;
        for control in admission.active.values() {
            control.cancel();
        }
        self.inner.tasks.close();
    }
    pub(crate) async fn shutdown(&self) {
        self.prepare_shutdown();
        self.inner.tasks.wait().await;
    }
}
fn administrator(actor: &User) -> Result<(), ServiceError> {
    if actor.is_admin { Ok(()) } else { Err(ServiceError::Forbidden) }
}

// Keep even failed prepublication cleanup off the async executor. Its owner
// retains admission and tracking until this actual filesystem work finishes.
async fn discard_prepared(prepared: PreparedPackage) {
    match tokio::task::spawn_blocking(move || prepared.discard()).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::warn!(error = %error, "private package staging cleanup incomplete"),
        Err(error) => tracing::warn!(error = %error, "private package staging cleanup worker failed"),
    }
}
