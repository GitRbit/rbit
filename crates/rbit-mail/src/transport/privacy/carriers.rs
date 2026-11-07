//! Transport and failure Debug never recurse into private mailbox or message fields.

use super::super::{LogMailer, Mailer, MemoryMailer, SmtpMailer};
use super::*;
use crate::{MailError, messages::PasswordReset};
use rbit_core::config::MailConfig;

#[tokio::test]
async fn memory_and_log_transport_debug_do_not_reveal_the_outbox_or_sender() {
    let message = PasswordReset {
        site_name: SITE.into(),
        base_url: BASE.into(),
        username: BODY.into(),
        link: TOKEN.into(),
        valid_for: "1 hour".into(),
    };
    let mail = Mail::render(ADDRESS.parse().unwrap(), &message).unwrap().with_headers(headers());
    let memory = MemoryMailer::new();
    memory.send(mail.clone()).await.unwrap();
    opaque(&memory, &[ADDRESS, SITE, BASE, BODY, TOKEN, UNSUBSCRIBE]);
    assert_eq!(memory.sent(), [mail], "explicit test outbox access retains the complete message");
    let log = LogMailer::new(Sender::new(ADDRESS, SITE).unwrap());
    opaque(&log, &[ADDRESS, SITE]);
    // Deliberately do not send through LogMailer: its explicit dev behavior logs content.
}

#[test]
fn smtp_debug_does_not_reveal_configuration_and_does_not_connect() {
    let config = MailConfig {
        smtp_host: "private-relay.example.test".into(),
        smtp_username: "PRIVATE_SMTP_USER_SENTINEL".into(),
        smtp_password: "PRIVATE_SMTP_PASSWORD_SENTINEL".into(),
        ..MailConfig::default()
    };
    let smtp = SmtpMailer::new(&config, Sender::new(ADDRESS, SITE).unwrap()).unwrap();
    opaque(&smtp, &[ADDRESS, SITE, "private-relay.example.test", "PRIVATE_SMTP_USER_SENTINEL", "PRIVATE_SMTP_PASSWORD_SENTINEL"]);
}

#[test]
fn error_debug_hides_details_without_changing_display_or_sources() {
    let address = MailError::Address { address: ADDRESS.into(), reason: TOKEN.into() };
    opaque(&address, &[ADDRESS, TOKEN]);
    assert!(address.to_string().contains(ADDRESS));
    assert!(address.to_string().contains(TOKEN));
    let config = MailError::Config(BODY.into());
    opaque(&config, &[BODY]);
    assert!(config.to_string().contains(BODY));
    let render = MailError::from(askama::Error::Custom(std::io::Error::other(TOKEN).into()));
    opaque(&render, &[TOKEN]);
    assert!(std::error::Error::source(&render).is_some());
    assert!(render.to_string().contains(TOKEN));
}
