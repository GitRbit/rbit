//! Cookie-authenticated credential forms retain sessions and proofs on overload.

use axum::http::{StatusCode, header};

use super::{
    fixture::{Fixture, PASSWORD},
    unavailable,
};

#[tokio::test]
async fn password_change_busy_and_closed_preserve_both_sessions_then_retry_successfully() {
    for closed in [false, true] {
        let mut fixture = Fixture::new().await;
        let current = fixture.session().await;
        let other = fixture.session().await;
        let original = fixture.password_hash().await;
        let held = if closed {
            fixture.passwords.close();
            None
        } else {
            Some(fixture.passwords.begin().unwrap())
        };
        let form = format!("current_password={PASSWORD}&new_password=new-protocol-password-456&confirm_password=new-protocol-password-456");
        let (status, headers, body) = fixture.signed_form(&current, "/settings/account/password", &form).await;
        unavailable(status, &headers, &body);
        assert!(body.contains("hx-history=\"false\""));
        assert!(!body.contains("data-app-stylesheet"), "HTMX error responses reuse the existing document's styles");
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert!(fixture.session_is_valid(&current).await, "503 must not log out the caller");
        assert!(fixture.session_is_valid(&other).await, "503 must not revoke other sessions");
        assert!(fixture.password_hash().await == original, "503 must not replace the credential");
        drop(held);
        if closed {
            fixture.restart_password_owner().await;
        }
        let (status, _, _) = fixture.signed_form(&current, "/settings/account/password", &form).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert!(fixture.session_is_valid(&current).await, "successful self-service keeps the current session");
        assert!(!fixture.session_is_valid(&other).await, "successful change still revokes the other session");
        assert_ne!(fixture.password_hash().await, original);
        fixture.services.accounts.authenticate("keeper", "new-protocol-password-456".into()).await.unwrap();
    }
}

#[tokio::test]
async fn mfa_disable_busy_and_closed_keep_the_recovery_proof_and_session_for_retry() {
    for closed in [false, true] {
        let mut fixture = Fixture::new().await;
        let codes = fixture.enable_two_factor().await;
        let current = fixture.session().await;
        let before = fixture.recovery_codes_left().await;
        let held = if closed {
            fixture.passwords.close();
            None
        } else {
            Some(fixture.passwords.begin().unwrap())
        };
        let form = format!("password={PASSWORD}&code={}", codes[0]);
        let (status, headers, body) = fixture.signed_form(&current, "/settings/security/two-factor/disable", &form).await;
        unavailable(status, &headers, &body);
        assert!(body.contains("hx-history=\"false\""));
        assert!(!body.contains("data-app-stylesheet"), "exercise the HTMX head-style path");
        assert!(!body.contains(&codes[0]), "rejected proof must not be reflected in the generic error DOM");
        assert!(!headers.contains_key(header::SET_COOKIE));
        assert!(fixture.session_is_valid(&current).await);
        assert!(fixture.two_factor_is_enabled().await);
        assert_eq!(fixture.recovery_codes_left().await, before, "overload must not consume the one-use proof");
        drop(held);
        if closed {
            fixture.restart_password_owner().await;
        }
        let (status, _, _) = fixture.signed_form(&current, "/settings/security/two-factor/disable", &form).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "the identical recovery proof must still authorize the retry");
        assert!(!fixture.two_factor_is_enabled().await);
    }
}
