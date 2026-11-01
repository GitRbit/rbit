use crate::{LanguageHost, LanguageRequest, Routes};
use askama::Template;
use rbit_messages::LocaleSnapshot;
use rbit_plugin_web::{
    CompiledPage, FormAction, FormReference, LocaleChoice, MutationDescriptor, MutationError, PageAudience, PageDescriptor, PageScope,
    PluginRedirect, PreferenceOutcome, compiled_form_page, compiled_locale_preference,
};
use std::time::Duration;

#[derive(Template)]
#[template(path = "i18n/language.html")]
struct LanguagePage {
    messages: LocaleSnapshot,
    locales: Vec<String>,
    selected: String,
    form: Option<FormAction>,
    error: Option<String>,
}

pub(super) fn bundle<H: LanguageHost>(host: H) -> Result<Routes, String> {
    let descriptor = PageDescriptor::new("language", PageScope::Global, "language", PageAudience::Public, Duration::from_secs(1))
        .and_then(|descriptor| descriptor.with_forms(vec![FormReference::new("language", "language")?]))
        .map_err(super::declaration)?;
    let page_host = host.clone();
    let page = compiled_form_page(descriptor, move |context, actions| {
        let host = page_host.clone();
        async move {
            let state = host.connect()?;
            let selected = actions.locale_preference().and_then(LocaleChoice::preference).unwrap_or_default().to_owned();
            let title = super::title(&context.locale, "locale-title");
            CompiledPage::new(
                &title,
                LanguagePage {
                    messages: context.locale,
                    locales: state.available_locales(),
                    selected,
                    form: actions.get("language").cloned(),
                    error: None,
                },
            )
        }
    });
    let descriptor = MutationDescriptor::locale_preference("language", "language", Duration::from_secs(1)).map_err(super::declaration)?;
    let mutation = compiled_locale_preference(descriptor, move |context, choice| {
        let host = host.clone();
        async move {
            let state = host.connect().map_err(|_| MutationError::Unavailable)?;
            if !state.is_enabled() {
                return Err(MutationError::Unavailable);
            }
            if choice.preference().is_some_and(|locale| !state.available_locales().iter().any(|available| available == locale)) {
                let error = context.locale.text("core", "locale-unavailable");
                let title = super::title(&context.locale, "locale-title");
                return Ok(PreferenceOutcome::Invalid(CompiledPage::new(
                    &title,
                    LanguagePage {
                        messages: context.locale,
                        locales: state.available_locales(),
                        selected: choice.preference().unwrap_or_default().to_owned(),
                        form: Some(context.form),
                        error: Some(error),
                    },
                )?));
            }
            if let Some(actor) = context.actor {
                state.set_account_locale(actor.account_id, choice.preference()).await?;
            }
            Ok(PreferenceOutcome::Selected { choice, redirect: PluginRedirect::new("language")? })
        }
    });
    Ok(Routes { pages: vec![page], mutations: vec![mutation] })
}
