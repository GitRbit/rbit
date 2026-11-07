//! Debug privacy at real rendering, MIME construction and transport boundaries.

use std::fmt::Debug;

use super::{Mail, Sender, build_message};
use crate::messages::MailHeaders;

mod accounts;
mod carriers;
mod notifications;

const ADDRESS: &str = "private-recipient@example.test";
const SITE: &str = "PRIVATE_SITE_SENTINEL";
const BASE: &str = "https://private-instance.example.test";
const TOKEN: &str = "https://private-instance.example.test/reset/PRIVATE_TOKEN_SENTINEL";
const UNSUBSCRIBE: &str = "https://private-instance.example.test/unsubscribe/PRIVATE_UNSUBSCRIBE_SENTINEL";
const TITLE: &str = "PRIVATE_SUBJECT_SENTINEL";
const BODY: &str = "PRIVATE_BODY_SENTINEL";

fn opaque(value: &impl Debug, secrets: &[&str]) {
    for output in [format!("{value:?}"), format!("{value:#?}")] {
        for secret in secrets {
            assert!(!output.contains(secret), "Debug must not disclose a private field");
        }
    }
}

fn headers() -> MailHeaders {
    MailHeaders {
        message_id: Some("<PRIVATE_MESSAGE_ID@example.test>".into()),
        in_reply_to: Some("<PRIVATE_PARENT_ID@example.test>".into()),
        one_click_unsubscribe: Some(UNSUBSCRIBE.into()),
    }
}

/// MIME must retain delivery content; only our Debug carriers are opaque.
/// The foreign lettre Message value is neither logged nor given a new Debug contract.
fn verify_delivery(mut mail: Mail, body: &str) {
    mail.from_name = Some(SITE.into());
    let subject = mail.subject.clone();
    assert!(mail.text.contains(body));
    assert!(mail.html.contains(body));
    let mut secrets = vec![ADDRESS, SITE, BASE, TOKEN, UNSUBSCRIBE, TITLE, BODY, &subject];
    secrets.extend(["PRIVATE_MESSAGE_ID", "PRIVATE_PARENT_ID"]);
    opaque(&mail, &secrets);
    opaque(&mail.headers, &secrets);
    let sender = Sender::new("private-sender@example.test", SITE).unwrap();
    opaque(&sender, &["private-sender@example.test", SITE]);
    let wire = String::from_utf8(build_message(&sender, mail).unwrap().formatted()).unwrap();
    assert!(wire.contains(ADDRESS));
    assert!(wire.contains(body), "MIME must retain the original private content");
}
