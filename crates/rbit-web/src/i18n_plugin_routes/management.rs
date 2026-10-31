use askama::Template;
use rbit_messages::{Argument, LocaleSnapshot, MessageRef, Value};
use rbit_services::{
    I18nRouteBundle, LanguagePackInfo, ServiceError,
    plugin_pages::{
        CompiledPage, FormAction, FormField, FormFieldKind, FormReference, FormSchema, MutationAudience, MutationDescriptor, MutationError,
        MutationOutcome, PageAudience, PageDescriptor, PageError, PageScope, PluginRedirect, compiled_form_page, compiled_mutation,
        compiled_upload,
    },
};
use std::time::Duration;

use crate::state::WeakAppState;

#[derive(Debug)]
struct Pack {
    id: String,
    locale: String,
    version: String,
    coverage: String,
}
#[derive(Template)]
#[template(path = "i18n/packs.html")]
struct PackPage {
    messages: LocaleSnapshot,
    packs: Vec<Pack>,
    count: String,
    install: Option<FormAction>,
    remove: Option<FormAction>,
    error: Option<String>,
}
impl PackPage {
    fn new(
        messages: LocaleSnapshot,
        packs: Vec<LanguagePackInfo>,
        install: Option<FormAction>,
        remove: Option<FormAction>,
        error: Option<String>,
    ) -> Self {
        let count = messages.format(&MessageRef {
            namespace: "core",
            key: "language-pack-count",
            args: &[Argument { name: "count", value: number(packs.len()) }],
        });
        let packs = packs
            .into_iter()
            .map(|pack| Pack {
                coverage: messages.format(&MessageRef {
                    namespace: "core",
                    key: "language-pack-coverage",
                    args: &[
                        Argument { name: "count", value: number(pack.message_count) },
                        Argument { name: "total", value: number(pack.message_total) },
                    ],
                }),
                id: pack.id,
                locale: pack.locale,
                version: pack.version,
            })
            .collect();
        Self { messages, packs, count, install, remove, error }
    }
}
fn number(value: usize) -> Value<'static> {
    Value::Number(f64::from(u32::try_from(value).unwrap_or(u32::MAX)))
}

pub(super) fn bundle(weak: WeakAppState) -> Result<I18nRouteBundle, ServiceError> {
    let descriptor = PageDescriptor::new("packs", PageScope::Global, "packs", PageAudience::SiteAdmin, Duration::from_secs(1))
        .and_then(|descriptor| {
            descriptor.with_forms(vec![FormReference::new("install", "packs/install")?, FormReference::new("remove", "packs/remove")?])
        })
        .map_err(super::declaration)?;
    let page_weak = weak.clone();
    let page = compiled_form_page(descriptor, move |context, actions| {
        let weak = page_weak.clone();
        async move {
            let state = weak.upgrade().ok_or(PageError::Unavailable)?;
            let id = context.actor.ok_or(PageError::Forbidden)?.account_id;
            let actor = super::actor(&state, id).await.map_err(|_| PageError::Forbidden)?;
            let packs = state.services.language_packs(&actor).await.map_err(|_| PageError::Unavailable)?;
            let title = super::title(&context.locale, "plugin-languages");
            CompiledPage::new(
                &title,
                PackPage::new(context.locale, packs, actions.get("install").cloned(), actions.get("remove").cloned(), None),
            )
        }
    });
    let install_weak = weak.clone();
    let install_descriptor = MutationDescriptor::new(
        "install",
        PageScope::Global,
        "packs/install",
        MutationAudience::SiteAdmin,
        Duration::from_secs(1),
        FormSchema::new(Vec::new()).map_err(super::declaration)?,
    )
    .and_then(|descriptor| descriptor.with_upload(2 * 1024 * 1024))
    .map_err(super::declaration)?;
    let install = compiled_upload(install_descriptor, move |context, upload| {
        let weak = install_weak.clone();
        async move {
            let state = weak.upgrade().ok_or(MutationError::Unavailable)?;
            let actor = super::actor(&state, context.actor.account_id).await?;
            match state.services.install_language_pack(&actor, upload.bytes()).await {
                Ok(_) => Ok(MutationOutcome::Redirect(PluginRedirect::new("packs")?)),
                Err(ServiceError::Invalid(message)) => {
                    let packs = state.services.language_packs(&actor).await.map_err(|_| MutationError::Unavailable)?;
                    let title = super::title(&context.locale, "plugin-languages");
                    Ok(MutationOutcome::Invalid(CompiledPage::new(
                        &title,
                        PackPage::new(context.locale, packs, Some(context.form), None, Some(message)),
                    )?))
                }
                Err(ServiceError::Forbidden) => Err(MutationError::Forbidden),
                Err(_) => Err(MutationError::Unavailable),
            }
        }
    });
    let remove_descriptor = MutationDescriptor::new(
        "remove",
        PageScope::Global,
        "packs/remove",
        MutationAudience::SiteAdmin,
        Duration::from_secs(1),
        FormSchema::new(vec![FormField {
            name: "id".into(),
            kind: FormFieldKind::Text { max_bytes: 128, multiline: false },
            required: true,
        }])
        .map_err(super::declaration)?,
    )
    .map_err(super::declaration)?;
    let remove = compiled_mutation(remove_descriptor, move |context, values| {
        let weak = weak.clone();
        async move {
            let state = weak.upgrade().ok_or(MutationError::Unavailable)?;
            let actor = super::actor(&state, context.actor.account_id).await?;
            let id = values.text("id").ok_or(MutationError::InvalidForm)?;
            state.services.remove_language_pack(&actor, id).await.map_err(|error| {
                if matches!(error, ServiceError::Forbidden) { MutationError::Forbidden } else { MutationError::Unavailable }
            })?;
            Ok(MutationOutcome::<PackPage>::Redirect(PluginRedirect::new("packs")?))
        }
    });
    Ok(I18nRouteBundle { pages: vec![page], mutations: vec![install, remove] })
}
