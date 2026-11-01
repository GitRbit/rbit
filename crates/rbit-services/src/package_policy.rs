//! Host-owned, metadata-only policy administration. No worker or broker authority
//! is created here. Owned finite tasks settle possible commits after waiter loss;
//! shutdown closes admission and drains them before the database is closed.

use std::{
    collections::BTreeSet,
    future::Future,
    sync::{Arc, Mutex},
};

use rbit_core::{Timestamp, User};
use rbit_plugin_api::{
    InstalledPackageRef, PackagePolicyRenewalRequest, PackageReviewRequest, PackageSelectionClearRequest, PackageSelectionRequest, PluginId,
};
use rbit_services_base::{ServiceContext, ServiceError};
use rbit_storage::{
    StorageError,
    sqlite::{
        plugin_installations::OperationId,
        plugin_policy::{
            ApplyOutcome, PluginPolicyStore, PolicyGate, PolicyPackageSnapshot, PolicyReceipt, PolicyRenewalOutcome, PolicyRenewalReceipt,
            PolicySnapshot,
        },
    },
};
use tokio::sync::oneshot;
use tokio_util::task::{TaskTracker, task_tracker::TaskTrackerToken};

const MAX_OPERATIONS: usize = 2;

/// Explicitly unavailable until an adapter enforces review, sandbox and lifecycle
/// policy. An Open administrative gate or an approval is never a runnable handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageExecutionAvailability {
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagePolicyInspection {
    pub policy: PolicySnapshot,
    pub execution: PackageExecutionAvailability,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageReviewInspection {
    pub policy: PolicyPackageSnapshot,
    pub execution: PackageExecutionAvailability,
}

#[derive(Debug, Clone)]
pub struct PackagePolicyService {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    store: PluginPolicyStore,
    admission: Mutex<Admission>,
    tasks: TaskTracker,
}

#[derive(Debug, Default)]
struct Admission {
    closed: bool,
    active: BTreeSet<String>,
    maintenance: bool,
}

struct OwnedOperation {
    inner: Arc<Inner>,
    key: String,
    maintenance: bool,
    _token: TaskTrackerToken,
}
impl Drop for OwnedOperation {
    fn drop(&mut self) {
        let mut admission = self.inner.admission.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        admission.active.remove(&self.key);
        if self.maintenance {
            admission.maintenance = false;
        }
    }
}

impl PackagePolicyService {
    /// Construction performs no query, random generation, filesystem work or spawn.
    pub(crate) fn new(context: &ServiceContext) -> Self {
        Self {
            inner: Arc::new(Inner {
                store: context.database.plugin_policy(),
                admission: Mutex::new(Admission::default()),
                tasks: TaskTracker::new(),
            }),
        }
    }

    /// Explicit bootstrap only; ordinary inspection never initializes policy.
    pub async fn prepare(&self, actor: &User) -> Result<PolicyGate, ServiceError> {
        administrator(actor)?;
        let store = self.inner.store.clone();
        let actor = actor.id();
        self.own("prepare".to_owned(), async move { store.prepare(actor, Timestamp::now()).await }).await
    }

    /// Explicit sealed-epoch renewal; never automatic and exclusive with admitted mutations.
    pub async fn renew(
        &self,
        actor: &User,
        id: OperationId,
        request: PackagePolicyRenewalRequest,
    ) -> Result<PolicyRenewalOutcome, ServiceError> {
        administrator(actor)?;
        request.validate().map_err(|_| ServiceError::Invalid("invalid package policy renewal request".to_owned()))?;
        let key = format!("renewal:{}", id.as_str());
        let store = self.inner.store.clone();
        let actor = actor.id();
        self.own_with_admission(key, async move { store.renew(actor, &id, &request, Timestamp::now()).await }, true).await
    }
    pub async fn renewal_operation(&self, actor: &User, id: &OperationId) -> Result<Option<PolicyRenewalReceipt>, ServiceError> {
        self.check_read(actor)?;
        self.inner.store.renewal_operation(actor.id(), id).await.map_err(policy_error)
    }

    pub async fn gate(&self, actor: &User) -> Result<PolicyGate, ServiceError> {
        self.check_read(actor)?;
        self.inner.store.gate(actor.id()).await.map_err(policy_error)
    }

    pub async fn inspect(&self, actor: &User, id: &PluginId) -> Result<PackagePolicyInspection, ServiceError> {
        self.check_read(actor)?;
        let policy = self.inner.store.inspect(actor.id(), id).await.map_err(policy_error)?;
        Ok(PackagePolicyInspection { policy, execution: PackageExecutionAvailability::Unavailable })
    }

    pub async fn inspect_package(&self, actor: &User, package: &InstalledPackageRef) -> Result<PackageReviewInspection, ServiceError> {
        self.check_read(actor)?;
        let policy = self.inner.store.inspect_package(actor.id(), package).await.map_err(policy_error)?;
        Ok(PackageReviewInspection { policy, execution: PackageExecutionAvailability::Unavailable })
    }

    pub async fn operation(&self, actor: &User, id: &OperationId) -> Result<Option<PolicyReceipt>, ServiceError> {
        self.check_read(actor)?;
        self.inner.store.operation(actor.id(), id).await.map_err(policy_error)
    }

    pub async fn review(&self, actor: &User, id: OperationId, request: PackageReviewRequest) -> Result<ApplyOutcome, ServiceError> {
        administrator(actor)?;
        request.validate().map_err(|_| ServiceError::Invalid("invalid package review request".to_owned()))?;
        let key = format!("operation:{}", id.as_str());
        let store = self.inner.store.clone();
        let actor = actor.id();
        self.own(key, async move { store.apply_review(actor, &id, &request, Timestamp::now()).await }).await
    }

    pub async fn select(&self, actor: &User, id: OperationId, request: PackageSelectionRequest) -> Result<ApplyOutcome, ServiceError> {
        administrator(actor)?;
        request.validate().map_err(|_| ServiceError::Invalid("invalid package selection request".to_owned()))?;
        let key = format!("operation:{}", id.as_str());
        let store = self.inner.store.clone();
        let actor = actor.id();
        self.own(key, async move { store.apply_selection(actor, &id, &request, Timestamp::now()).await }).await
    }

    pub async fn clear(&self, actor: &User, id: OperationId, request: PackageSelectionClearRequest) -> Result<ApplyOutcome, ServiceError> {
        administrator(actor)?;
        request.validate().map_err(|_| ServiceError::Invalid("invalid package clearing request".to_owned()))?;
        let key = format!("operation:{}", id.as_str());
        let store = self.inner.store.clone();
        let actor = actor.id();
        self.own(key, async move { store.apply_clear(actor, &id, &request, Timestamp::now()).await }).await
    }

    async fn own<T: Send + 'static>(
        &self,
        key: String,
        operation: impl Future<Output = Result<T, StorageError>> + Send + 'static,
    ) -> Result<T, ServiceError> {
        self.own_with_admission(key, operation, false).await
    }

    async fn own_with_admission<T: Send + 'static>(
        &self,
        key: String,
        operation: impl Future<Output = Result<T, StorageError>> + Send + 'static,
        maintenance: bool,
    ) -> Result<T, ServiceError> {
        let owned = {
            let mut admission = self.inner.admission.lock().map_err(|_| ServiceError::AdmissionUnavailable {
                scope: "package policy",
                reason: rbit_storage::AdmissionReason::CoordinatorUnavailable,
            })?;
            if admission.closed {
                return Err(ServiceError::AdmissionUnavailable { scope: "package policy", reason: rbit_storage::AdmissionReason::Stopped });
            }
            if admission.active.contains(&key) {
                return Err(ServiceError::Conflict("package policy operation already active"));
            }
            if admission.maintenance || (maintenance && !admission.active.is_empty()) {
                return Err(ServiceError::AdmissionUnavailable { scope: "package policy", reason: rbit_storage::AdmissionReason::Busy });
            }
            if admission.active.len() >= MAX_OPERATIONS {
                return Err(ServiceError::AdmissionUnavailable {
                    scope: "package policy",
                    reason: rbit_storage::AdmissionReason::Capacity,
                });
            }
            let token = self.inner.tasks.token();
            admission.active.insert(key.clone());
            admission.maintenance = maintenance;
            OwnedOperation { inner: Arc::clone(&self.inner), key, maintenance, _token: token }
        };
        let (sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let result = operation.await.map_err(policy_error);
            // Retain tracking through COMMIT and release admission before a caller
            // can retry. Dropping the waiter never aborts this owned transaction.
            drop(owned);
            let _ = sender.send(result);
        });
        receiver.await.map_err(|_| ServiceError::OutcomeUnavailable { scope: "package policy" })?
    }

    fn check_read(&self, actor: &User) -> Result<(), ServiceError> {
        administrator(actor)?;
        let admission = self.inner.admission.lock().map_err(|_| ServiceError::AdmissionUnavailable {
            scope: "package policy",
            reason: rbit_storage::AdmissionReason::CoordinatorUnavailable,
        })?;
        if admission.closed {
            Err(ServiceError::AdmissionUnavailable { scope: "package policy", reason: rbit_storage::AdmissionReason::Stopped })
        } else {
            Ok(())
        }
    }

    pub(crate) fn prepare_shutdown(&self) {
        self.inner.admission.lock().unwrap_or_else(std::sync::PoisonError::into_inner).closed = true;
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

fn policy_error(error: StorageError) -> ServiceError {
    match error {
        StorageError::Conflict("active administrator required") => ServiceError::Forbidden,
        StorageError::Conflict("empty host capability catalog") => {
            ServiceError::Invalid("host capability catalog has no grants".to_owned())
        }
        StorageError::Conflict("package review declaration mismatch") => {
            ServiceError::Invalid("review does not match the installed package declarations".to_owned())
        }
        other => other.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Services;
    use rbit_core::Config;
    use rbit_plugin_api::{
        ExpectedPolicyRevision, PackageManifest, PackageReviewIntent, PolicyGrantRequest, PolicyGrantRequests, PolicyGrantScope,
    };
    use rbit_storage::{
        Database, KvStore, NewUser,
        sqlite::{
            plugin_installations::InstallationDescriptor,
            plugin_policy::{PolicyCommand, PolicyGateState},
        },
    };
    use std::{path::PathBuf, time::Duration};

    struct Fixture {
        services: Services,
        database: Database,
        admin: User,
        reader: User,
        path: PathBuf,
        _kv: KvStore,
        _directory: tempfile::TempDir,
    }
    impl Fixture {
        async fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let mut config = Config::default();
            config.storage.data_dir = directory.path().join("data");
            config.actions.enabled = false;
            config.i18n.enabled = false;
            config.updates.check_for_updates = false;
            std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
            let path = config.storage.sqlite_path();
            let database = Database::open(&path).await.unwrap();
            let kv = database.open_kv(&config.storage.kv_path()).unwrap();
            let mut users = Vec::new();
            for (name, is_admin) in [("policy-admin", true), ("policy-reader", false)] {
                users.push(
                    database
                        .accounts()
                        .create_user(NewUser {
                            name: name.parse().unwrap(),
                            email: format!("{name}@example.com").parse().unwrap(),
                            display_name: None,
                            password_hash: "unused".to_owned(),
                            is_admin,
                        })
                        .await
                        .unwrap(),
                );
            }
            let reader = users.pop().unwrap();
            let admin = users.pop().unwrap();
            let services = Services::new(Arc::new(config), &database, &kv).await.unwrap();
            Self { services, database, admin, reader, path, _kv: kv, _directory: directory }
        }
        async fn close(&self) {
            self.services.shutdown().await;
            self.database.close().await;
        }
        async fn package(&self) -> InstalledPackageRef {
            let manifest: PackageManifest = serde_json::from_value(serde_json::json!({
                "schema_version":1,
                "plugin":{"id":"example.independent","version":"1.0.0+private.build","host_api":"^1","capabilities":["repository.read"]},
                "kind":{"type":"native","target":"x86_64-unknown-linux-gnu","entrypoint":"greeter"},
                "files":[{"path":"greeter","size":1,"sha256":"a".repeat(64)}],
                "budgets":{"memory_bytes":1024,"concurrent_calls":1,"call_timeout_ms":1000,"storage_bytes":0,"event_queue_capacity":0}
            }))
            .unwrap();
            // Trusted-host installation assertion fixture; no runtime materialization.
            let descriptor = InstallationDescriptor::new(manifest, "b".repeat(64).parse().unwrap(), 4096).unwrap();
            let token = OperationId::new("policy-install").unwrap();
            let store = self.database.plugin_installations();
            store.begin(&token, &descriptor, Some(self.admin.id()), Timestamp::now()).await.unwrap();
            store.complete(&token, 0, Timestamp::now()).await.unwrap();
            InstalledPackageRef::new(
                descriptor.manifest().plugin.id.clone(),
                descriptor.manifest().plugin.version.clone(),
                descriptor.digest().clone(),
            )
            .unwrap()
        }
    }

    #[tokio::test]
    async fn construction_is_lazy_and_stale_or_forged_admin_cannot_initialize() {
        let fixture = Fixture::new().await;
        let (service, work) = rbit_storage::count_work(async { PackagePolicyService::new(fixture.services.context()) }).await;
        assert_eq!(work, rbit_storage::Work::default());
        assert_eq!(service.inner.tasks.len(), 0);
        assert!(service.inner.admission.lock().unwrap().active.is_empty());
        assert_eq!(service.gate(&fixture.admin).await.unwrap().state, PolicyGateState::Uninitialized);
        assert!(matches!(service.prepare(&fixture.reader).await, Err(ServiceError::Forbidden)));
        let mut forged = fixture.reader.clone();
        forged.is_admin = true;
        assert!(matches!(service.prepare(&forged).await, Err(ServiceError::Forbidden)));
        assert_eq!(service.gate(&fixture.admin).await.unwrap().state, PolicyGateState::Uninitialized);
        assert!(!fixture.path.parent().unwrap().join("plugins").exists());
        service.shutdown().await;
        assert!(matches!(
            service.prepare(&fixture.admin).await,
            Err(ServiceError::AdmissionUnavailable { scope: "package policy", reason: rbit_storage::AdmissionReason::Stopped })
        ));
        fixture.close().await;
    }

    #[tokio::test]
    async fn independent_native_review_and_selection_remain_unavailable_without_enabled_intent() {
        let fixture = Fixture::new().await;
        let package = fixture.package().await;
        let service = &fixture.services.package_policy;
        let presentation_revision = fixture.services.plugin_revision();
        let gate = service.prepare(&fixture.admin).await.unwrap();
        let proposal = service.inspect_package(&fixture.admin, &package).await.unwrap();
        assert_eq!(proposal.execution, PackageExecutionAvailability::Unavailable);
        assert!(proposal.policy.review.is_none());
        let request = PackageReviewRequest::new(
            package.clone(),
            ExpectedPolicyRevision::Absent {},
            gate.fingerprint.unwrap(),
            PackageReviewIntent::NativeFullServiceUserTrust {},
            PolicyGrantRequests::new(Vec::new()).unwrap(),
        )
        .unwrap();
        let mut wrong_kind = request.clone();
        wrong_kind.intent = PackageReviewIntent::DataOnly {};
        assert!(matches!(
            service.review(&fixture.admin, OperationId::new("wrong-kind").unwrap(), wrong_kind).await,
            Err(ServiceError::Invalid(_))
        ));
        let mut unsupported_grant = request.clone();
        unsupported_grant.grants = PolicyGrantRequests::new(vec![PolicyGrantRequest {
            capability: "repository.read".parse().unwrap(),
            scope: PolicyGrantScope::Instance {},
        }])
        .unwrap();
        assert!(matches!(
            service.review(&fixture.admin, OperationId::new("unsupported-grant").unwrap(), unsupported_grant).await,
            Err(ServiceError::Invalid(_))
        ));
        assert!(service.inspect_package(&fixture.admin, &package).await.unwrap().policy.review.is_none());
        let token = OperationId::new("independent-review").unwrap();
        let ApplyOutcome::Applied { receipt, .. } = service.review(&fixture.admin, token.clone(), request.clone()).await.unwrap() else {
            panic!("review must apply")
        };
        let selection = PackageSelectionRequest::new(
            package.clone(),
            ExpectedPolicyRevision::Absent {},
            receipt.result_revision,
            request.expected_host_policy.clone(),
        );
        assert!(matches!(
            service.select(&fixture.admin, OperationId::new("independent-select").unwrap(), selection).await.unwrap(),
            ApplyOutcome::Applied { .. }
        ));
        let status = service.inspect(&fixture.admin, package.plugin_id()).await.unwrap();
        assert_eq!(status.execution, PackageExecutionAvailability::Unavailable);
        assert_eq!(status.policy.selection.as_ref().unwrap().package.as_ref(), Some(&package));
        assert_eq!(fixture.database.plugin_settings().desired_enabled(package.plugin_id().as_str()).await.unwrap(), None);
        assert!(matches!(service.review(&fixture.admin, token, request).await.unwrap(), ApplyOutcome::HistoricalReceipt { .. }));
        assert!(matches!(
            service.operation(&fixture.admin, &OperationId::new("independent-review").unwrap()).await.unwrap().unwrap().command,
            PolicyCommand::Review(_)
        ));
        assert_eq!(service.inner.tasks.len(), 0);
        assert_eq!(fixture.services.plugin_revision(), presentation_revision);
        assert_eq!(fixture.services.actions_background_tasks(), 0);
        assert!(!fixture.path.parent().unwrap().join("plugins").exists());
        fixture.close().await;
    }

    async fn wait_active(service: &PackagePolicyService, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if service.inner.admission.lock().unwrap().active.len() == count {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn dropped_waiters_retain_bounded_owners_and_shutdown_drains_real_sqlite_commits() {
        use sqlx::{ConnectOptions, Connection};
        let fixture = Fixture::new().await;
        let package = fixture.package().await;
        let service = fixture.services.package_policy.clone();
        let gate = service.prepare(&fixture.admin).await.unwrap();
        let request = PackageReviewRequest::new(
            package,
            ExpectedPolicyRevision::Absent {},
            gate.fingerprint.unwrap(),
            PackageReviewIntent::NativeFullServiceUserTrust {},
            PolicyGrantRequests::new(Vec::new()).unwrap(),
        )
        .unwrap();
        let mut blocker = sqlx::sqlite::SqliteConnectOptions::new().filename(&fixture.path).connect().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut blocker).await.unwrap();
        let mut waiters = Vec::new();
        for token in ["owned-one", "owned-two"] {
            let service = service.clone();
            let actor = fixture.admin.clone();
            let request = request.clone();
            waiters.push(tokio::spawn(async move { service.review(&actor, OperationId::new(token).unwrap(), request).await }));
        }
        wait_active(&service, 2).await;
        assert!(matches!(
            service.review(&fixture.admin, OperationId::new("owned-three").unwrap(), request.clone()).await,
            Err(ServiceError::AdmissionUnavailable { scope: "package policy", reason: rbit_storage::AdmissionReason::Capacity })
        ));
        let maintenance = PackagePolicyRenewalRequest::new(request.expected_host_policy.clone(), "unused-seal").unwrap();
        assert!(matches!(
            service.renew(&fixture.admin, OperationId::new("blocked-maintenance").unwrap(), maintenance).await,
            Err(ServiceError::AdmissionUnavailable { scope: "package policy", reason: rbit_storage::AdmissionReason::Busy })
        ));
        assert!(matches!(
            service.review(&fixture.admin, OperationId::new("owned-one").unwrap(), request).await,
            Err(ServiceError::Conflict("package policy operation already active"))
        ));
        for waiter in waiters {
            waiter.abort();
            assert!(waiter.await.unwrap_err().is_cancelled());
        }
        assert_eq!(service.inner.admission.lock().unwrap().active.len(), 2);
        service.prepare_shutdown();
        let drain = tokio::spawn({
            let service = service.clone();
            async move { service.shutdown().await }
        });
        tokio::task::yield_now().await;
        assert!(!drain.is_finished());
        sqlx::query("COMMIT").execute(&mut blocker).await.unwrap();
        blocker.close().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), drain).await.unwrap().unwrap();
        assert_eq!(service.inner.tasks.len(), 0);
        assert!(service.inner.admission.lock().unwrap().active.is_empty());
        let store = fixture.database.plugin_policy();
        // Both owners used the same absent CAS; exactly one commits a review.
        let first = store.operation(fixture.admin.id(), &OperationId::new("owned-one").unwrap()).await.unwrap();
        let second = store.operation(fixture.admin.id(), &OperationId::new("owned-two").unwrap()).await.unwrap();
        assert_ne!(first.is_some(), second.is_some());
        fixture.close().await;
    }

    // Real capacity fixture in one SQLite transaction; it does not execute an archive.
    async fn sealed_epoch(fixture: &Fixture, request: &PackageReviewRequest) -> PolicyGate {
        use sqlx::{ConnectOptions as _, Connection as _};
        let service = &fixture.services.package_policy;
        assert!(matches!(
            service.review(&fixture.admin, OperationId::new("seed").unwrap(), request.clone()).await.unwrap(),
            ApplyOutcome::Applied { .. }
        ));
        let mut connection = sqlx::sqlite::SqliteConnectOptions::new().filename(&fixture.path).foreign_keys(true).connect().await.unwrap();
        let mut tx = connection.begin().await.unwrap();
        sqlx::query("WITH RECURSIVE n(v) AS (VALUES(2) UNION ALL SELECT v+1 FROM n WHERE v<4096) INSERT INTO plugin_review_history(digest,revision,plugin_id,version,intent,request,catalog_version,epoch,fingerprint,actor,created_at) SELECT digest,v,plugin_id,version,intent,json_set(request,'$.command.request.expected_review',json_object('kind','exact','revision',CAST(v-1 AS TEXT))),catalog_version,epoch,fingerprint,actor,created_at FROM plugin_review_history,n WHERE revision=1").execute(&mut *tx).await.unwrap();
        sqlx::query("INSERT INTO plugin_policy_receipts(id,kind,request,actor,plugin_id,version,digest,result_revision,epoch,fingerprint,created_at) SELECT 'bulk-service-'||revision,0,request,actor,plugin_id,version,digest,revision,epoch,fingerprint,created_at FROM plugin_review_history WHERE revision>1").execute(&mut *tx).await.unwrap();
        sqlx::query("UPDATE plugin_review_heads SET revision=4096").execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        connection.close().await.unwrap();
        let mut next = request.clone();
        next.expected_review = ExpectedPolicyRevision::Exact { revision: rbit_plugin_api::PolicyRevision::new(4096).unwrap() };
        next.intent = PackageReviewIntent::Deny {};
        let ApplyOutcome::CapacitySealed { gate } =
            service.review(&fixture.admin, OperationId::new("maintenance-seal").unwrap(), next).await.unwrap()
        else {
            panic!("expected real capacity seal")
        };
        gate
    }

    #[tokio::test]
    async fn exclusive_renewal_owner_survives_waiter_loss_and_shutdown() {
        use sqlx::{ConnectOptions as _, Connection as _};
        let fixture = Fixture::new().await;
        let package = fixture.package().await;
        let service = fixture.services.package_policy.clone();
        let gate = service.prepare(&fixture.admin).await.unwrap();
        let review = PackageReviewRequest::new(
            package.clone(),
            ExpectedPolicyRevision::Absent {},
            gate.fingerprint.unwrap(),
            PackageReviewIntent::NativeFullServiceUserTrust {},
            PolicyGrantRequests::new(Vec::new()).unwrap(),
        )
        .unwrap();
        let sealed = sealed_epoch(&fixture, &review).await;
        let request =
            PackagePolicyRenewalRequest::new(sealed.fingerprint.clone().unwrap(), sealed.seal.as_ref().unwrap().operation.as_str())
                .unwrap();
        let key = OperationId::new("owned-renewal").unwrap();
        let revision = fixture.services.plugin_revision();
        let mut blocker = sqlx::sqlite::SqliteConnectOptions::new().filename(&fixture.path).connect().await.unwrap();
        sqlx::query("BEGIN IMMEDIATE").execute(&mut blocker).await.unwrap();
        let waiter = tokio::spawn({
            let service = service.clone();
            let actor = fixture.admin.clone();
            let request = request.clone();
            let key = key.clone();
            async move { service.renew(&actor, key, request).await }
        });
        wait_active(&service, 1).await;
        assert!(service.inner.admission.lock().unwrap().maintenance);
        assert!(matches!(
            service.review(&fixture.admin, OperationId::new("blocked-write").unwrap(), review).await,
            Err(ServiceError::AdmissionUnavailable { scope: "package policy", reason: rbit_storage::AdmissionReason::Busy })
        ));
        assert!(matches!(
            service.renew(&fixture.admin, OperationId::new("blocked-renewal").unwrap(), request.clone()).await,
            Err(ServiceError::AdmissionUnavailable { scope: "package policy", reason: rbit_storage::AdmissionReason::Busy })
        ));
        waiter.abort();
        assert!(waiter.await.unwrap_err().is_cancelled());
        assert!(service.inner.admission.lock().unwrap().maintenance);
        service.prepare_shutdown();
        let drain = tokio::spawn({
            let service = service.clone();
            async move { service.shutdown().await }
        });
        tokio::task::yield_now().await;
        assert!(!drain.is_finished());
        sqlx::query("COMMIT").execute(&mut blocker).await.unwrap();
        blocker.close().await.unwrap();
        tokio::time::timeout(Duration::from_secs(30), drain).await.unwrap().unwrap();
        assert!(!service.inner.admission.lock().unwrap().maintenance);
        assert!(service.inner.admission.lock().unwrap().active.is_empty());
        let store = fixture.database.plugin_policy();
        let receipt = store.renewal_operation(fixture.admin.id(), &key).await.unwrap().unwrap();
        assert_eq!(Some(receipt.source_epoch), sealed.epoch);
        let current = store.gate(fixture.admin.id()).await.unwrap();
        assert_eq!(current.state, PolicyGateState::Open);
        assert_ne!(current.fingerprint, sealed.fingerprint);
        assert!(store.inspect(fixture.admin.id(), package.plugin_id()).await.unwrap().selection.is_none());
        assert!(store.inspect_package(fixture.admin.id(), &package).await.unwrap().review.is_none());
        assert!(matches!(
            store.renew(fixture.admin.id(), &key, &request, Timestamp::now()).await.unwrap(),
            PolicyRenewalOutcome::HistoricalReceipt { .. }
        ));
        assert_eq!(fixture.services.plugin_revision(), revision);
        assert_eq!(fixture.services.actions_background_tasks(), 0);
        assert!(!fixture.path.parent().unwrap().join("plugins").exists());
        fixture.close().await;
    }
}
