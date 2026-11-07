//! Account notice payloads and their actual MIME output remain separate from Debug.

use super::*;
use crate::messages::{EmailChanged, PasswordReset, VerifyEmail, VerifyReason};

#[test]
fn reset_and_verification_debug_hide_tokens_without_changing_delivery() {
    let reset = PasswordReset {
        site_name: SITE.into(),
        base_url: BASE.into(),
        username: BODY.into(),
        link: TOKEN.into(),
        valid_for: "PRIVATE_EXPIRY_SENTINEL".into(),
    };
    opaque(&reset, &[SITE, BASE, BODY, TOKEN, "PRIVATE_EXPIRY_SENTINEL"]);
    let mail = Mail::render(ADDRESS.parse().unwrap(), &reset).unwrap();
    assert!(mail.links().contains(&TOKEN));
    verify_delivery(mail, BODY);

    let verification = VerifyEmail {
        site_name: SITE.into(),
        base_url: BASE.into(),
        username: BODY.into(),
        link: TOKEN.into(),
        valid_for: "PRIVATE_EXPIRY_SENTINEL".into(),
        reason: VerifyReason::Changed,
    };
    opaque(&verification, &[SITE, BASE, BODY, TOKEN, "PRIVATE_EXPIRY_SENTINEL"]);
    let mail = Mail::render(ADDRESS.parse().unwrap(), &verification).unwrap();
    assert!(mail.links().contains(&TOKEN));
    verify_delivery(mail, BODY);
}

#[test]
fn address_change_debug_does_not_disclose_the_notice_fields() {
    let changed = EmailChanged { site_name: SITE.into(), base_url: BASE.into(), username: TITLE.into(), new_address: ADDRESS.into() };
    opaque(&changed, &[SITE, BASE, TITLE, ADDRESS]);
    let mail = Mail::render(ADDRESS.parse().unwrap(), &changed).unwrap();
    verify_delivery(mail, ADDRESS);
}
