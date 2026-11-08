//! A controllable resolver for unit tests: names a test registers get scripted answers,
//! in order (the last one repeats), and every lookup is counted. Names are matched the
//! way DNS matches them (ignoring ASCII case and one trailing dot); unregistered names
//! go to the system resolver. Tests register distinct names, so they can run in parallel.

use std::{
    collections::{HashMap, VecDeque},
    io,
    net::{IpAddr, SocketAddr},
    sync::{
        Arc, LazyLock, Mutex, PoisonError,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Debug)]
struct Script {
    answers: VecDeque<Vec<IpAddr>>,
    lookups: Arc<AtomicUsize>,
}

static NAMES: LazyLock<Mutex<HashMap<String, Script>>> = LazyLock::new(Mutex::default);

fn key(host: &str) -> String {
    host.strip_suffix('.').unwrap_or(host).to_ascii_lowercase()
}

/// A registered name; dropping it forgets the name.
#[derive(Debug)]
pub(crate) struct Name {
    key: String,
    lookups: Arc<AtomicUsize>,
}

impl Name {
    /// How many times the name was resolved so far.
    pub(crate) fn lookups(&self) -> usize {
        self.lookups.load(Ordering::SeqCst)
    }
}

impl Drop for Name {
    fn drop(&mut self) {
        NAMES.lock().unwrap_or_else(PoisonError::into_inner).remove(&self.key);
    }
}

/// Answers `host` with `answers` in turn; the last answer repeats. An empty answer is a
/// failed lookup.
pub(crate) fn serve(host: &str, answers: &[&[IpAddr]]) -> Name {
    assert!(!answers.is_empty(), "a scripted name needs an answer");
    let lookups = Arc::new(AtomicUsize::new(0));
    let script = Script { answers: answers.iter().map(|answer| answer.to_vec()).collect(), lookups: Arc::clone(&lookups) };
    let previous = NAMES.lock().unwrap_or_else(PoisonError::into_inner).insert(key(host), script);
    assert!(previous.is_none(), "{host} is scripted twice");
    Name { key: key(host), lookups }
}

/// The scripted answer for `host`, if a test registered it.
pub(super) fn answer(host: &str, port: u16) -> Option<io::Result<Vec<SocketAddr>>> {
    let mut names = NAMES.lock().unwrap_or_else(PoisonError::into_inner);
    let script = names.get_mut(&key(host))?;
    script.lookups.fetch_add(1, Ordering::SeqCst);
    let addresses = if script.answers.len() > 1 { script.answers.pop_front() } else { script.answers.front().cloned() };
    let addresses = addresses.unwrap_or_default();
    Some(if addresses.is_empty() {
        Err(io::Error::new(io::ErrorKind::NotFound, format!("{host} has no scripted address")))
    } else {
        Ok(addresses.into_iter().map(|ip| SocketAddr::new(ip, port)).collect())
    })
}
