//! Instance identity. Text is rendered as escaped text, never administrator-supplied HTML.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BrandingConfig {
    pub primary_color: String,
    pub primary_text_color: String,
    pub description: String,
    pub welcome_title: String,
    pub welcome_message: String,
    pub logo_url: String,
    pub favicon_url: String,
    pub app_icon_url: String,
    pub footer_text: String,
    pub support_url: String,
    pub terms_url: String,
    pub privacy_url: String,
    pub show_powered_by: bool,
    pub show_version: bool,
    pub show_features: bool,
}

impl Default for BrandingConfig {
    fn default() -> Self {
        Self {
            primary_color: String::new(),
            primary_text_color: String::new(),
            description: "Self-hosted Git".into(),
            welcome_title: String::new(),
            welcome_message: "Self-hosted Git that stays out of your way. Fast to browse, simple to run, and yours.".into(),
            logo_url: String::new(),
            favicon_url: String::new(),
            app_icon_url: String::new(),
            footer_text: String::new(),
            support_url: String::new(),
            terms_url: String::new(),
            privacy_url: String::new(),
            show_powered_by: true,
            show_version: true,
            show_features: true,
        }
    }
}

impl BrandingConfig {
    pub fn validate(&self) -> Result<(), String> {
        for color in [&self.primary_color, &self.primary_text_color] {
            if !(color.is_empty() || color.len() == 7 && color.starts_with('#') && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit))
            {
                return Err("brand colors must be six-digit hex colors such as #2563eb, or blank".into());
            }
        }
        if self.primary_color.is_empty() != self.primary_text_color.is_empty() {
            return Err("set both the primary color and its text color, or leave both blank".into());
        }
        for (name, value, max) in [
            ("description", &self.description, 300),
            ("welcome title", &self.welcome_title, 100),
            ("welcome message", &self.welcome_message, 2000),
            ("footer text", &self.footer_text, 300),
        ] {
            if value.chars().count() > max || value.chars().any(char::is_control) {
                return Err(format!("{name} must be at most {max} characters and contain no control characters"));
            }
        }
        for (name, url) in [
            ("logo", &self.logo_url),
            ("favicon", &self.favicon_url),
            ("app icon", &self.app_icon_url),
            ("support", &self.support_url),
            ("terms", &self.terms_url),
            ("privacy", &self.privacy_url),
        ] {
            // Same-origin paths or HTTPS only. Reject protocol-relative URLs, backslashes,
            // whitespace and executable schemes, including browser URL normalization tricks.
            if !url.is_empty()
                && (url.len() > 2048
                    || url.chars().any(|c| c.is_whitespace() || c.is_control() || c == '\\')
                    || !(url.starts_with('/') && !url.starts_with("//") || url.starts_with("https://") && url.len() > 8))
            {
                return Err(format!("{name} URL must be a local /path or an HTTPS URL (at most 2048 bytes)"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branding_rejects_executable_urls_and_css_injection() {
        for url in ["javascript:alert(1)", "data:image/svg+xml,x", "//other.example/logo", "/\\other.example", "/\n/other.example"] {
            let branding = BrandingConfig { logo_url: url.into(), ..BrandingConfig::default() };
            assert!(branding.validate().is_err(), "{url:?}");
        }
        for url in ["/brand/logo.svg", "https://example.com/logo.png"] {
            assert!(BrandingConfig { logo_url: url.into(), ..BrandingConfig::default() }.validate().is_ok());
        }
        assert!(BrandingConfig { primary_color: "red;}body{display:none".into(), ..BrandingConfig::default() }.validate().is_err());
        assert!(BrandingConfig { primary_color: "#123456".into(), ..BrandingConfig::default() }.validate().is_err());
        assert!(
            BrandingConfig { primary_color: "#123456".into(), primary_text_color: "#ffffff".into(), ..BrandingConfig::default() }
                .validate()
                .is_ok()
        );
    }
}
