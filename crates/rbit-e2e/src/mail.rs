//! Captured mail: messages servers wrote with `[mail] transport = "log"`, and the
//! process's `tracing` subscriber that keeps them alongside the servers' log.

use std::time::Duration;

use crate::TestServer;

/// Every message the `log` mail transport wrote since [`MailLog::capture`] was first called,
/// by any server in this process.
static CAPTURED_MAIL: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// What the `log` transport writes before each message.
const LOG_TRANSPORT_PREAMBLE: &str = "email not sent (log transport)";

/// Messages written by servers using `[mail] transport = "log"`.
///
/// [`MailLog::capture`] installs a process-wide `tracing` subscriber that keeps only the
/// log transport's events, so every test server in the binary feeds the same log: filter
/// by recipient and by the server's URL (each server has its own port).
#[derive(Debug, Clone, Copy)]
pub struct MailLog;

/// One captured message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedMail {
    /// Every header the log shows (`From`, `To`, `Subject`, `Message-ID`, `List-Unsubscribe`, …).
    pub headers: Vec<(String, String)>,
    /// The plain-text part.
    pub text: String,
}

impl CapturedMail {
    fn parse(entry: &str) -> Option<Self> {
        let rest = entry.strip_prefix(LOG_TRANSPORT_PREAMBLE)?.strip_prefix('\n')?;
        let (head, text) = rest.split_once("\n\n").unwrap_or((rest, ""));
        let headers =
            head.lines().filter_map(|line| line.split_once(": ")).map(|(name, value)| (name.to_owned(), value.to_owned())).collect();
        Some(Self { headers, text: text.to_owned() })
    }

    /// The first header called `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(header, _)| header.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
    }

    pub fn to(&self) -> &str {
        self.header("To").unwrap_or_default()
    }

    pub fn subject(&self) -> &str {
        self.header("Subject").unwrap_or_default()
    }

    /// The first link in the text that starts with `prefix`.
    pub fn link_starting_with(&self, prefix: &str) -> Option<String> {
        self.text.split_whitespace().find(|word| word.starts_with(prefix)).map(str::to_owned)
    }
}

impl MailLog {
    /// Starts capturing (once per process; later calls only return the handle).
    pub fn capture() -> Self {
        install_tracing();
        Self
    }

    /// Every message captured so far, oldest first.
    pub fn all(self) -> Vec<CapturedMail> {
        let entries = CAPTURED_MAIL.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
        entries.iter().filter_map(|entry| CapturedMail::parse(entry)).collect()
    }

    /// Captured messages to `address` whose text mentions `server`'s URL.
    pub fn to(self, server: &TestServer, address: &str) -> Vec<CapturedMail> {
        self.all().into_iter().filter(|mail| mail.to() == address && mail.text.contains(server.base_url())).collect()
    }

    /// Waits (up to 20 seconds) until `server` has sent `count` messages to `address`, and
    /// returns them all.
    pub async fn wait_for(self, server: &TestServer, address: &str, count: usize) -> Vec<CapturedMail> {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let found = self.to(server, address);
            if found.len() >= count {
                return found;
            }
            assert!(std::time::Instant::now() < deadline, "timed out waiting for email {count} to {address}; got {found:#?}");
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
}

/// Installs the process's `tracing` subscriber, once: the mail capture behind [`MailLog`],
/// and the servers' log (`info` for rbit, `warn` for the rest; `RBIT_E2E_LOG` overrides
/// it) written through the test harness, so a failing test shows what its server said.
pub(crate) fn install_tracing() {
    static INSTALLED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    INSTALLED.get_or_init(|| {
        use tracing_subscriber::{EnvFilter, Layer as _, filter::filter_fn, layer::SubscriberExt as _};
        let mail = MailCapture.with_filter(filter_fn(|metadata| metadata.target().starts_with("rbit_mail")));
        let filter = EnvFilter::try_from_env("RBIT_E2E_LOG").unwrap_or_else(|_| EnvFilter::new("warn,rbit=info"));
        let log = tracing_subscriber::fmt::layer().with_test_writer().with_ansi(false).with_filter(filter);
        // Fails only if a test installed its own subscriber first; then nothing is captured
        // and a test waiting for mail says so.
        let _ = tracing::subscriber::set_global_default(tracing_subscriber::registry().with(mail).with(log));
    });
}

/// The `tracing` layer behind [`MailLog`]: keeps the message of each log transport event.
struct MailCapture;

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for MailCapture {
    fn on_event(&self, event: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        struct Message(String);
        impl tracing::field::Visit for Message {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.0 = format!("{value:?}");
                }
            }
        }
        let mut message = Message(String::new());
        event.record(&mut message);
        if message.0.starts_with(LOG_TRANSPORT_PREAMBLE) {
            CAPTURED_MAIL.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(message.0);
        }
    }
}
