use crate::{InstallError, LanguageHost, LanguageRequest, PackSummary, Routes};
use askama::Template;
use rbit_messages::{Argument, LocaleSnapshot, MessageRef, Value};
use rbit_plugin_web::{
    CompiledPage, FormAction, FormField, FormFieldKind, FormReference, FormSchema, MutationAudience, MutationDescriptor, MutationError,
    MutationOutcome, PageAudience, PageDescriptor, PageError, PageScope, PluginRedirect, compiled_form_page, compiled_mutation,
    compiled_upload,
};
use std::time::Duration;

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
    fn new<Packs: ExactSizeIterator<Item = PackSummary>>(
        messages: LocaleSnapshot,
        packs: Packs,
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

pub(super) fn bundle<H: LanguageHost>(host: H) -> Result<Routes, String> {
    let descriptor = PageDescriptor::new("packs", PageScope::Global, "packs", PageAudience::SiteAdmin, Duration::from_secs(1))
        .and_then(|descriptor| {
            descriptor.with_forms(vec![FormReference::new("install", "packs/install")?, FormReference::new("remove", "packs/remove")?])
        })
        .map_err(super::declaration)?;
    let page_host = host.clone();
    let page = compiled_form_page(descriptor, move |context, actions| {
        let host = page_host.clone();
        async move {
            let state = host.connect()?;
            let id = context.actor.ok_or(PageError::Forbidden)?.account_id;
            let packs = state.language_packs(id).await?;
            let title = super::title(&context.locale, "plugin-languages");
            CompiledPage::new(
                &title,
                PackPage::new(context.locale, packs, actions.get("install").cloned(), actions.get("remove").cloned(), None),
            )
        }
    });
    let install_host = host.clone();
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
        let host = install_host.clone();
        async move {
            let state = host.connect().map_err(|_| MutationError::Unavailable)?;
            let actor = context.actor.account_id;
            match state.install_language_pack(actor, upload.bytes()).await {
                Ok(()) => Ok(MutationOutcome::Redirect(PluginRedirect::new("packs")?)),
                Err(InstallError::Invalid { message, packs }) => {
                    let title = super::title(&context.locale, "plugin-languages");
                    Ok(MutationOutcome::Invalid(CompiledPage::new(
                        &title,
                        PackPage::new(context.locale, packs, Some(context.form), None, Some(message)),
                    )?))
                }
                Err(InstallError::Forbidden) => Err(MutationError::Forbidden),
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
        let host = host.clone();
        async move {
            let state = host.connect().map_err(|_| MutationError::Unavailable)?;
            let id = values.text("id").ok_or(MutationError::InvalidForm)?;
            state.remove_language_pack(context.actor.account_id, id).await?;
            Ok(MutationOutcome::<PackPage>::Redirect(PluginRedirect::new("packs")?))
        }
    });
    Ok(Routes { pages: vec![page], mutations: vec![install, remove] })
}
