//! Sending one delivery: the HTTP client and its address guard. Signing, headers and
//! retry timing are feature policy ([`rbit_notifications::webhooks`]).
//!
//! The client never follows redirects, ignores proxy environment variables (a proxy
//! would resolve names itself and bypass the guard), applies `[webhooks] timeout_seconds`
//! to the whole exchange, and resolves names through [`GuardedResolver`], which refuses
//! internal addresses at connection time, so a name that resolves to a public address
//! when the hook is saved and to `127.0.0.1` later (DNS rebinding) is still refused.
//! Address literals never reach a resolver, so they are checked before sending.

use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Once},
    time::{Duration, Instant},
};

use rbit_core::{
    Timestamp,
    webhook::{WebhookUrl, is_internal_address},
};
use rbit_storage::sqlite::webhooks::{AttemptRecord, ClaimedDelivery};
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use rbit_notifications::webhooks::protocol::format_guid;
/// The signed wire contract and retry schedule are feature policy.
pub use rbit_notifications::webhooks::{
    policy::retry_delay,
    protocol::{MAX_RESPONSE_BODY, MAX_RESPONSE_HEADERS, USER_AGENT, request_headers, signature, user_agent},
};

/// A random number in `[0, 1)`.
pub fn random_unit() -> f64 {
    f64::from(getrandom::u32().unwrap_or(u32::MAX / 2)) / (f64::from(u32::MAX) + 1.0)
}

/// A random delivery id (`X-Rbit-Delivery`), formatted like a UUID v4.
pub fn new_guid() -> String {
    let mut bytes = [0_u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        // Uniqueness is all that matters here; time-based bytes are good enough.
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        bytes = nanos.to_le_bytes();
    }
    format_guid(bytes)
}

// ------------------------------------------------------------------ address guard

/// Why the guard refused a target. Its message is recorded in the delivery log.
#[derive(Debug, thiserror::Error)]
#[error("{host} resolves only to internal addresses, which webhooks may not reach")]
pub struct RefusedAddress {
    host: String,
}

/// Resolves names with [`crate::outbound::lookup`] and drops internal addresses (unless allowed).
#[derive(Debug, Clone, Copy)]
pub struct GuardedResolver {
    allow_private: bool,
}

impl GuardedResolver {
    pub fn new(allow_private: bool) -> Self {
        Self { allow_private }
    }

    /// The addresses of `resolved` a delivery may connect to.
    pub fn permitted(&self, resolved: impl IntoIterator<Item = SocketAddr>) -> Vec<SocketAddr> {
        resolved.into_iter().filter(|address| self.allow_private || !is_internal_address(address.ip())).collect()
    }
}

impl Resolve for GuardedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let guard = *self;
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let resolved = crate::outbound::lookup(&host, 0).await?;
            let permitted = guard.permitted(resolved);
            if permitted.is_empty() {
                return Err(Box::new(RefusedAddress { host }) as Box<dyn std::error::Error + Send + Sync>);
            }
            let addresses: Addrs = Box::new(permitted.into_iter());
            Ok(addresses)
        })
    }
}

// ------------------------------------------------------------------ the client

static CRYPTO_PROVIDER: Once = Once::new();

