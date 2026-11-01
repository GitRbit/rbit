use super::*;
use rbit_plugin_api::{Generation, PluginId};
use rbit_plugin_runtime::PluginScope;
use rbit_plugin_web::*;
use std::future::{Future, ready};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Default)]
struct Data {
    enabled: AtomicBool,
    admin: AtomicBool,
    upload_address: AtomicUsize,
    saved: AtomicUsize,
}
#[derive(Clone)]
struct Host(Weak<Data>);
struct Request(Arc<Data>);
impl LanguageHost for Host {
    type Request = Request;
    fn connect(&self) -> Result<Request, PageError> {
        self.0.upgrade().map(Request).ok_or(PageError::Unavailable)
    }
}
impl LanguageRequest for Request {
    type Packs = std::vec::IntoIter<PackSummary>;
    fn available_locales(&self) -> Vec<String> {
        vec!["en".into(), "fr".into()]
    }
    fn is_enabled(&self) -> bool {
        self.0.enabled.load(Ordering::Acquire)
    }
    fn language_packs(&self, actor: i64) -> impl Future<Output = Result<Self::Packs, PageError>> + Send {
        ready(if actor != 1 || !self.0.admin.load(Ordering::Acquire) {
            Err(PageError::Forbidden)
        } else {
            Ok(vec![PackSummary {
                id: "<untrusted>".into(),
                locale: "fr".into(),
                version: "1.0.0".into(),
                message_count: 1,
                message_total: 2,
            }]
            .into_iter())
        })
    }
    fn install_language_pack(&self, actor: i64, bytes: &[u8]) -> impl Future<Output = Result<(), InstallError<Self::Packs>>> + Send {
        ready(if actor != 1 || !self.0.admin.load(Ordering::Acquire) {
            Err(InstallError::Forbidden)
        } else {
            self.0.upload_address.store(bytes.as_ptr() as usize, Ordering::Release);
            Err(InstallError::Invalid { message: "<invalid archive>".into(), packs: Vec::new().into_iter() })
        })
    }
    fn remove_language_pack(&self, actor: i64, _: &str) -> impl Future<Output = Result<(), MutationError>> + Send {
        ready(if actor != 1 || !self.0.admin.load(Ordering::Acquire) { Err(MutationError::Forbidden) } else { Ok(()) })
    }
    fn set_account_locale(&self, actor: i64, _: Option<&str>) -> impl Future<Output = Result<(), MutationError>> + Send {
        ready(if actor == 1 {
            self.0.saved.fetch_add(1, Ordering::AcqRel);
            Ok(())
        } else {
            Err(MutationError::Forbidden)
        })
    }
}
fn fixture() -> (Arc<Data>, Routes, Routes) {
    let data = Arc::new(Data::default());
    data.enabled.store(true, Ordering::Release);
    data.admin.store(true, Ordering::Release);
    let (active, management) = routes(Host(Arc::downgrade(&data))).unwrap();
    (data, active, management)
}
fn context(actor: Option<PageActor>) -> PageContext {
    PageContext { actor, repository: None, locale: LocaleSnapshot::english(0) }
}
fn administrator() -> PageActor {
    PageActor { account_id: 1, is_admin: true }
}
fn form(name: &str, path: &str) -> FormAction {
    FormAction::new(name, path, "csrf-proof").unwrap()
}

