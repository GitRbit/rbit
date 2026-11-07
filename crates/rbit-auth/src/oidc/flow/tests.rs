//! PKCE RFC vector with access to its private verifier construction.

use super::Pkce;

#[test]
fn pkce_matches_the_rfc_example() {
    // RFC 7636 appendix B.
    let pkce = Pkce("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_owned());
    assert_eq!(pkce.challenge(), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    assert_eq!(Pkce::METHOD, "S256");
}
