#![allow(clippy::struct_excessive_bools)]

use std::{
    future::{Future, ready},
    pin::pin,
    sync::Mutex,
    task::{Context, Poll, Waker},
};

use super::{
    callback::{self, CallbackParams, EXPIRED},
    completion::{Completion, CompletionError, ExternalAccounts, ExternalAudit, LinkConflict, complete},
    registration::{CreateFailure, ExternalRegistrations, NewExternalAccount, RegistrationError, register},
    *,
};
use crate::{
    Account, AccountId, AccountKind, SessionToken, User,
    oidc::{Flow, FlowKey},
};
use rbit_contracts::Timestamp;

fn run<T>(future: impl Future<Output = T>) -> T {
    match pin!(future).poll(&mut Context::from_waker(Waker::noop())) {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("unexpected suspension"),
    }
}

fn user(id: i64) -> User {
    User {
        account: Account {
            id: AccountId(id),
            kind: AccountKind::User,
            name: "alice".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(1),
        },
        email: "alice@example.com".parse().unwrap(),
        is_admin: false,
    }
}

fn identity(email: Option<&str>, verified: bool) -> VerifiedIdentity {
    VerifiedIdentity {
        provider_id: "corp".to_owned(),
        subject: "s-1".to_owned(),
        email: email.map(str::to_owned),
        email_verified: verified,
        username: Some("alice".to_owned()),
        display_name: None,
        mfa: false,
    }
}

const fn policy(trust_email: bool, trust_mfa: bool, registration: ProviderRegistration) -> ProviderPolicy<'static> {
    ProviderPolicy { id: "corp", name: "Corp SSO", trust_email, trust_mfa, registration }
}

/// An in-memory port: one optionally linked identity and one account (id 5).
#[derive(Debug, Default)]
struct Port {
    linked: Option<i64>,
    email_account: bool,
    conflict: Option<LinkConflict>,
    suspended: bool,
    two_factor: bool,
    setup_required: bool,
    registration_open: bool,
    create: Option<CreateFailure<&'static str>>,
    log: Mutex<Vec<String>>,
}

impl Port {
    fn note(&self, entry: impl Into<String>) {
        self.log.lock().unwrap().push(entry.into());
    }
    fn log(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
    fn linked(&self) -> Result<i64, LinkConflict> {
        self.conflict.map_or(Ok(5), Err)
    }
}

impl ExternalAccounts for Port {
    type Error = &'static str;
    type Origin = ();
    type Linked = i64;
    type Account = User;

    fn find(&self, _: &str, _: &str) -> impl Future<Output = Result<Option<(i64, ())>, &'static str>> + Send {
        ready(Ok(self.linked.map(|account| (account, ()))))
    }
    fn linked_account(&self, linked: &i64) -> AccountId {
        AccountId(*linked)
    }
    fn touch(&self, _: &i64, _: &VerifiedIdentity) -> impl Future<Output = Result<(), &'static str>> + Send {
        self.note("touch");
        ready(Ok(()))
    }
    fn active_user(&self, account: AccountId) -> impl Future<Output = Result<Option<User>, &'static str>> + Send {
        ready(Ok((!self.suspended).then(|| user(account.0))))
    }
    fn user(&self, account: AccountId) -> impl Future<Output = Result<Option<User>, &'static str>> + Send {
        ready(Ok(Some(user(account.0))))
    }
    fn is_suspended(&self, _: AccountId) -> impl Future<Output = Result<bool, &'static str>> + Send {
        ready(Ok(self.suspended))
    }
    fn link(&self, _: AccountId, _: &VerifiedIdentity) -> impl Future<Output = Result<Result<i64, LinkConflict>, &'static str>> + Send {
        self.note("link");
        ready(Ok(self.linked()))
    }
    fn verified_email_account(&self, _: &str) -> impl Future<Output = Result<Option<(AccountId, ())>, &'static str>> + Send {
        self.note("email");
        ready(Ok(self.email_account.then_some((AccountId(5), ()))))
    }
    fn link_by_email(
        &self,
        _: AccountId,
        _: &VerifiedIdentity,
        (): &(),
    ) -> impl Future<Output = Result<Result<i64, LinkConflict>, &'static str>> + Send {
        self.note("link_by_email");
        ready(Ok(self.linked()))
    }
    fn setup_required(&self) -> impl Future<Output = Result<bool, &'static str>> + Send {
        ready(Ok(self.setup_required))
    }
    fn registration_open(&self) -> impl Future<Output = Result<bool, &'static str>> + Send {
        ready(Ok(self.registration_open))
    }
    fn start_registration(&self, _: VerifiedIdentity, next: String) -> SessionToken {
        self.note(format!("register {next}"));
        SessionToken::generate()
    }
    fn finish(&self, account: AccountId, (): ()) -> impl Future<Output = Result<User, &'static str>> + Send {
        ready(Ok(user(account.0)))
    }
    fn account_user<'a>(&self, account: &'a User) -> &'a User {
        account
    }
    fn two_factor_enabled(&self, _: AccountId) -> impl Future<Output = Result<bool, &'static str>> + Send {
        self.note("two_factor");
        ready(Ok(self.two_factor))
    }
    fn start_second_factor(&self, _: User, method: &str) -> SessionToken {
        self.note(format!("second step {method}"));
        SessionToken::generate()
    }
    fn audit(&self, entry: ExternalAudit<'_>) -> impl Future<Output = ()> + Send {
        self.note(format!("{:?} {} by {:?}", entry.kind, entry.detail, entry.by.map(User::id)));
        ready(())
    }
}

