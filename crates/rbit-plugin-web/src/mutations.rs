//! Authenticated POST contracts. Cancellation is cooperative and cannot undo writes
//! already committed by a native callback; use transactional/idempotent domain APIs.
use crate::{CompiledPage, FormAction, FormData, FormSchema, FormValues, PageActor, PageBody, PageError, PageScope, validate_page_path};
use askama::Template;
use rbit_messages::LocaleSnapshot;
use rbit_plugin_api::{Generation, PluginId, UiContributionId, UiTarget};
use rbit_plugin_runtime::{ExtensionLease, ExtensionRegistration, ExtensionSnapshot, OperationLease, PluginScope, ScopedExtensionRegistry};
use std::{
    collections::BTreeSet,
    fmt,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::Instant,
};
use tokio_util::sync::CancellationToken;

pub const MAX_MUTATION_ROUTES: usize = 128;
pub const MAX_CONCURRENT_MUTATIONS: usize = 64;
pub const MAX_MUTATION_DEADLINE: Duration = Duration::from_secs(1);
#[derive(Debug, thiserror::Error)]
pub enum MutationError {
    #[error("invalid mutation declaration or context: {0}")]
    Invalid(&'static str),
    #[error("invalid submitted form")]
    InvalidForm,
    #[error("mutation access forbidden")]
    Forbidden,
    #[error("mutation generation unavailable or capacity exhausted")]
    Unavailable,
    #[error("mutation deadline exceeded; committed effects may still exist")]
    Timeout,
    #[error(transparent)]
    Page(#[from] PageError),
    #[error(transparent)]
    Registry(#[from] rbit_plugin_runtime::ExtensionRegistryError),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationAudience {
    SignedIn,
    SiteAdmin,
    RepositoryWriter,
    RepositoryAdmin,
    RepositorySignedIn,
    /// Only the sealed locale-preference callback may use this guest-capable policy.
    LocalePreference,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationInputKind {
    Form,
    Upload { max_bytes: usize },
    LocalePreference,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationDescriptor {
    id: UiContributionId,
    scope: PageScope,
    path: String,
    audience: MutationAudience,
    deadline: Duration,
    form: FormSchema,
    input_kind: MutationInputKind,
    presentation: crate::PagePresentation,
    body_limit: usize,
}
impl MutationDescriptor {
    pub fn new(
        id: &str,
        scope: PageScope,
        path: &str,
        audience: MutationAudience,
        deadline: Duration,
        form: FormSchema,
    ) -> Result<Self, MutationError> {
        if !validate_page_path(path) || deadline < Duration::from_millis(1) || deadline > MAX_MUTATION_DEADLINE {
            return Err(MutationError::Invalid("path or deadline"));
        }
        if !matches!(
            (scope, audience),
            (PageScope::Global, MutationAudience::SignedIn | MutationAudience::SiteAdmin)
                | (
                    PageScope::Repository,
                    MutationAudience::RepositoryWriter | MutationAudience::RepositoryAdmin | MutationAudience::RepositorySignedIn
                )
        ) {
            return Err(MutationError::Invalid("scope and audience mismatch"));
        }
        Ok(Self {
            id: id.parse().map_err(|_| MutationError::Invalid("mutation identifier"))?,
            scope,
            path: path.into(),
            audience,
            deadline,
            form,
            input_kind: MutationInputKind::Form,
            presentation: crate::PagePresentation::Shell,
            body_limit: crate::MAX_PAGE_BODY_BYTES,
        })
    }
    pub fn with_body_limit(mut self, bytes: usize) -> Result<Self, MutationError> {
        if bytes == 0 || bytes > crate::MAX_LARGE_PAGE_BODY_BYTES {
            return Err(MutationError::Invalid("mutation body limit"));
        }
        self.body_limit = bytes;
        Ok(self)
    }
    pub const fn body_limit(&self) -> usize {
        self.body_limit
    }
    #[must_use]
    pub fn with_document(mut self) -> Self {
        self.presentation = crate::PagePresentation::Document;
        self
    }
    pub const fn presentation(&self) -> crate::PagePresentation {
        self.presentation
    }
    pub fn with_upload(mut self, max_bytes: usize) -> Result<Self, MutationError> {
        if self.scope != PageScope::Global
            || self.audience != MutationAudience::SiteAdmin
            || !self.form.fields().is_empty()
            || max_bytes == 0
            || max_bytes > crate::MAX_UPLOAD_BYTES
        {
            return Err(MutationError::Invalid("upload policy or limit"));
        }
        self.input_kind = MutationInputKind::Upload { max_bytes };
        Ok(self)
    }
    pub fn locale_preference(id: &str, path: &str, deadline: Duration) -> Result<Self, MutationError> {
        let form = FormSchema::new(vec![crate::FormField {
            name: "locale".into(),
            kind: crate::FormFieldKind::Text { max_bytes: 64, multiline: false },
            required: false,
        }])?;
        let mut descriptor = Self::new(id, PageScope::Global, path, MutationAudience::SignedIn, deadline, form)?;
        descriptor.audience = MutationAudience::LocalePreference;
        descriptor.input_kind = MutationInputKind::LocalePreference;
        Ok(descriptor)
    }
    pub const fn input_kind(&self) -> MutationInputKind {
        self.input_kind
    }
    pub fn id(&self) -> &str {
        self.id.as_str()
    }
    pub const fn scope(&self) -> PageScope {
        self.scope
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub const fn audience(&self) -> MutationAudience {
        self.audience
    }
    pub const fn deadline(&self) -> Duration {
        self.deadline
    }
    pub fn form(&self) -> &FormSchema {
        &self.form
    }
    pub fn target(&self, owner: &PluginId) -> UiTarget {
        target(owner, self.scope, &self.path)
    }
}
fn target(owner: &PluginId, scope: PageScope, path: &str) -> UiTarget {
    match scope {
        PageScope::Global => format!("/-/plugins/{owner}/{path}"),
        PageScope::Repository => format!("/{{owner}}/{{repo}}/-/plugins/{owner}/{path}"),
    }
    .parse()
    .expect("bounded mutation paths fit internal UI targets")
}
#[derive(Debug, Clone)]
pub struct MutationRepository {
    pub repository_id: i64,
    pub owner: String,
    pub name: String,
    pub can_write: bool,
    pub can_admin: bool,
}
#[derive(Debug, Clone)]
pub struct MutationContext {
    pub actor: PageActor,
    pub repository: Option<MutationRepository>,
    pub locale: LocaleSnapshot,
    pub form: FormAction,
    cancellation: CancellationToken,
    preference_principal: bool,
    forms: crate::FormActions,
}
impl MutationContext {
    pub fn new(actor: PageActor, repository: Option<MutationRepository>, locale: LocaleSnapshot, form: FormAction) -> Self {
        Self {
            actor,
            repository,
            locale,
            forms: crate::FormActions::new(vec![form.clone()]).expect("one valid form"),
            form,
            cancellation: CancellationToken::new(),
            preference_principal: false,
        }
    }
    pub fn forms(&self) -> &crate::FormActions {
        &self.forms
    }
    pub fn set_forms(&mut self, forms: crate::FormActions) {
        self.forms = forms;
    }
    pub fn for_preference(actor: Option<PageActor>, locale: LocaleSnapshot, form: FormAction) -> Self {
        Self {
            actor: actor.unwrap_or(PageActor { account_id: 0, is_admin: false }),
            repository: None,
            locale,
            forms: crate::FormActions::new(vec![form.clone()]).expect("one valid form"),
            form,
            cancellation: CancellationToken::new(),
            preference_principal: true,
        }
    }
    pub fn cancellation_token(&self) -> CancellationToken {
        self.cancellation.child_token()
    }

    fn authorize(&self, descriptor: &MutationDescriptor) -> Result<(), MutationError> {
        let preference =
            descriptor.input_kind == MutationInputKind::LocalePreference && descriptor.audience == MutationAudience::LocalePreference;
        if preference != self.preference_principal {
            return Err(MutationError::Forbidden);
        }
        if self.actor.account_id < 0 || (self.actor.account_id == 0 && (!preference || self.actor.is_admin)) {
            return Err(MutationError::Invalid("actor identifier"));
        }
        if let Some(repo) = &self.repository
            && (repo.repository_id <= 0 || !crate::pages::valid_name(&repo.owner, 39) || !crate::pages::valid_name(&repo.name, 100))
        {
            return Err(MutationError::Invalid("repository context"));
        }
        if (descriptor.scope == PageScope::Global && self.repository.is_some())
            || (descriptor.scope == PageScope::Repository && self.repository.is_none())
        {
            return Err(MutationError::Forbidden);
        }
        let allowed = match descriptor.audience {
            MutationAudience::SignedIn => true,
            MutationAudience::SiteAdmin => self.actor.is_admin,
            MutationAudience::RepositoryWriter => self.repository.as_ref().is_some_and(|r| r.can_write),
            MutationAudience::RepositoryAdmin => self.repository.as_ref().is_some_and(|r| r.can_admin),
            MutationAudience::RepositorySignedIn => self.repository.is_some(),
            MutationAudience::LocalePreference => preference,
        };
        if allowed { Ok(()) } else { Err(MutationError::Forbidden) }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginRedirect {
    path: String,
    query: crate::PageQuery,
}
impl PluginRedirect {
    pub fn new(path: &str) -> Result<Self, MutationError> {
        if !validate_page_path(path) {
            return Err(MutationError::Invalid("redirect path"));
        }
        Ok(Self { path: path.into(), query: crate::PageQuery::default() })
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn with_query(mut self, pairs: &[(&str, &str)]) -> Result<Self, MutationError> {
        self.query = crate::PageQuery::new(pairs.iter().map(|(key, value)| ((*key).into(), (*value).into())).collect())?;
        if self.query.encoded().len() > crate::MAX_QUERY_BYTES {
            return Err(MutationError::Invalid("redirect query size"));
        }
        Ok(self)
    }
    pub fn query_string(&self) -> String {
        self.query.encoded()
    }
    pub fn target(&self, owner: &PluginId, scope: PageScope) -> UiTarget {
        target(owner, scope, &self.path)
    }
}
#[derive(Debug)]
pub enum MutationOutcome<T: Template> {
    Redirect(PluginRedirect),
    Success(CompiledPage<T>),
    Invalid(CompiledPage<T>),
}
#[derive(Debug)]
pub enum MutationResponse {
    Redirect(PluginRedirect),
    Success(PageBody),
    Invalid(PageBody),
    LocalePreference { choice: crate::LocaleChoice, redirect: PluginRedirect },
}
#[derive(Debug)]
pub enum PreferenceOutcome<T: Template> {
    Selected { choice: crate::LocaleChoice, redirect: PluginRedirect },
    Invalid(CompiledPage<T>),
}
enum ValidatedInput {
    Form(FormValues),
    Upload(crate::UploadData),
    LocalePreference(crate::LocaleChoice),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FactoryKind {
    Form,
    Upload,
    LocalePreference,
}

type MutationFuture<'a> = Pin<Box<dyn Future<Output = Result<MutationResponse, MutationError>> + Send + 'a>>;
trait Mutator: Send + Sync {
    fn execute(&self, context: MutationContext, input: ValidatedInput, limit: usize) -> MutationFuture<'_>;
}
struct TemplateMutator<F>(F);
impl<T, F, Fut> Mutator for TemplateMutator<F>
where
    T: Template + Send + 'static,
    F: Fn(MutationContext, FormValues) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<MutationOutcome<T>, MutationError>> + Send + 'static,
{
    fn execute(&self, context: MutationContext, input: ValidatedInput, limit: usize) -> MutationFuture<'_> {
        Box::pin(async move {
            let ValidatedInput::Form(form) = input else {
                return Err(MutationError::Invalid("factory input kind"));
            };
            match (self.0)(context, form).await? {
                MutationOutcome::Redirect(path) => Ok(MutationResponse::Redirect(path)),
                MutationOutcome::Success(page) => Ok(MutationResponse::Success(page.render_with_limit(limit)?)),
                MutationOutcome::Invalid(page) => Ok(MutationResponse::Invalid(page.render_with_limit(limit)?)),
            }
        })
    }
}
#[derive(Clone)]
pub struct MutationRegistration {
    descriptor: MutationDescriptor,
    callback: Arc<dyn Mutator>,
    factory_kind: FactoryKind,
}
impl fmt::Debug for MutationRegistration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MutationRegistration").field("descriptor", &self.descriptor).finish_non_exhaustive()
    }
}
impl MutationRegistration {
    pub fn descriptor(&self) -> &MutationDescriptor {
        &self.descriptor
    }
}
/// The factory returns compiled templates or a namespaced redirect, never raw HTML.
/// ```compile_fail
/// use rbit_plugin_web::{compiled_mutation,MutationDescriptor};
/// fn invalid(descriptor:MutationDescriptor){let _=compiled_mutation(descriptor,|_,_|async{Ok("raw HTML".to_owned())});}
/// ```
pub fn compiled_mutation<T, F, Fut>(descriptor: MutationDescriptor, factory: F) -> MutationRegistration
where
    T: Template + Send + 'static,
    F: Fn(MutationContext, FormValues) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<MutationOutcome<T>, MutationError>> + Send + 'static,
{
    MutationRegistration { descriptor, callback: Arc::new(TemplateMutator(factory)), factory_kind: FactoryKind::Form }
}
#[derive(Debug, Clone)]
struct Metadata {
    descriptor: MutationDescriptor,
    version: u64,
}
#[derive(Debug, Clone)]
pub struct MutationRegistry {
    inner: ScopedExtensionRegistry<MutationRegistration>,
    operations: Arc<Semaphore>,
    versions: Arc<AtomicU64>,
    uploads: Arc<Semaphore>,
    large: Arc<Semaphore>,
}
impl Default for MutationRegistry {
    fn default() -> Self {
        Self::new()
    }
}
impl MutationRegistry {
    pub fn new() -> Self {
        Self {
            inner: ScopedExtensionRegistry::with_capacity(MAX_MUTATION_ROUTES).expect("valid host capacity"),
            operations: Arc::new(Semaphore::new(MAX_CONCURRENT_MUTATIONS)),
            versions: Arc::new(AtomicU64::new(1)),
            uploads: Arc::new(Semaphore::new(crate::MAX_CONCURRENT_UPLOADS)),
            large: Arc::new(Semaphore::new(crate::MAX_CONCURRENT_LARGE_DOCUMENTS)),
        }
    }
    pub fn close(&self) {
        self.inner.close();
    }
    /// Terminally closes admission, then waits for every operation/commit worker
    /// and response guard. There is deliberately no false-success timeout: a
    /// noncooperative native commit must finish before stores may be closed.
    pub async fn wait_for_idle(&self) {
        self.close();
        let count = u32::try_from(MAX_CONCURRENT_MUTATIONS).expect("bounded host mutation capacity");
        let permits = Arc::clone(&self.operations).acquire_many_owned(count).await.expect("private operation semaphore is never closed");
        drop(permits);
    }
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
    pub fn revision(&self) -> u64 {
        self.inner.snapshot().revision
    }
    pub fn snapshot(&self) -> MutationRouteSnapshot {
        MutationRouteSnapshot {
            inner: self.inner.snapshot(),
            operations: Arc::clone(&self.operations),
            uploads: Arc::clone(&self.uploads),
            large: Arc::clone(&self.large),
        }
    }
    pub fn stage(
        &self,
        owner: PluginId,
        scope: &PluginScope,
        generation: Generation,
        entries: Vec<MutationRegistration>,
    ) -> Result<MutationRouteRegistration, MutationError> {
        let values = prepare(entries, &self.versions)?;
        Ok(MutationRouteRegistration {
            inner: self.inner.stage_with_metadata(owner, scope, generation, values)?,
            versions: Arc::clone(&self.versions),
        })
    }
}
fn prepare(entries: Vec<MutationRegistration>, versions: &AtomicU64) -> Result<Vec<(MutationRegistration, Metadata)>, MutationError> {
    if entries.is_empty() || entries.len() > 32 {
        return Err(MutationError::Invalid("mutation batch capacity"));
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for entry in &entries {
        if !matches!(
            (entry.descriptor.input_kind, entry.factory_kind),
            (MutationInputKind::Form, FactoryKind::Form)
                | (MutationInputKind::Upload { .. }, FactoryKind::Upload)
                | (MutationInputKind::LocalePreference, FactoryKind::LocalePreference)
        ) {
            return Err(MutationError::Invalid("factory and descriptor kind mismatch"));
        }
        if !ids.insert((entry.descriptor.scope, entry.descriptor.id())) || !paths.insert((entry.descriptor.scope, entry.descriptor.path()))
        {
            return Err(MutationError::Invalid("duplicate mutation identifier or path"));
        }
    }
    let mut version = versions.load(Ordering::Acquire);
    loop {
        let next = version.checked_add(1).ok_or(MutationError::Unavailable)?;
        match versions.compare_exchange_weak(version, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(current) => version = current,
        }
    }
    Ok(entries
        .into_iter()
        .map(|entry| {
            let metadata = Metadata { descriptor: entry.descriptor.clone(), version };
            (entry, metadata)
        })
        .collect())
}
#[derive(Debug, Clone)]
pub struct MutationRouteRegistration {
    inner: ExtensionRegistration<MutationRegistration>,
    versions: Arc<AtomicU64>,
}
impl MutationRouteRegistration {
    pub fn publish(&self) -> Result<(), MutationError> {
        Ok(self.inner.publish()?)
    }
    pub fn replace(&self, entries: Vec<MutationRegistration>) -> Result<(), MutationError> {
        Ok(self.inner.replace_with_metadata(prepare(entries, &self.versions)?)?)
    }
    pub fn unpublish(&self) -> bool {
        self.inner.unpublish()
    }
}
#[derive(Debug, Clone)]
pub struct MutationRouteSnapshot {
    inner: ExtensionSnapshot<MutationRegistration>,
    operations: Arc<Semaphore>,
    uploads: Arc<Semaphore>,
    large: Arc<Semaphore>,
}
impl MutationRouteSnapshot {
    pub fn len(&self) -> usize {
        self.inner.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.entries.is_empty()
    }
    pub fn revision(&self) -> u64 {
        self.inner.revision
    }
    pub fn routes(&self) -> impl Iterator<Item = MutationRouteHandle> + '_ {
        (0..self.inner.entries.len()).map(|index| MutationRouteHandle { snapshot: self.clone(), index })
    }
    pub fn find(&self, owner: &PluginId, scope: PageScope, path: &str) -> Option<MutationRouteHandle> {
        if !validate_page_path(path) {
            return None;
        }
        let index = self.inner.entries.iter().position(|entry| {
            &entry.owner == owner && entry.metadata::<Metadata>().is_some_and(|m| m.descriptor.scope == scope && m.descriptor.path == path)
        })?;
        Some(MutationRouteHandle { snapshot: self.clone(), index })
    }
}
#[derive(Debug, Clone)]
pub struct MutationRouteHandle {
    snapshot: MutationRouteSnapshot,
    index: usize,
}
impl MutationRouteHandle {
    fn metadata(&self) -> &Metadata {
        self.snapshot.inner.entries[self.index].metadata().expect("paired mutation metadata")
    }
    pub fn descriptor(&self) -> &MutationDescriptor {
        &self.metadata().descriptor
    }
    pub fn version(&self) -> u64 {
        self.metadata().version
    }
    pub fn owner(&self) -> &PluginId {
        &self.snapshot.inner.entries[self.index].owner
    }
    pub fn generation(&self) -> Generation {
        self.snapshot.inner.entries[self.index].generation
    }
    pub fn admit(&self) -> Result<MutationLease, MutationError> {
        let large_permit = if self.descriptor().body_limit > crate::MAX_PAGE_BODY_BYTES {
            Some(Arc::clone(&self.snapshot.large).try_acquire_owned().map_err(|_| MutationError::Unavailable)?)
        } else {
            None
        };
        let upload_permit = if matches!(self.descriptor().input_kind, MutationInputKind::Upload { .. }) {
            Some(Arc::clone(&self.snapshot.uploads).try_acquire_owned().map_err(|_| MutationError::Unavailable)?)
        } else {
            None
        };
        let permit = Arc::clone(&self.snapshot.operations).try_acquire_owned().map_err(|_| MutationError::Unavailable)?;
        let inner = self.snapshot.inner.entries[self.index].admit().map_err(|_| MutationError::Unavailable)?;
        Ok(MutationLease { inner, permit, upload_permit, large_permit })
    }
}
#[derive(Debug)]
pub struct MutationLease {
    inner: ExtensionLease<MutationRegistration>,
    permit: OwnedSemaphorePermit,
    upload_permit: Option<OwnedSemaphorePermit>,
    large_permit: Option<OwnedSemaphorePermit>,
}
impl MutationLease {
    pub fn cancellation_token(&self) -> CancellationToken {
        self.inner.cancellation_token()
    }
    /// Starts bounded scope-tracked native work. Dropping this HTTP waiter signals
    /// cancellation but never drops a callback in the middle of a possible commit.
    pub async fn execute(self, context: MutationContext, input: FormData) -> Result<MutationExecution, MutationError> {
        let entry = self.inner.value();
        context.authorize(&entry.descriptor)?;
        if matches!(entry.descriptor.input_kind, MutationInputKind::Upload { .. }) {
            return Err(MutationError::Invalid("upload input required"));
        }
        let values = entry.descriptor.form.validate(input)?;
        let input = if entry.descriptor.input_kind == MutationInputKind::LocalePreference {
            ValidatedInput::LocalePreference(crate::LocaleChoice::new(values.text("locale").ok_or(MutationError::InvalidForm)?)?)
        } else {
            ValidatedInput::Form(values)
        };
        self.execute_validated(context, input).await
    }
    pub async fn execute_upload(self, context: MutationContext, input: crate::UploadData) -> Result<MutationExecution, MutationError> {
        let entry = self.inner.value();
        context.authorize(&entry.descriptor)?;
        let MutationInputKind::Upload { max_bytes } = entry.descriptor.input_kind else {
            return Err(MutationError::Invalid("unexpected upload"));
        };
        if input.bytes().len() > max_bytes {
            return Err(MutationError::InvalidForm);
        }
        self.execute_validated(context, ValidatedInput::Upload(input)).await
    }
    async fn execute_validated(self, mut context: MutationContext, input: ValidatedInput) -> Result<MutationExecution, MutationError> {
        let entry = self.inner.value();
        let deadline = Instant::now() + entry.descriptor.deadline;
        let cancellation = self.cancellation_token();
        if cancellation.is_cancelled() {
            return Err(MutationError::Unavailable);
        }
        let cooperative = cancellation.child_token();
        context.cancellation = cooperative.clone();
        let _waiter = CancelWaiter(cooperative.clone());
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let worker_cancellation = cancellation.clone();
        tokio::spawn(async move {
            let result = {
                let entry = self.inner.value();
                // The async block defers construction until after the biased
                // cancellation check, so queued closed work never enters native code.
                let started = std::sync::atomic::AtomicBool::new(false);
                let callback = async {
                    if worker_cancellation.is_cancelled() || cooperative.is_cancelled() {
                        return Err(MutationError::Unavailable);
                    }
                    if Instant::now() >= deadline {
                        return Err(MutationError::Timeout);
                    }
                    started.store(true, Ordering::Release);
                    entry.callback.execute(context, input, entry.descriptor.body_limit).await
                };
                tokio::pin!(callback);
                tokio::select! {biased;
                    ()=cooperative.cancelled()=>{
                        if started.load(Ordering::Acquire){callback.await}else{Err(MutationError::Unavailable)}
                    },
                    result=&mut callback=>result,
                    ()=tokio::time::sleep_until(deadline)=>{cooperative.cancel();callback.await},
                }
            };
            let result = if worker_cancellation.is_cancelled() {
                Err(MutationError::Unavailable)
            } else if Instant::now() >= deadline {
                Err(MutationError::Timeout)
            } else {
                result
            };
            let result = result.map(|response| {
                let (operation, cancellation) = self.into_parts();
                MutationExecution { response, operation, cancellation }
            });
            let _ = sender.send(result);
        });
        tokio::select! {biased;
            ()=cancellation.cancelled()=>Err(MutationError::Unavailable),
            result=tokio::time::timeout_at(deadline,receiver)=>result.map_err(|_|MutationError::Timeout)?.map_err(|_|MutationError::Unavailable)?,
        }
    }

    pub fn into_parts(self) -> (MutationOperation, CancellationToken) {
        let (registration, lease, cancellation) = self.inner.into_parts();
        drop(registration);
        (
            MutationOperation { _lease: lease, _permit: self.permit, _upload_permit: self.upload_permit, _large_permit: self.large_permit },
            cancellation,
        )
    }
}
#[derive(Debug)]
pub struct MutationOperation {
    _lease: OperationLease,
    _permit: OwnedSemaphorePermit,
    _upload_permit: Option<OwnedSemaphorePermit>,
    _large_permit: Option<OwnedSemaphorePermit>,
}

struct CancelWaiter(CancellationToken);
impl Drop for CancelWaiter {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
#[derive(Debug)]
pub struct MutationExecution {
    response: MutationResponse,
    operation: MutationOperation,
    cancellation: CancellationToken,
}
impl MutationExecution {
    pub fn into_parts(self) -> (MutationResponse, MutationOperation, CancellationToken) {
        (self.response, self.operation, self.cancellation)
    }
}

struct UploadMutator<F>(F);
impl<T, F, Fut> Mutator for UploadMutator<F>
where
    T: Template + Send + 'static,
    F: Fn(MutationContext, crate::UploadData) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<MutationOutcome<T>, MutationError>> + Send + 'static,
{
    fn execute(&self, context: MutationContext, input: ValidatedInput, limit: usize) -> MutationFuture<'_> {
        Box::pin(async move {
            let ValidatedInput::Upload(upload) = input else {
                return Err(MutationError::Invalid("factory input kind"));
            };
            match (self.0)(context, upload).await? {
                MutationOutcome::Redirect(path) => Ok(MutationResponse::Redirect(path)),
                MutationOutcome::Success(page) => Ok(MutationResponse::Success(page.render_with_limit(limit)?)),
                MutationOutcome::Invalid(page) => Ok(MutationResponse::Invalid(page.render_with_limit(limit)?)),
            }
        })
    }
}
/// Registers a compiled validation model and bounded bytes, not raw multipart data.
pub fn compiled_upload<T, F, Fut>(descriptor: MutationDescriptor, factory: F) -> MutationRegistration
where
    T: Template + Send + 'static,
    F: Fn(MutationContext, crate::UploadData) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<MutationOutcome<T>, MutationError>> + Send + 'static,
{
    MutationRegistration { descriptor, callback: Arc::new(UploadMutator(factory)), factory_kind: FactoryKind::Upload }
}
struct LocaleMutator<F>(F);
impl<T, F, Fut> Mutator for LocaleMutator<F>
where
    T: Template + Send + 'static,
    F: Fn(crate::PreferenceContext, crate::LocaleChoice) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<PreferenceOutcome<T>, MutationError>> + Send + 'static,
{
    fn execute(&self, context: MutationContext, input: ValidatedInput, limit: usize) -> MutationFuture<'_> {
        Box::pin(async move {
            let ValidatedInput::LocalePreference(choice) = input else {
                return Err(MutationError::Invalid("factory input kind"));
            };
            let cancellation = context.cancellation_token();
            let context = crate::PreferenceContext {
                actor: (context.actor.account_id > 0).then_some(context.actor),
                locale: context.locale,
                form: context.form,
                cancellation,
            };
            match (self.0)(context, choice).await? {
                PreferenceOutcome::Selected { choice, redirect } => Ok(MutationResponse::LocalePreference { choice, redirect }),
                PreferenceOutcome::Invalid(page) => Ok(MutationResponse::Invalid(page.render_with_limit(limit)?)),
            }
        })
    }
}
/// Only this factory may produce the host's typed locale-cookie intent. It cannot
/// choose cookie names/attributes or grant anonymous access to standard mutations.
pub fn compiled_locale_preference<T, F, Fut>(descriptor: MutationDescriptor, factory: F) -> MutationRegistration
where
    T: Template + Send + 'static,
    F: Fn(crate::PreferenceContext, crate::LocaleChoice) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<PreferenceOutcome<T>, MutationError>> + Send + 'static,
{
    MutationRegistration { descriptor, callback: Arc::new(LocaleMutator(factory)), factory_kind: FactoryKind::LocalePreference }
}
