//! Compatibility exports for shared URL construction and redirect validation.

pub use rbit_ui::urls::{DocumentBase, LinkKind, RepoUrls, ResolvedLink, encode_path, issue_url, safe_redirect_target};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redirect_targets_reject_controls_before_building_location_headers() {
        // Browsers remove tabs/newlines before parsing URLs, turning the first two
        // targets into external redirects. Other controls make Redirect::to panic.
        for target in ["/\t/evil.example", "/\n/evil.example", "/repo\r\nX-Injected: yes", "/\0", "/\u{7f}"] {
            let safe = safe_redirect_target(Some(target));
            assert_eq!(safe, "/", "{target:?}");
            let response = axum::response::IntoResponse::into_response(axum::response::Redirect::to(safe));
            assert_eq!(response.headers()[axum::http::header::LOCATION], "/");
        }
        assert_eq!(safe_redirect_target(Some("/alice/repo?tab=code#readme")), "/alice/repo?tab=code#readme");
    }
}