impl ExternalRegistrations for Port {
    fn site_registration_open(&self) -> bool {
        self.registration_open
    }
    fn create(
        &self,
        account: NewExternalAccount,
        _: &VerifiedIdentity,
    ) -> impl Future<Output = Result<User, CreateFailure<&'static str>>> + Send {
        self.note(format!("create {} verified={} open={}", account.name, account.email_verified, account.registration_open));
        ready(self.create.as_ref().map_or_else(
            || Ok(user(7)),
            |failure| {
                Err(match failure {
                    CreateFailure::Linked(conflict) => CreateFailure::Linked(*conflict),
                    CreateFailure::EmailTaken => CreateFailure::EmailTaken,
                    CreateFailure::Store(error) => CreateFailure::Store(*error),
                })
            },
        ))
    }
    fn end_registration(&self) {
        self.note("end");
    }
    fn created(&self, user: &User) {
        self.note(format!("created {}", user.id().0));
    }
}

type Outcome = Result<Completion<User>, CompletionError<&'static str>>;

fn finish(port: &Port, policy: &ProviderPolicy<'_>, identity: VerifiedIdentity, link: Option<i64>) -> Outcome {
    run(complete(port, policy, identity, "/next".to_owned(), link.map(AccountId)))
}

fn refused(outcome: Outcome) -> String {
    match outcome {
        Err(CompletionError::Refused(message)) => message,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

const OPEN: ProviderPolicy<'static> = policy(false, false, ProviderRegistration::FollowSite);

#[test]
fn linked_identities_sign_in_after_their_use_is_recorded() {
    let port = Port { linked: Some(5), ..Port::default() };
    assert!(matches!(finish(&port, &OPEN, identity(None, false), None), Ok(Completion::SignedIn(user)) if user.id() == AccountId(5)));
    assert_eq!(port.log(), ["touch", "two_factor", "SignIn Corp SSO by Some(AccountId(5))"]);
}

#[test]
fn linked_identities_answer_link_requests_without_touching() {
    let port = Port { linked: Some(5), ..Port::default() };
    assert!(matches!(finish(&port, &OPEN, identity(None, false), Some(5)), Ok(Completion::Linked)));
    let message = refused(finish(&port, &OPEN, identity(None, false), Some(6)));
    assert_eq!(message, "this Corp SSO account is already linked to another account here");
    assert_eq!(port.log(), Vec::<String>::new());
}

#[test]
fn suspended_accounts_and_second_steps() {
    let suspended = Port { linked: Some(5), suspended: true, ..Port::default() };
    assert!(matches!(finish(&suspended, &OPEN, identity(None, false), None), Err(CompletionError::Forbidden)));
    assert_eq!(suspended.log(), ["touch", "SignInFailed Corp SSO: account suspended by Some(AccountId(5))"]);

    let second = Port { linked: Some(5), two_factor: true, ..Port::default() };
    assert!(matches!(finish(&second, &OPEN, identity(None, false), None), Ok(Completion::SecondFactorRequired(_))));
    assert_eq!(second.log(), ["touch", "two_factor", "second step Corp SSO"]);

    let trusted = Port { linked: Some(5), two_factor: true, ..Port::default() };
    let mfa = VerifiedIdentity { mfa: true, ..identity(None, false) };
    assert!(matches!(finish(&trusted, &policy(false, true, ProviderRegistration::Never), mfa, None), Ok(Completion::SignedIn(_))));
    assert_eq!(trusted.log(), ["touch", "SignIn Corp SSO (provider MFA) by Some(AccountId(5))"], "trusted MFA skips the 2FA lookup");
}

#[test]
fn signed_in_accounts_link_new_identities() {
    let port = Port::default();
    assert!(matches!(finish(&port, &OPEN, identity(Some("a@example.com"), true), Some(5)), Ok(Completion::Linked)));
    assert_eq!(port.log(), ["link", "IdentityLinked Corp SSO (alice) by Some(AccountId(5))"]);

    let inactive = Port { suspended: true, ..Port::default() };
    assert!(matches!(finish(&inactive, &OPEN, identity(None, false), Some(5)), Err(CompletionError::Forbidden)));
    assert_eq!(inactive.log(), Vec::<String>::new(), "nothing is linked for an inactive account");

    let taken = Port { conflict: Some(LinkConflict::ProviderTaken), ..Port::default() };
    let message = refused(finish(&taken, &OPEN, identity(None, false), Some(5)));
    assert_eq!(message, "your account is already linked to another Corp SSO account; unlink it first");
}

#[test]
fn only_trusted_verified_addresses_link_by_email() {
    let trusting = policy(true, false, ProviderRegistration::Never);
    let port = Port { email_account: true, ..Port::default() };
    assert!(matches!(finish(&port, &trusting, identity(Some("a@example.com"), true), None), Ok(Completion::SignedIn(_))));
    assert_eq!(&port.log()[..3], ["email", "link_by_email", "IdentityLinked Corp SSO (alice), by verified email by Some(AccountId(5))"]);

    for (policy, identity) in [(OPEN, identity(Some("a@example.com"), true)), (trusting, identity(Some("a@example.com"), false))] {
        let port = Port { email_account: true, ..Port::default() };
        let _ = finish(&port, &policy, identity, None);
        assert!(!port.log().contains(&"email".to_owned()), "no address lookup");
    }

    let other = Port { email_account: true, conflict: Some(LinkConflict::ProviderTaken), ..Port::default() };
    let message = refused(finish(&other, &trusting, identity(Some("a@example.com"), true), None));
    assert_eq!(message, "the account with this email address is linked to a different Corp SSO account");
}

#[test]
fn unknown_identities_register_or_are_refused() {
    let port = Port { registration_open: true, ..Port::default() };
    assert!(matches!(finish(&port, &OPEN, identity(None, false), None), Ok(Completion::Register(_))));
    assert_eq!(port.log(), ["register /next"]);

    for port in [Port { registration_open: true, setup_required: true, ..Port::default() }, Port::default()] {
        let message = refused(finish(&port, &OPEN, identity(None, false), None));
        assert!(message.starts_with("no account here is linked to this Corp SSO account"), "{message}");
        assert_eq!(port.log(), ["SignInFailed Corp SSO: no linked account by None"]);
    }
    let always = Port::default();
    let policy = policy(false, false, ProviderRegistration::Always);
    assert!(matches!(finish(&always, &policy, identity(None, false), None), Ok(Completion::Register(_))));
}

#[test]
fn registration_creates_marks_and_records() {
    let port = Port { registration_open: true, ..Port::default() };
    let created = run(register(&port, &OPEN, &identity(Some("Alice@Example.com"), true), " bob ", "alice@example.com")).unwrap();
    assert_eq!(created.id(), AccountId(7));
    assert_eq!(
        port.log(),
        [
            "create bob verified=true open=true",
            "end",
            "created 7",
            "IdentityLinked Corp SSO (alice), new account by Some(AccountId(7))",
            "SignIn Corp SSO by Some(AccountId(7))",
        ]
    );

    let other_address = Port { registration_open: true, ..Port::default() };
    let _ = run(register(&other_address, &OPEN, &identity(Some("a@example.com"), true), "bob", "b@example.com"));
    assert_eq!(other_address.log()[0], "create bob verified=false open=true");

    let closed = Port::default();
    assert!(matches!(run(register(&closed, &OPEN, &identity(None, false), "bob", "b@example.com")), Err(RegistrationError::Closed)));
    let bad = Port { registration_open: true, ..Port::default() };
    assert!(matches!(run(register(&bad, &OPEN, &identity(None, false), "settings", "b@example.com")), Err(RegistrationError::Name(_))));
}

#[test]
fn registration_conflicts_explain_themselves() {
    let linked = Port { registration_open: true, create: Some(CreateFailure::Linked(LinkConflict::IdentityTaken)), ..Port::default() };
    match run(register(&linked, &OPEN, &identity(None, false), "bob", "b@example.com")) {
        Err(RegistrationError::Refused(message)) => {
            assert_eq!(message, "this Corp SSO account is already linked to an account here; sign in instead");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(linked.log().last().map(String::as_str), Some("end"));
    let email = Port { registration_open: true, create: Some(CreateFailure::EmailTaken), ..Port::default() };
    match run(register(&email, &OPEN, &identity(None, false), "bob", "b@example.com")) {
        Err(RegistrationError::Refused(message)) => assert!(message.starts_with("an account already uses this email address")),
        other => panic!("{other:?}"),
    }
    assert!(!email.log().contains(&"end".to_owned()), "the step stays open to choose another address");
}

#[test]
fn callbacks_are_checked_before_any_exchange() {
    let key = FlowKey::new(&[7; 32]).unwrap();
    let flow = Flow::new("corp", "/next", None, 1_000);
    let cookie = key.seal(&flow);
    let params = CallbackParams { code: Some("c"), state: Some(&flow.state), ..CallbackParams::default() };
    let (opened, code) = callback::check(&key, &OPEN, Some(&cookie), params, 1_010).unwrap();
    assert_eq!((opened.next.as_str(), code), ("/next", "c"));

    let refusal =
        |cookie: Option<&str>, params: CallbackParams<'_>, now: i64| callback::check(&key, &OPEN, cookie, params, now).unwrap_err();
    assert_eq!(refusal(None, params, 1_010).user, EXPIRED);
    assert_eq!(refusal(Some(&cookie), params, 1_000 + callback::FLOW_TTL_SECONDS + 120).log, "missing, forged or expired flow cookie");
    let other = key.seal(&Flow::new("github", "/", None, 1_000));
    assert_eq!(refusal(Some(&other), params, 1_010).log, "flow cookie for another provider");
    assert_eq!(refusal(Some(&cookie), CallbackParams { state: Some("x"), ..params }, 1_010).log, "state mismatch");
    let denied = refusal(Some(&cookie), CallbackParams { error: Some("access_denied"), ..params }, 1_010);
    assert_eq!(denied.user, "Corp SSO didn't sign you in (access was denied)");
    let failed = refusal(Some(&cookie), CallbackParams { error: Some("server_error"), error_description: Some("down\n"), ..params }, 1_010);
    assert_eq!(
        (failed.log.as_str(), failed.user.as_str()),
        ("provider error server_error: down\n", "Corp SSO reported an error: server_error: down")
    );
    let long = "c".repeat(callback::MAX_CODE_LEN + 1);
    assert_eq!(refusal(Some(&cookie), CallbackParams { code: Some(&long), ..params }, 1_010).log, "no code");
    assert_eq!(callback::unconfirmed(&OPEN, "bad token").user, "Corp SSO couldn't confirm who you are; try again");
}

#[test]
fn usernames_are_suggested_from_the_provider() {
    assert_eq!(suggest_username(&[Some("octocat"), Some("other")]), "octocat");
    assert_eq!(suggest_username(&[Some("Jane Doe"), None]), "JaneDoe");
    assert_eq!(suggest_username(&[Some("settings"), Some("alice")]), "alice", "reserved names are skipped");
    assert_eq!(suggest_username(&[Some(".hidden")]), "hidden");
    assert_eq!(suggest_username(&[Some("!!!"), None]), "");
    assert_eq!(suggest_username(&[Some(&"a".repeat(80))]).len(), AccountName::MAX_LEN);
    let mut from_address = identity(Some("jane.doe@example.com"), false);
    from_address.username = None;
    assert_eq!(suggested_username(&from_address), "jane.doe");
}

#[test]
fn registration_policy() {
    assert!(!may_register(ProviderRegistration::Never, true));
    assert!(may_register(ProviderRegistration::Always, false));
    assert!(may_register(ProviderRegistration::FollowSite, true) && !may_register(ProviderRegistration::FollowSite, false));
}

#[test]
fn the_last_way_to_sign_in_is_kept() {
    assert!(is_last_method(false, [("corp", true)], "corp"));
    assert!(is_last_method(false, [("corp", true), ("old", false)], "corp"), "an identity at a removed provider can't sign in");
    assert!(!is_last_method(false, [("corp", true), ("github", true)], "corp"));
    assert!(!is_last_method(true, [("corp", true)], "corp"));
}