/// Makes `ring` the process's rustls crypto provider (unless one is installed already),
/// so neither this client nor any other rustls user needs `CMake` or `NASM` to build.
pub fn install_crypto_provider() {
    CRYPTO_PROVIDER.call_once(|| {
        // An error only means another provider was installed first, which is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// The HTTP client deliveries use.
pub fn build_client(allow_private: bool, timeout: Duration, report_version: bool) -> Result<reqwest::Client, reqwest::Error> {
    install_crypto_provider();
    reqwest::Client::builder()
        .user_agent(user_agent(report_version))
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .timeout(timeout)
        .connect_timeout(timeout)
        .pool_max_idle_per_host(2)
        .dns_resolver(Arc::new(GuardedResolver::new(allow_private)))
        .build()
}

/// Checks the same canonical URL the HTTP client will use. Numeric IPv4 spellings
/// such as `127.1` or `0x7f000001` become address literals in reqwest and never reach
/// its guarded resolver, even though `std::net::IpAddr` rejects their original form.
pub(super) fn target_url(url: &WebhookUrl, allow_private: bool) -> Result<reqwest::Url, String> {
    let target = reqwest::Url::parse(url.as_str()).map_err(|error| format!("invalid webhook URL: {error}"))?;
    let host = target.host_str().unwrap_or_default().trim_start_matches('[').trim_end_matches(']');
    if !allow_private && (url.targets_internal_host() || host.parse::<IpAddr>().is_ok_and(is_internal_address)) {
        return Err(format!("{host} is an internal address, which webhooks may not reach"));
    }
    Ok(target)
}

/// The result of one attempt.
#[derive(Debug)]
pub struct Attempt {
    pub record: AttemptRecord,
    pub succeeded: bool,
    /// Retrying cannot help (the target is refused, the URL is invalid).
    pub permanent: bool,
}

/// Sends `delivery` once and reports what happened. Never returns an error: failures are
/// part of the record.
pub async fn attempt(client: Option<&reqwest::Client>, delivery: &ClaimedDelivery, allow_private: bool, report_version: bool) -> Attempt {
    let signature = delivery.secret.as_deref().map(|secret| signature(secret, delivery.payload.as_bytes()));
    let headers = request_headers(delivery.event.as_str(), &delivery.guid, delivery.hook_id.0, signature.as_deref(), report_version);
    let mut record = AttemptRecord {
        at: Timestamp::now(),
        request_headers: headers.clone(),
        response_status: None,
        response_headers: Vec::new(),
        response_body: None,
        duration_ms: 0,
        error: None,
    };
    let refuse = |mut record: AttemptRecord, message: String| {
        record.error = Some(message);
        Attempt { record, succeeded: false, permanent: true }
    };

    let url = match WebhookUrl::parse(&delivery.url) {
        Ok(url) => url,
        Err(error) => return refuse(record, error.to_string()),
    };
    let target = match target_url(&url, allow_private) {
        Ok(target) => target,
        Err(error) => return refuse(record, error),
    };
    let Some(client) = client else {
        record.error = Some("the HTTP client could not be initialised; see the server log".to_owned());
        return Attempt { record, succeeded: false, permanent: false };
    };

    let started = Instant::now();
    let mut request = client.post(target).body(delivery.payload.clone());
    for (name, value) in &headers {
        request = request.header(name.as_str(), value.as_str());
    }
    let result = match request.send().await {
        Ok(mut response) => {
            let status = response.status();
            record.response_status = Some(status.as_u16());
            record.response_headers = response
                .headers()
                .iter()
                .take(MAX_RESPONSE_HEADERS)
                .map(|(name, value)| (name.as_str().to_owned(), String::from_utf8_lossy(value.as_bytes()).into_owned()))
                .collect();
            let mut body = Vec::new();
            while body.len() < MAX_RESPONSE_BODY {
                match response.chunk().await {
                    Ok(Some(chunk)) => body.extend_from_slice(&chunk),
                    Ok(None) => break,
                    Err(error) => {
                        record.error = Some(describe(&error));
                        break;
                    }
                }
            }
            body.truncate(MAX_RESPONSE_BODY);
            record.response_body = Some(String::from_utf8_lossy(&body).into_owned());
            Ok(status.is_success())
        }
        Err(error) => Err(error),
    };
    record.duration_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);

    match result {
        Ok(succeeded) => Attempt { record, succeeded, permanent: false },
        Err(error) => {
            let permanent = is_refused(&error);
            record.error = Some(describe(&error));
            Attempt { record, succeeded: false, permanent }
        }
    }
}

/// The error and its causes on one line (reqwest's own message is terse).
fn describe(error: &reqwest::Error) -> String {
    let mut message = if error.is_timeout() { "timed out".to_owned() } else { error.to_string() };
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        let text = cause.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

/// Whether the address guard refused the connection.
fn is_refused(error: &reqwest::Error) -> bool {
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        if cause.downcast_ref::<RefusedAddress>().is_some() {
            return true;
        }
        source = cause.source();
    }
    false
}

/// `true` when `ip` may be contacted under the current policy.
pub fn may_contact(ip: IpAddr, allow_private: bool) -> bool {
    allow_private || !is_internal_address(ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_units_stay_in_range() {
        for _ in 0..100 {
            let unit = random_unit();
            assert!((0.0..1.0).contains(&unit));
        }
    }

    #[test]
    fn guids_look_like_uuids_and_differ() {
        let guid = new_guid();
        assert_eq!(guid.len(), 36);
        assert_eq!(guid.as_bytes()[14], b'4');
        assert_eq!(guid.matches('-').count(), 4);
        assert_ne!(guid, new_guid());
    }

    #[test]
    fn the_guard_drops_internal_addresses_unless_allowed() {
        let resolved: Vec<SocketAddr> =
            ["127.0.0.1:0", "10.0.0.5:0", "93.184.215.14:0", "[::1]:0", "[2606:4700::1111]:0", "[::ffff:192.168.1.1]:0"]
                .iter()
                .map(|address| address.parse().unwrap())
                .collect();
        let strict: Vec<String> = GuardedResolver::new(false).permitted(resolved.clone()).iter().map(ToString::to_string).collect();
        assert_eq!(strict, ["93.184.215.14:0", "[2606:4700::1111]:0"]);
        assert_eq!(GuardedResolver::new(true).permitted(resolved.clone()), resolved);
        assert!(!may_contact("169.254.169.254".parse().unwrap(), false));
        assert!(may_contact("169.254.169.254".parse().unwrap(), true));
    }

    #[tokio::test]
    async fn the_guard_refuses_names_that_resolve_internally() {
        let refused = GuardedResolver::new(false).resolve("localhost".parse().unwrap()).await;
        let error = refused.err().expect("localhost is internal");
        assert!(error.to_string().contains("internal addresses"), "{error}");
        let allowed = GuardedResolver::new(true).resolve("localhost".parse().unwrap()).await;
        assert!(allowed.is_ok_and(|mut addresses| addresses.next().is_some()));
    }
}
