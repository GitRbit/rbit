use serde_json::json;

use super::*;

fn token(email: Option<&str>, amr: &[&str]) -> IdToken {
    IdToken {
        subject: "sub-1".to_owned(),
        email: email.map(str::to_owned),
        email_verified: email.is_some(),
        name: Some("Alice".to_owned()),
        preferred_username: None,
        amr: amr.iter().map(|&method| method.to_owned()).collect(),
    }
}

#[test]
fn authorization_requests_use_the_code_flow_with_pkce() {
    let oidc = authorization_query("client", "https://rbit/cb", "openid email", "st", "ch", Some("no"));
    let names: Vec<&str> = oidc.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["response_type", "client_id", "redirect_uri", "scope", "state", "code_challenge", "code_challenge_method", "nonce"]);
    assert_eq!((oidc[0].1, oidc[6].1, oidc[7].1), ("code", "S256", "no"));
    assert_eq!(authorization_query("c", "r", "s", "st", "ch", None).len(), 7, "GitHub gets no nonce");
}

#[test]
fn id_tokens_name_the_identity() {
    let identity = oidc_identity("corp", token(Some("a@example.com"), &["pwd", "mfa"]));
    assert_eq!((identity.subject.as_str(), identity.email_verified, identity.mfa), ("sub-1", true, true));
    assert_eq!(identity.display_name.as_deref(), Some("Alice"));
    assert!(!needs_userinfo(&identity));
    assert!(!oidc_identity("corp", token(None, &["pwd"])).mfa);
}

#[test]
fn userinfo_fills_the_address_for_the_same_subject_only() {
    let mut identity = oidc_identity("corp", token(None, &[]));
    assert!(needs_userinfo(&identity));
    assert!(!apply_userinfo(&mut identity, &json!({"sub": "other", "email": "x@example.com"})));
    assert_eq!(identity.email, None);

    let info = json!({"sub": "sub-1", "email": "a@example.com", "email_verified": "TRUE", "preferred_username": "alice"});
    assert!(apply_userinfo(&mut identity, &info));
    assert_eq!(
        (identity.email.as_deref(), identity.email_verified, identity.username.as_deref()),
        (Some("a@example.com"), true, Some("alice"))
    );

    identity.username = Some("kept".to_owned());
    assert!(apply_userinfo(&mut identity, &json!({"sub": "sub-1", "email_verified": false, "preferred_username": "other"})));
    assert_eq!((identity.email, identity.email_verified, identity.username.as_deref()), (None, false, Some("kept")));
}

#[test]
fn github_users_carry_only_a_verified_address() {
    let identity = github_identity("github", "42".to_owned(), "octocat".to_owned(), None, Some("o@example.com".to_owned()));
    assert_eq!(
        (identity.subject.as_str(), identity.username.as_deref(), identity.email_verified, identity.mfa),
        ("42", Some("octocat"), true, false)
    );
    assert!(!github_identity("github", "42".to_owned(), "octocat".to_owned(), None, None).email_verified);
}