#[tokio::test]
async fn compiled_inventory_escapes_data_rechecks_host_authority_and_keeps_only_weak_host() {
    let (data, _, management) = fixture();
    let registry = PageRegistry::new();
    let scope = PluginScope::new(Generation::INITIAL);
    let owner: PluginId = "rbit.i18n-admin".parse().unwrap();
    let registration = registry.stage(owner.clone(), &scope, scope.generation(), management.pages).unwrap();
    registration.publish().unwrap();
    let handle = registry.snapshot().find(&owner, PageScope::Global, "packs").unwrap();
    let output = handle.admit().unwrap().render(context(Some(administrator()))).await.unwrap();
    assert!(output.html().contains("&#60;untrusted&#62;"));
    assert!(!output.html().contains("<untrusted>"));
    data.admin.store(false, Ordering::Release);
    assert!(matches!(handle.admit().unwrap().render(context(Some(administrator()))).await, Err(PageError::Forbidden)));
    data.admin.store(true, Ordering::Release);
    drop(data);
    assert!(matches!(handle.admit().unwrap().render(context(Some(administrator()))).await, Err(PageError::Unavailable)));
}
#[tokio::test]
async fn invalid_upload_uses_same_borrowed_buffer_and_compiled_escaped_error() {
    let (data, _, management) = fixture();
    let registry = MutationRegistry::new();
    let scope = PluginScope::new(Generation::INITIAL);
    let owner: PluginId = "rbit.i18n-admin".parse().unwrap();
    let registration = registry.stage(owner.clone(), &scope, scope.generation(), management.mutations).unwrap();
    registration.publish().unwrap();
    let handle = registry.snapshot().find(&owner, PageScope::Global, "packs/install").unwrap();
    let bytes = vec![b'x'; 2 * 1024 * 1024];
    let address = bytes.as_ptr() as usize;
    let context = MutationContext::new(
        administrator(),
        None,
        LocaleSnapshot::english(0),
        form("install", "/-/plugins/rbit.i18n-admin/packs/install"),
    );
    let result = handle.admit().unwrap().execute_upload(context, UploadData::new(bytes).unwrap()).await.unwrap();
    let (response, _operation, _cancellation) = result.into_parts();
    let MutationResponse::Invalid(body) = response else { panic!("{response:?}") };
    assert!(body.html().contains("&#60;invalid archive&#62;"));
    assert_eq!(data.upload_address.load(Ordering::Acquire), address);
}
#[tokio::test]
async fn anonymous_preference_never_writes_account_and_disabled_host_refuses() {
    let (data, active, _) = fixture();
    let registry = MutationRegistry::new();
    let scope = PluginScope::new(Generation::INITIAL);
    let owner: PluginId = "rbit.i18n".parse().unwrap();
    let registration = registry.stage(owner.clone(), &scope, scope.generation(), active.mutations).unwrap();
    registration.publish().unwrap();
    let handle = registry.snapshot().find(&owner, PageScope::Global, "language").unwrap();
    for actor in [None, Some(administrator())] {
        let context = MutationContext::for_preference(actor, LocaleSnapshot::english(0), form("language", "/-/plugins/rbit.i18n/language"));
        let input = FormData::new(vec![("locale".into(), "fr".into())]).unwrap();
        let result = handle.admit().unwrap().execute(context, input).await.unwrap();
        assert!(matches!(result.into_parts().0, MutationResponse::LocalePreference { .. }));
    }
    assert_eq!(data.saved.load(Ordering::Acquire), 1);
    let context = MutationContext::for_preference(None, LocaleSnapshot::english(0), form("language", "/-/plugins/rbit.i18n/language"));
    let invalid = handle.admit().unwrap().execute(context, FormData::new(vec![("locale".into(), "zz".into())]).unwrap()).await.unwrap();
    let (response, _operation, _cancellation) = invalid.into_parts();
    let MutationResponse::Invalid(body) = response else {
        panic!("{response:?}");
    };
    assert!(body.html().contains("role=\"alert\""));
    assert_eq!(data.saved.load(Ordering::Acquire), 1);
    data.enabled.store(false, Ordering::Release);
    let context = MutationContext::for_preference(None, LocaleSnapshot::english(0), form("language", "/-/plugins/rbit.i18n/language"));
    assert!(matches!(
        handle.admit().unwrap().execute(context, FormData::new(vec![("locale".into(), "fr".into())]).unwrap()).await,
        Err(MutationError::Unavailable)
    ));
}
#[test]
fn declarations_preserve_host_route_audiences_deadlines_and_names() {
    let (_data, active, management) = fixture();
    assert_eq!(active.pages.len(), 1);
    assert_eq!(management.pages.len(), 1);
    assert_eq!(active.mutations.len(), 1);
    assert_eq!(management.mutations.len(), 2);
    for page in active.pages.iter().chain(&management.pages) {
        assert_eq!(page.descriptor().deadline(), std::time::Duration::from_secs(1));
    }
    assert_eq!(active.pages[0].descriptor().audience(), PageAudience::Public);
    assert_eq!(management.pages[0].descriptor().audience(), PageAudience::SiteAdmin);
    for mutation in active.mutations.iter().chain(&management.mutations) {
        assert_eq!(mutation.descriptor().deadline(), std::time::Duration::from_secs(1));
    }
}
