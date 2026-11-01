//! Compatibility exports for shared presentation primitives.

pub use rbit_ui::components::{Avatar, FormError, OrgLink, Pagination, RepoCard, SettingsCard};

#[cfg(test)]
mod tests {
    use askama::Template;
    #[derive(Template)]
    #[template(source = r#"{% import "macros.html" as ui %}{% call ui::icon(name, class) %}{% endcall %}"#, ext = "html")]
    struct CompatibilityIcon<'a> {
        name: &'a str,
        class: &'a str,
    }
    #[test]
    fn web_icon_macro_delegates_without_added_whitespace_or_escaping() {
        for name in ["folder", "user-plus", "unknown-icon", ""] {
            for class in ["size-4", r#"icon" onload="alert(1)<&"#] {
                assert_eq!(CompatibilityIcon { name, class }.render().unwrap(), rbit_ui::Icon::new(name, class).render().unwrap());
            }
        }
    }
}
