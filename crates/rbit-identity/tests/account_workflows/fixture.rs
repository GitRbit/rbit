use std::{
    fmt,
    future::{Future, ready},
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

use rbit_contracts::Timestamp;
use rbit_identity::{
    Account, AccountId, AccountKind, PasswordWork, User,
    accounts::{AccountRepository, LoginRecord, NewUser, SignUp},
};
use rbit_kernel::BlockingBudget;

pub const PASSWORD: &str = "a sufficiently long password";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError(pub &'static str);
impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for StoreError {}

#[derive(Debug, Default)]
pub struct State {
    pub count: i64,
    pub login: Option<LoginRecord>,
    pub fail_lookup: bool,
    pub reject_registration: bool,
    pub calls: Vec<String>,
    pub writes: usize,
}

#[derive(Debug, Default, Clone)]
pub struct Repository(pub Arc<Mutex<State>>);

pub fn user() -> User {
    User {
        account: Account {
            id: AccountId(7),
            kind: AccountKind::User,
            name: "alice".parse().unwrap(),
            display_name: None,
            bio: None,
            created_at: Timestamp(0),
        },
        email: "alice@example.com".parse().unwrap(),
        is_admin: false,
    }
}

pub fn input() -> SignUp {
    SignUp { username: " alice ".into(), email: " alice@example.com ".into(), password: PASSWORD.into() }
}

pub fn work() -> PasswordWork {
    PasswordWork::new(BlockingBudget::new(NonZeroUsize::new(1).unwrap()))
}

impl Repository {
    fn insert(&self, new: &NewUser, method: &str, open: bool) -> Result<User, StoreError> {
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("{method}:{open}:{}", new.is_admin));
        assert!(new.password_hash.starts_with("$argon2id$v=19$"));
        assert_eq!(new.name.as_str(), "alice");
        assert_eq!(new.email.as_str(), "alice@example.com");
        if method == "register" && state.reject_registration {
            return Err(StoreError("registration transaction rejected"));
        }
        state.writes += 1;
        let mut user = user();
        user.is_admin = new.is_admin;
        Ok(user)
    }
}

impl AccountRepository for Repository {
    type Error = StoreError;
    fn count_users(&self) -> impl Future<Output = Result<i64, Self::Error>> + Send {
        let mut state = self.0.lock().unwrap();
        state.calls.push("count".into());
        ready(Ok(state.count))
    }
    fn find_login(&self, login: &str) -> impl Future<Output = Result<Option<LoginRecord>, Self::Error>> + Send {
        let mut state = self.0.lock().unwrap();
        state.calls.push(format!("find:{login}"));
        ready(if state.fail_lookup {
            Err(StoreError("lookup failed"))
        } else {
            Ok(state.login.as_ref().map(|record| LoginRecord {
                user: record.user.clone(),
                password_hash: record.password_hash.clone(),
                suspended: record.suspended,
            }))
        })
    }
    fn create_user(&self, new: NewUser) -> impl Future<Output = Result<User, Self::Error>> + Send {
        ready(self.insert(&new, "explicit", false))
    }
    fn create_user_with_initial_password(&self, new: NewUser) -> impl Future<Output = Result<User, Self::Error>> + Send {
        ready(self.insert(&new, "initial", false))
    }
    fn register_user(&self, new: NewUser, open: bool) -> impl Future<Output = Result<User, Self::Error>> + Send {
        ready(self.insert(&new, "register", open))
    }
}
