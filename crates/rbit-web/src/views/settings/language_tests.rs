use super::*;
use crate::views::test_support::page;

#[derive(Template)]
#[template(source = "{% extends \"base.html\" %}", ext = "html")]
struct Shell {
    page: Page,
}

/// Where each marker first occurs in `html`.
fn positions(html: &str, markers: &[&str]) -> Vec<usize> {
    markers.iter().map(|marker| html.find(marker).unwrap_or_else(|| panic!("{marker} is rendered"))).collect()
}

#[test]
fn header_language_button_leads_the_theme_buttons_in_reading_order() {
    let html = Shell { page: page("/") }.render().unwrap();
    let header = html.split("<header").nth(1).unwrap().split("</header>").next().unwrap();
    let button = header.split("<a href=\"/settings/language\"").nth(1).unwrap().split("</a>").next().unwrap();
    for part in [
        "class=\"btn btn-ghost btn-icon hidden sm:inline-flex\"",
        "hx-boost=\"false\"",
        "aria-haspopup=\"dialog\"",
        "aria-label=\"Language\"",
        "title=\"Language\"",
        "data-language-dialog-open",
        "<path d=\"m22 22-5-10-5 10\"/>",
    ] {
        assert!(button.contains(part), "the language button has {part}: {button}");
    }
    let order = positions(header, &["data-language-dialog-open", "data-theme-toggle", "data-theme-dialog-open"]);
    assert!(order.is_sorted(), "language, then the light/dark switch, then the theme picker: {order:?}");
    // On phones all three are rows of the site menu, in the same order.
    let menu = header.split("<details").nth(1).unwrap();
    let rows = positions(menu, &["data-language-dialog-open", "data-theme-toggle", "data-theme-dialog-open"]);
    assert!(rows.is_sorted(), "the menu rows keep the header's order: {rows:?}");
    assert!(menu.contains("<a class=\"menu-item sm:hidden\" href=\"/settings/language\" hx-boost=\"false\" aria-haspopup=\"dialog\""));
}

#[test]
fn header_language_button_mirrors_with_the_document_and_names_itself_in_the_page_language() {
    let html = Shell { page: page("/").with_locale(Locale::PseudoRtl) }.render().unwrap();
    assert!(html.contains("<html lang=\"ar-XB\" dir=\"rtl\""));
    let header = html.split("<header").nth(1).unwrap().split("</header>").next().unwrap();
    // Logical order only: no direction of its own, so a right-to-left page places the language button
    // to the right of the theme buttons, still first in reading order.
    assert!(!header.split_once('>').unwrap().0.contains("dir="));
    let order = positions(header, &["data-language-dialog-open", "data-theme-toggle", "data-theme-dialog-open"]);
    assert!(order.is_sorted(), "{order:?}");
    let spanish = Shell { page: page("/").with_locale(Locale::Spanish) }.render().unwrap();
    assert!(spanish.contains("aria-label=\"Idioma\" title=\"Idioma\" data-language-dialog-open"));
}

#[test]
fn dialog_lists_every_language_in_its_own_name_and_marks_the_current_one() {
    let html = LanguageDialog::new(page("/").with_locale(Locale::Japanese), false).render().unwrap();
    let opening = "<dialog class=\"lpk\" lang=\"ja\" dir=\"ltr\" aria-labelledby=\"lpk-title\" data-language-dialog>";
    assert!(html.trim_start().starts_with(opening));
    assert!(html.contains("<form class=\"lpk-list\" id=\"lpk-form\" method=\"post\" action=\"/settings/language\" hx-boost=\"false\""));
    let mut previous = 0;
    for language in Locale::LANGUAGES {
        let current = if language == Locale::Japanese { " aria-current=\"true\"" } else { "" };
        let row = format!("name=\"locale\" value=\"{tag}\" lang=\"{tag}\"{current} data-lpk-option>", tag = language.tag());
        let at = html.find(&row).unwrap_or_else(|| panic!("{row}"));
        assert!(at > previous, "languages keep their listed order");
        previous = at;
        assert!(html.contains(&format!("<span class=\"lpk-name\" dir=\"auto\">{}</span>", language.native_name())));
    }
    assert_eq!(html.matches("aria-current=\"true\"").count(), 1);
    let latin = ["English", "Deutsch", "Español", "Français", "Português (Brasil)"];
    for name in latin.into_iter().chain(["Русский", "日本語", "简体中文", "한국어"]) {
        assert!(html.contains(name), "{name}");
    }
    let close = Catalog::new(Locale::Japanese).localized(Message::Close);
    assert!(html.contains(&format!("aria-label=\"{close}\"")) && html.contains("data-lpk-close"));
    assert!(html.contains(Catalog::new(Locale::Japanese).localized(Message::DeviceOnly).as_str()), "visitors learn it is a cookie");
    assert!(!html.contains("value=\"\""), "nothing chosen yet: no way back to the browser's language needed");
    let chosen = LanguageDialog::new(page("/"), true).render().unwrap();
    let reset = chosen.split("type=\"submit\" form=\"lpk-form\" name=\"locale\" value=\"\" data-lpk-option>").nth(1).unwrap();
    assert!(reset.starts_with("Browser "), "a chosen language can go back to the browser's: {reset}");
}

#[test]
fn dialog_follows_a_right_to_left_page() {
    let html = LanguageDialog::new(page("/").with_locale(Locale::PseudoRtl), false).render().unwrap();
    assert!(html.trim_start().starts_with("<dialog class=\"lpk\" lang=\"ar-XB\" dir=\"rtl\""));
    assert!(!html.contains("dir=\"ltr\""), "rows follow the dialog; each name isolates itself with dir=auto");
}

#[test]
fn visitor_page_keeps_the_settings_form_without_the_settings_navigation() {
    let page = page("/settings/language").with_locale(Locale::German);
    let html = VisitorLanguagePage::new(page, Some(Locale::French), true).render().unwrap();
    assert!(html.contains("<form class=\"box mt-5 p-4\" method=\"post\" action=\"/settings/language\" hx-boost=\"false\">"));
    assert!(html.contains("<option value=\"fr\" lang=\"fr\" selected>Français</option>"));
    assert!(html.contains("role=\"status\""), "the saved notice");
    assert!(html.contains(Catalog::new(Locale::German).localized(Message::DeviceOnly).as_str()));
    assert!(!html.contains("/settings/profile"), "no settings navigation for a visitor");
}
