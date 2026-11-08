//! Concurrent password and second-factor attempts: password work never exceeds its
//! admission, excess attempts are refused at once instead of queueing behind the hashing,
//! and an ordinary read is served meanwhile. Request throttling is off, so admission alone
//! bounds the work.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::http::{StatusCode, header};
use rbit_auth::{TotpSecret, totp::time_step};
use rbit_core::Timestamp;
use rbit_services::accounts::ClientInfo;

use super::fixture::{
    Fixture, ORDINARY_PATH, PASSWORD, PASSWORD_JOBS, assert_refused, assert_served_alongside, form, get, send, under_load,
};

/// Concurrent attempts of each kind.
const ATTEMPTS: usize = 8;

async fn enable_two_factor(fixture: &Fixture) {
    fixture.services.two_factor.begin_enrolment(&fixture.user, "rbit").await.unwrap();
    let record = fixture.database.two_factor().find(fixture.user.id()).await.unwrap().unwrap();
    let secret = TotpSecret::from_bytes(&record.secret).unwrap();
    let code = format!("{:06}", secret.code_at_step(time_step(Timestamp::now().seconds())));
    fixture.services.confirm_two_factor(&fixture.user, &code, &ClientInfo::default()).await.unwrap();
}

/// A pending second-factor sign-in cookie, from a correct password.
async fn pending_sign_in(fixture: &Fixture) -> String {
    let reply = send(fixture.router(), form("/login", &format!("login=hostile&password={PASSWORD}"), None)).await;
    assert_eq!(reply.status, StatusCode::SEE_OTHER, "{}", reply.body);
    let cookie = reply.headers[header::SET_COOKIE].to_str().unwrap();
    cookie.split(';').next().unwrap().to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_password_and_second_factor_attempts_stay_within_admission() {
    let fixture = Fixture::new(|_| {}, |_| {}).await;
    enable_two_factor(&fixture).await;
    let pending = pending_sign_in(&fixture).await;
    let session = fixture.services.accounts.start_session(&fixture.user).await.unwrap();
    let session = format!("{}={}", rbit_web::middleware::SESSION_COOKIE, session.encode());

    // Sample the admitted password jobs for as long as the flood lasts.
    let (peak, running) = (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicBool::new(true)));
    let sampler = {
        let (passwords, peak, running) = (fixture.passwords.clone(), Arc::clone(&peak), Arc::clone(&running));
        tokio::spawn(async move {
            while running.load(Ordering::Relaxed) {
                peak.fetch_max(passwords.admitted(), Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
    };
    let mut hostile = Vec::new();
    for attempt in 0..ATTEMPTS {
        hostile.push(form("/login", &format!("login=hostile&password=wrong-{attempt}"), None));
        hostile.push(form("/login", &format!("login=nobody-{attempt}&password=wrong"), None));
        hostile.push(form("/settings/security/two-factor/disable", &format!("password=wrong-{attempt}&code=not-a-code"), Some(&session)));
        hostile.push(form("/login/two-factor", "code=not-a-code", Some(&pending)));
    }
    let (replies, ordinary) = under_load(&fixture.router(), hostile, get(ORDINARY_PATH)).await;
    running.store(false, Ordering::Relaxed);
    sampler.await.unwrap();

    assert_served_alongside(&replies, &ordinary);
    let peak = peak.load(Ordering::Relaxed);
    assert!(peak <= PASSWORD_JOBS && peak > 0, "{peak} password jobs admitted at once");
    let mut busy = 0;
    for (index, reply) in replies.iter().enumerate() {
        match reply.status {
            // Code-only attempts never use password work.
            StatusCode::SERVICE_UNAVAILABLE if index % 4 != 3 => {
                assert_refused(reply, StatusCode::SERVICE_UNAVAILABLE);
                busy += 1;
            }
            StatusCode::UNPROCESSABLE_ENTITY => {}
            status => panic!("unexpected {status} for attempt {index}: {}", reply.body),
        }
    }
    // Queued attempts would all eventually hash and answer 422; refusals answer 503.
    assert!(busy > 0, "excess password attempts are refused, not queued");
    // Capacity is released: nothing is left admitted and a correct password works.
    assert_eq!(fixture.passwords.admitted(), 0);
    let ok = send(fixture.router(), form("/login", &format!("login=hostile&password={PASSWORD}"), None)).await;
    assert_eq!(ok.status, StatusCode::SEE_OTHER, "{}", ok.body);
    assert!(fixture.services.two_factor.is_enabled(fixture.user.id()).await.unwrap(), "no hostile attempt disabled 2FA");
}
