//! Exact namespaced GET pages rendered by compiled, reviewed Askama templates.
//!
//! Authors must use HTML autoescaping (e.g. `ext = "html"`) and review any `safe`
//! filters. Template is a native Rust contract, not a sandbox or proof of escaping.

use askama::Template;
use rbit_messages::LocaleSnapshot;
use rbit_plugin_api::{Generation, PluginId, UiContributionId, UiTarget};
use rbit_plugin_runtime::{
    ExtensionLease, ExtensionRegistration, ExtensionRegistryError, ExtensionSnapshot, OperationLease, PluginScope, ScopedExtensionRegistry,
};
use std::{collections::BTreeSet, fmt, future::Future, pin::Pin, sync::Arc, time::Duration};
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore},
    time::{Instant, timeout_at},
};
use tokio_util::sync::CancellationToken;

pub const MAX_PAGE_ROUTES: usize = 128;
pub const MAX_PAGE_PATH_BYTES: usize = 96;
pub const MAX_PAGE_BODY_BYTES: usize = 256 * 1024;
pub const MAX_PAGE_TITLE_BYTES: usize = 160;
pub const MAX_PAGE_DEADLINE: Duration = Duration::from_secs(1);
pub const MAX_CONCURRENT_PAGES: usize = 64;

#[derive(Debug, thiserror::Error)]
pub enum PageError {
    #[error("invalid page declaration or context: {0}")]
    Invalid(&'static str),
    #[error("page access forbidden")]
    Forbidden,
    #[error("page generation unavailable or render capacity exhausted")]
    Unavailable,
    #[error("page rendering timed out")]
    Timeout,
    #[error("page template rendering failed or exceeded its output limit")]
    Render,
    #[error(transparent)]
    Registry(#[from] ExtensionRegistryError),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PageScope {
    Global,
    Repository,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageAudience {
    Public,
    SignedIn,
    SiteAdmin,
    RepositoryReader,
    RepositoryAdmin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageDescriptor {
    id: UiContributionId,
    scope: PageScope,
    path: String,
    audience: PageAudience,
    deadline: Duration,
    forms: Vec<crate::FormReference>,
}
impl PageDescriptor {
    pub fn new(id: &str, scope: PageScope, path: &str, audience: PageAudience, deadline: Duration) -> Result<Self, PageError> {
        if !validate_page_path(path) || deadline < Duration::from_millis(1) || deadline > MAX_PAGE_DEADLINE {
            return Err(PageError::Invalid("path or deadline"));
        }
        if !matches!(
            (scope, audience),
            (PageScope::Global, PageAudience::Public | PageAudience::SignedIn | PageAudience::SiteAdmin)
                | (PageScope::Repository, PageAudience::RepositoryReader | PageAudience::RepositoryAdmin)
        ) {
            return Err(PageError::Invalid("scope and audience mismatch"));
        }
        Ok(Self {
            id: id.parse().map_err(|_| PageError::Invalid("page identifier"))?,
            scope,
            path: path.to_owned(),
            audience,
            deadline,
            forms: Vec::new(),
        })
    }
    pub fn with_forms(mut self, forms: Vec<crate::FormReference>) -> Result<Self, PageError> {
        if forms.len() > 8
            || forms.iter().enumerate().any(|(i, form)| {
                crate::FormReference::new(&form.name, &form.path).is_err() || forms[..i].iter().any(|old| old.name == form.name)
            })
        {
            return Err(PageError::Invalid("form references"));
        }
        self.forms = forms;
        Ok(self)
    }
    pub fn forms(&self) -> &[crate::FormReference] {
        &self.forms
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
    pub const fn audience(&self) -> PageAudience {
        self.audience
    }
    pub const fn deadline(&self) -> Duration {
        self.deadline
    }
    pub fn target(&self, owner: &PluginId) -> UiTarget {
        let path = match self.scope {
            PageScope::Global => format!("/-/plugins/{owner}/{}", self.path),
            PageScope::Repository => format!("/{{owner}}/{{repo}}/-/plugins/{owner}/{}", self.path),
        };
        path.parse().expect("validated page paths and plugin identifiers fit internal UI targets")
    }
}
/// The same exact literal grammar is used by registration and the host dispatcher.
pub fn validate_page_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= MAX_PAGE_PATH_BYTES
        && path.split('/').count() <= 8
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment.len() <= 32
                && segment.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_'))
        })
}

#[derive(Debug, Clone)]
pub struct PageActor {
    pub account_id: i64,
    pub is_admin: bool,
}
/// The host constructs this only after repository read authorization succeeds.
#[derive(Debug, Clone)]
pub struct PageRepository {
    pub repository_id: i64,
    pub owner: String,
    pub name: String,
    pub can_admin: bool,
}
#[derive(Debug, Clone)]
pub struct PageContext {
    pub actor: Option<PageActor>,
    pub repository: Option<PageRepository>,
    pub locale: LocaleSnapshot,
}
impl PageContext {
    fn authorize(&self, descriptor: &PageDescriptor) -> Result<(), PageError> {
        if self.actor.as_ref().is_some_and(|actor| actor.account_id <= 0) {
            return Err(PageError::Invalid("actor identifier"));
        }
        if let Some(repo) = &self.repository
            && (repo.repository_id <= 0 || !valid_name(&repo.owner, 39) || !valid_name(&repo.name, 100))
        {
            return Err(PageError::Invalid("repository context"));
        }
        if (descriptor.scope == PageScope::Global && self.repository.is_some())
            || (descriptor.scope == PageScope::Repository && self.repository.is_none())
        {
            return Err(PageError::Forbidden);
        }
        let allowed = match descriptor.audience {
            PageAudience::Public | PageAudience::RepositoryReader => true,
            PageAudience::SignedIn => self.actor.is_some(),
            PageAudience::SiteAdmin => self.actor.as_ref().is_some_and(|actor| actor.is_admin),
            PageAudience::RepositoryAdmin => self.actor.is_some() && self.repository.as_ref().is_some_and(|repo| repo.can_admin),
        };
        if allowed { Ok(()) } else { Err(PageError::Forbidden) }
    }
}
pub(crate) fn valid_name(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && !value.contains("..")
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.as_bytes()[value.len() - 1].is_ascii_alphanumeric()
        && value.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

/// A typed model, not a raw HTML return value. The bridge performs bounded rendering.
#[derive(Debug)]
pub struct CompiledPage<T> {
    title: String,
    template: T,
}
impl<T: Template> CompiledPage<T> {
    pub(crate) fn render(self) -> Result<PageBody, PageError> {
        // Never trust SIZE_HINT or allocate an unbounded rendered intermediary.
        let mut writer = BoundedWriter { output: String::new(), overflow: false };
        if self.template.render_into(&mut writer).is_err() || writer.overflow {
            return Err(PageError::Render);
        }
        Ok(PageBody { title: self.title, html: writer.output })
    }

    pub fn new(title: &str, template: T) -> Result<Self, PageError> {
        if title.is_empty() || title.len() > MAX_PAGE_TITLE_BYTES || title.chars().any(char::is_control) {
            return Err(PageError::Invalid("page title"));
        }
        Ok(Self { title: title.to_owned(), template })
    }
}
/// Only the compiled-template bridge can construct trusted page markup.
#[derive(Debug)]
pub struct PageBody {
    title: String,
    html: String,
}
impl PageBody {
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn html(&self) -> &str {
        &self.html
    }
}
struct BoundedWriter {
    output: String,
    overflow: bool,
}
impl fmt::Write for BoundedWriter {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        if self.overflow || self.output.len().checked_add(value.len()).is_none_or(|length| length > MAX_PAGE_BODY_BYTES) {
            self.overflow = true;
            return Err(fmt::Error);
        }
        self.output.push_str(value);
        Ok(())
    }
}

type RenderFuture<'a> = Pin<Box<dyn Future<Output = Result<PageBody, PageError>> + Send + 'a>>;
trait Renderer: Send + Sync {
    fn render(&self, context: PageContext, forms: crate::FormActions) -> RenderFuture<'_>;
}
struct TemplateRenderer<F>(F);
impl<T, F, Fut> Renderer for TemplateRenderer<F>
where
    T: Template + Send + 'static,
    F: Fn(PageContext, crate::FormActions) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CompiledPage<T>, PageError>> + Send + 'static,
{
    fn render(&self, context: PageContext, forms: crate::FormActions) -> RenderFuture<'_> {
        Box::pin(async move { (self.0)(context, forms).await?.render() })
    }
}

#[derive(Clone)]
pub struct PageRegistration {
    descriptor: PageDescriptor,
    renderer: Arc<dyn Renderer>,
}
impl fmt::Debug for PageRegistration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PageRegistration").field("descriptor", &self.descriptor).finish_non_exhaustive()
    }
}
impl PageRegistration {
    pub fn descriptor(&self) -> &PageDescriptor {
        &self.descriptor
    }
}
/// Registers only factories yielding compiled Askama models; String factories do not compile.
///
/// ```compile_fail
/// use rbit_plugin_web::{compiled_page,PageDescriptor};
/// fn invalid(descriptor:PageDescriptor) {
///     let _=compiled_page(descriptor, |_|async {Ok("raw HTML".to_owned())});
/// }
/// ```
pub fn compiled_page<T, F, Fut>(descriptor: PageDescriptor, factory: F) -> PageRegistration
where
    T: Template + Send + 'static,
    F: Fn(PageContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CompiledPage<T>, PageError>> + Send + 'static,
{
    PageRegistration { descriptor, renderer: Arc::new(TemplateRenderer(move |context, _| factory(context))) }
}

pub fn compiled_form_page<T, F, Fut>(descriptor: PageDescriptor, factory: F) -> PageRegistration
where
    T: Template + Send + 'static,
    F: Fn(PageContext, crate::FormActions) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CompiledPage<T>, PageError>> + Send + 'static,
{
    PageRegistration { descriptor, renderer: Arc::new(TemplateRenderer(factory)) }
}

#[derive(Debug, Clone)]
pub struct PageRegistry {
    inner: ScopedExtensionRegistry<PageRegistration>,
    renders: Arc<Semaphore>,
}
impl Default for PageRegistry {
    fn default() -> Self {
        Self::new()
    }
}
impl PageRegistry {
    pub fn new() -> Self {
        Self {
            inner: ScopedExtensionRegistry::with_capacity(MAX_PAGE_ROUTES).expect("host page capacity is valid"),
            renders: Arc::new(Semaphore::new(MAX_CONCURRENT_PAGES)),
        }
    }
    /// Terminally withdraws all callbacks and cancels admitted work across clones.
    /// The host still owns disposal of any independent plugin scope tasks.
    pub fn close(&self) {
        self.inner.close();
    }
    pub fn is_closed(&self) -> bool {
        self.inner.is_closed()
    }
    pub fn revision(&self) -> u64 {
        self.inner.snapshot().revision
    }
    pub fn snapshot(&self) -> PageRouteSnapshot {
        PageRouteSnapshot { inner: self.inner.snapshot(), renders: Arc::clone(&self.renders) }
    }
    pub fn stage(
        &self,
        owner: PluginId,
        scope: &PluginScope,
        generation: Generation,
        pages: Vec<PageRegistration>,
    ) -> Result<PageRouteRegistration, PageError> {
        validate_batch(&pages)?;
        let values = pages
            .into_iter()
            .map(|page| {
                let descriptor = page.descriptor.clone();
                (page, descriptor)
            })
            .collect();
        Ok(PageRouteRegistration { inner: self.inner.stage_with_metadata(owner, scope, generation, values)? })
    }
}
#[derive(Debug, Clone)]
pub struct PageRouteRegistration {
    inner: ExtensionRegistration<PageRegistration>,
}
impl PageRouteRegistration {
    pub fn publish(&self) -> Result<(), PageError> {
        Ok(self.inner.publish()?)
    }
    pub fn replace(&self, pages: Vec<PageRegistration>) -> Result<(), PageError> {
        validate_batch(&pages)?;
        let values = pages
            .into_iter()
            .map(|page| {
                let descriptor = page.descriptor.clone();
                (page, descriptor)
            })
            .collect();
        Ok(self.inner.replace_with_metadata(values)?)
    }
    pub fn unpublish(&self) -> bool {
        self.inner.unpublish()
    }
}
fn validate_batch(pages: &[PageRegistration]) -> Result<(), PageError> {
    if pages.is_empty() || pages.len() > 32 {
        return Err(PageError::Invalid("page batch capacity"));
    }
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for page in pages {
        if !ids.insert((page.descriptor.scope, page.descriptor.id())) || !paths.insert((page.descriptor.scope, page.descriptor.path())) {
            return Err(PageError::Invalid("duplicate page identifier or path in scope"));
        }
    }
    Ok(())
}
#[derive(Debug, Clone)]
pub struct PageRouteSnapshot {
    inner: ExtensionSnapshot<PageRegistration>,
    renders: Arc<Semaphore>,
}
impl PageRouteSnapshot {
    pub fn len(&self) -> usize {
        self.inner.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.entries.is_empty()
    }
    pub const fn revision(&self) -> u64 {
        self.inner.revision
    }
    pub fn find(&self, owner: &PluginId, scope: PageScope, path: &str) -> Option<PageRouteHandle> {
        if !validate_page_path(path) {
            return None;
        }
        let index = self.inner.entries.iter().position(|entry| {
            &entry.owner == owner
                && entry.metadata::<PageDescriptor>().is_some_and(|descriptor| descriptor.scope == scope && descriptor.path == path)
        })?;
        Some(PageRouteHandle { snapshot: self.clone(), index })
    }
}
#[derive(Debug, Clone)]
pub struct PageRouteHandle {
    snapshot: PageRouteSnapshot,
    index: usize,
}
impl PageRouteHandle {
    pub fn descriptor(&self) -> &PageDescriptor {
        self.snapshot.inner.entries[self.index].metadata().expect("page metadata is paired at registration")
    }
    pub fn owner(&self) -> &PluginId {
        &self.snapshot.inner.entries[self.index].owner
    }
    pub fn generation(&self) -> Generation {
        self.snapshot.inner.entries[self.index].generation
    }
    pub fn admit(&self) -> Result<PageLease, PageError> {
        let permit = Arc::clone(&self.snapshot.renders).try_acquire_owned().map_err(|_| PageError::Unavailable)?;
        let inner = self.snapshot.inner.entries[self.index].admit().map_err(|_| PageError::Unavailable)?;
        Ok(PageLease { inner, permit })
    }
}
#[derive(Debug)]
pub struct PageLease {
    inner: ExtensionLease<PageRegistration>,
    permit: OwnedSemaphorePermit,
}
impl PageLease {
    pub fn cancellation_token(&self) -> CancellationToken {
        self.inner.cancellation_token()
    }
    pub async fn render(&mut self, context: PageContext) -> Result<PageBody, PageError> {
        self.render_with_forms(context, crate::FormActions::default()).await
    }
    pub async fn render_with_forms(&mut self, context: PageContext, forms: crate::FormActions) -> Result<PageBody, PageError> {
        let registration = self.inner.value();
        context.authorize(&registration.descriptor)?;
        let deadline = Instant::now() + registration.descriptor.deadline;
        let cancellation = self.inner.cancellation_token();
        let page = tokio::select! {
            biased;
            ()=cancellation.cancelled()=>return Err(PageError::Unavailable),
            result=timeout_at(deadline,async {registration.renderer.render(context,forms).await})=>result.map_err(|_|PageError::Timeout)??,
        };
        // Synchronous native rendering cannot be preempted. Check again before
        // publishing markup in case it completed after the deadline or revocation.
        if cancellation.is_cancelled() {
            return Err(PageError::Unavailable);
        }
        if Instant::now() >= deadline {
            return Err(PageError::Timeout);
        }
        Ok(page)
    }
    pub fn into_parts(self) -> (PageOperation, CancellationToken) {
        let (registration, lease, cancellation) = self.inner.into_parts();
        drop(registration);
        (PageOperation { _lease: lease, _permit: self.permit }, cancellation)
    }
}
/// Host-owned response guard. Keep it until the last body producer is dropped.
#[derive(Debug)]
pub struct PageOperation {
    _lease: OperationLease,
    _permit: OwnedSemaphorePermit,
}

#[cfg(test)]
mod writer_tests {
    use super::*;
    use std::fmt::Write as _;
    #[test]
    fn overflow_is_latched_even_if_a_native_writer_caller_ignores_the_error() {
        let mut writer = BoundedWriter { output: String::new(), overflow: false };
        writer.write_str(&"x".repeat(MAX_PAGE_BODY_BYTES)).unwrap();
        let _ignored = writer.write_str("extra");
        assert!(writer.overflow);
        assert!(writer.write_str("").is_err());
        assert_eq!(writer.output.len(), MAX_PAGE_BODY_BYTES);
    }
}
