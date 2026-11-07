//! One second-factor proof presented to two pending sign-ins at once, released together on
//! a multi-threaded runtime: exactly one sign-in completes, whichever reaches the store first.

use std::sync::Arc;

use rbit_auth::{SessionToken, TotpSecret, totp::time_step};
use rbit_core::{Timestamp, User};
use rbit_services::{
    ServiceError, Services,
    two_factor::{RECOVERY_CODE_COUNT, SignInOutcome},
};

use super::{Fixture, PASSWORD, client};

/// Enrols `user` and returns their secret, the confirming step and the recovery codes.
async fn enrol(fixture: &Fixture, user: &User) -> (TotpSecret, u64, Vec<String>) {
    fixture.services.two_factor.begin_enrolment(user, "rbit").await.unwrap();
    let stored = fixture.database.two_factor().find(user.id()).await.unwrap().unwrap();
    let secret = TotpSecret::from_bytes(&stored.secret).unwrap();
    let step = time_step(Timestamp::now().seconds());
    let codes = fixture.services.confirm_two_factor(user, &code(&secret, step), &client()).await.unwrap().expect("first factor");
    (secret, step, codes)
}

fn code(secret: &TotpSecret, step: u64) -> String {
    format!("{:06}", secret.code_at_step(step))
}

async fn pending(services: &Services, login: &str) -> SessionToken {
    match services.sign_in(login, PASSWORD.to_owned(), &client()).await.unwrap() {
        SignInOutcome::SecondFactorRequired(token) => token,
        SignInOutcome::SignedIn(_) => panic!("expected a second step"),
    }
}

/// Completes both pending sign-ins with `proof`, released together.
async fn complete_both(services: &Services, pending: [SessionToken; 2], proof: &str) -> Vec<Result<User, ServiceError>> {
    let barrier = Arc::new(tokio::sync::Barrier::new(pending.len()));
    let tasks: Vec<_> = pending
        .into_iter()
        .map(|token| {
            let (services, barrier, proof) = (services.clone(), Arc::clone(&barrier), proof.to_owned());
            tokio::spawn(async move {
                barrier.wait().await;
                services.complete_sign_in(&token, proof.as_str(), &client()).await
            })
        })
        .collect();
    let mut outcomes = Vec::new();
    for task in tasks {
        outcomes.push(task.await.unwrap());
    }
    outcomes
}

fn assert_one_winner(outcomes: &[Result<User, ServiceError>], user: &User) {
    let winners: Vec<_> = outcomes.iter().filter_map(|outcome| outcome.as_ref().ok()).collect();
    assert_eq!(winners.len(), 1, "one proof completed {} sign-ins: {outcomes:?}", winners.len());
    assert_eq!(winners[0].id(), user.id());
    let loser = outcomes.iter().find(|outcome| outcome.is_err()).unwrap();
    assert!(matches!(loser, Err(ServiceError::InvalidCredentials)), "{loser:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_authenticator_code_completes_only_one_of_two_pending_sign_ins() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (secret, step, _) = enrol(&fixture, &alice).await;
    let both = [pending(&fixture.services, "alice").await, pending(&fixture.services, "alice@example.com").await];
    let outcomes = complete_both(&fixture.services, both, &code(&secret, step + 1)).await;
    assert_one_winner(&outcomes, &alice);
    let stored = fixture.database.two_factor().find(alice.id()).await.unwrap().unwrap();
    assert_eq!(stored.last_used_step, Some(step + 1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_recovery_code_completes_only_one_of_two_pending_sign_ins() {
    let fixture = Fixture::new().await;
    let alice = fixture.user("alice").await;
    let (_, _, codes) = enrol(&fixture, &alice).await;
    let both = [pending(&fixture.services, "alice").await, pending(&fixture.services, "alice").await];
    let outcomes = complete_both(&fixture.services, both, &codes[0]).await;
    assert_one_winner(&outcomes, &alice);
    let left = fixture.services.two_factor.status(&alice).await.unwrap().recovery_codes_left;
    assert_eq!(left, i64::try_from(RECOVERY_CODE_COUNT).unwrap() - 1, "the code was spent exactly once");
}
