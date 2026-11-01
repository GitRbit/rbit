//! Compiled first-party language fragments; the host retains all authority.
mod host;
mod language;
mod management;
pub use host::{InstallError, LanguageHost, LanguageRequest, PackSummary};
use rbit_messages::LocaleSnapshot;
use rbit_plugin_web::{MutationRegistration, PageRegistration};

#[derive(Debug, Clone, Default)]
pub struct Routes {
    pub pages: Vec<PageRegistration>,
    pub mutations: Vec<MutationRegistration>,
}
/// Active preference UI follows provider activation; administration remains
/// installed independently so a disabled provider's retained packs are manageable.
pub fn routes<H: LanguageHost>(host: H) -> Result<(Routes, Routes), String> {
    Ok((language::bundle(host.clone())?, management::bundle(host)?))
}
fn declaration(error: impl std::fmt::Display) -> String {
    format!("invalid built-in language route: {error}")
}
fn title(messages: &LocaleSnapshot, key: &str) -> String {
    let text = messages.text("core", key);
    let mut title = String::new();
    for character in text.chars() {
        let character = if character.is_control() { ' ' } else { character };
        if title.len() + character.len_utf8() > 160 {
            break;
        }
        title.push(character);
    }
    let title = title.trim();
    if title.is_empty() { "Languages".to_owned() } else { title.to_owned() }
}

#[cfg(test)]
mod tests;
