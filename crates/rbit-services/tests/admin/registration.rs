//! Bootstrap transactions race with sufficient admitted hashing capacity.

use super::*;

#[tokio::test]
async fn concurrent_signups_respect_the_bootstrap_boundary() {
    for open in [false, true] {
        let fixture = Fixture::new().await;
        let mut config = (*fixture.config).clone();
        config.server.open_registration = open;
        // This case tests three admitted registrations racing at persistence;
        // overload rejection has its own admission and HTTP regressions.
        config.security.max_password_jobs = 3;
        let services = Services::new(Arc::new(config), &fixture.database, &fixture.kv).await.unwrap();
        let signup = |name: &str| SignUp { username: name.to_owned(), email: format!("{name}@example.com"), password: PASSWORD.to_owned() };
        let token = services.accounts.setup_token().unwrap().to_owned();
        let (first, second, third) = tokio::join!(
            services.accounts.sign_up(signup("alice"), &token),
            services.accounts.sign_up(signup("bob"), &token),
            services.accounts.sign_up(signup("carol"), &token),
        );
        let outcomes = [first, second, third];
        assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), if open { 3 } else { 1 });
        assert_eq!(outcomes.iter().filter_map(|result| result.as_ref().ok()).filter(|user| user.is_admin).count(), 1);
        for result in outcomes {
            assert!(result.is_ok() || matches!(result, Err(ServiceError::RegistrationClosed)), "{result:?}");
        }
    }
}
