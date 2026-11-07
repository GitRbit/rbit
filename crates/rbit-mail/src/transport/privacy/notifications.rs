//! Private thread/digest payloads retain escaped delivery and opaque Debug output.

use super::*;
use crate::messages::{DigestEntry, DigestNotification, DigestRepository, DigestThread, ThreadNotification};

#[test]
fn private_thread_and_unsubscribe_headers_do_not_escape_through_debug() {
    let notification = ThreadNotification {
        site_name: SITE.into(),
        base_url: BASE.into(),
        repository: "PRIVATE_REPOSITORY_SENTINEL".into(),
        number: 37,
        title: TITLE.into(),
        headline: "PRIVATE_HEADLINE_SENTINEL".into(),
        excerpt: BODY.into(),
        link: TOKEN.into(),
        is_pull: true,
        is_reply: true,
        reason: "PRIVATE_REASON_SENTINEL".into(),
        settings_link: BASE.into(),
        unsubscribe_link: UNSUBSCRIBE.into(),
    };
    opaque(
        &notification,
        &[
            SITE,
            BASE,
            TITLE,
            BODY,
            TOKEN,
            UNSUBSCRIBE,
            "PRIVATE_REPOSITORY_SENTINEL",
            "PRIVATE_HEADLINE_SENTINEL",
            "PRIVATE_REASON_SENTINEL",
        ],
    );
    let mail = Mail::render(ADDRESS.parse().unwrap(), &notification).unwrap().with_headers(headers());
    assert!(mail.subject.contains(TITLE));
    assert!(mail.links().contains(&UNSUBSCRIBE));
    assert!(mail.headers.pairs().iter().any(|(name, value)| *name == "List-Unsubscribe" && value.contains(UNSUBSCRIBE)));
    verify_delivery(mail, BODY);
}

#[test]
fn digest_debug_is_opaque_at_every_nesting_level() {
    let entry = DigestEntry { headline: "PRIVATE_HEADLINE_SENTINEL".into(), excerpt: BODY.into(), link: TOKEN.into() };
    opaque(&entry, &[BODY, TOKEN, "PRIVATE_HEADLINE_SENTINEL"]);
    let thread = DigestThread { number: 37, title: TITLE.into(), is_pull: true, link: TOKEN.into(), entries: vec![entry] };
    opaque(&thread, &[TITLE, BODY, TOKEN]);
    let repository = DigestRepository { name: "PRIVATE_REPOSITORY_SENTINEL".into(), link: BASE.into(), threads: vec![thread] };
    opaque(&repository, &[BASE, TITLE, BODY, TOKEN, "PRIVATE_REPOSITORY_SENTINEL"]);
    let digest = DigestNotification {
        site_name: SITE.into(),
        base_url: BASE.into(),
        weekly: false,
        repositories: vec![repository],
        settings_link: BASE.into(),
        unsubscribe_link: UNSUBSCRIBE.into(),
    };
    opaque(&digest, &[SITE, BASE, TITLE, BODY, TOKEN, UNSUBSCRIBE, "PRIVATE_REPOSITORY_SENTINEL"]);
    let mail = Mail::render(ADDRESS.parse().unwrap(), &digest).unwrap().with_headers(headers());
    assert!(mail.links().contains(&TOKEN));
    assert!(mail.links().contains(&UNSUBSCRIBE));
    verify_delivery(mail, BODY);
}
