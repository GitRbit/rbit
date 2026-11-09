use super::*;
use crate::views::test_support::page;

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
