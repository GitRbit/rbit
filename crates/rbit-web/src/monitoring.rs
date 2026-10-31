//! Small, process-local dashboard telemetry. The request path only updates atomics;
//! a weakly-owned sampler keeps at most five minutes of five-second observations.
//! No database queries, labels, raw paths or per-request allocations are retained.

use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
pub const HISTORY_LIMIT: usize = 60;

#[derive(Debug, Default, Clone, Copy)]
pub struct Totals {
    pub responses: [u64; 5],
    pub duration_us: u64,
    /// Smart-HTTP exchange responses: clone/fetch, push. SSH is not included.
    pub git: [u64; 2],
}

impl Totals {
    pub fn requests(self) -> u64 {
        self.responses.iter().sum()
    }

    fn since(self, previous: Self) -> Self {
        Self {
            responses: std::array::from_fn(|i| self.responses[i].saturating_sub(previous.responses[i])),
            duration_us: self.duration_us.saturating_sub(previous.duration_us),
            git: std::array::from_fn(|i| self.git[i].saturating_sub(previous.git[i])),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub timestamp: u64,
    pub seconds: f64,
    pub totals: Totals,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub uptime: Duration,
    pub samples: Vec<Sample>,
}

#[derive(Debug)]
struct History {
    previous: Totals,
    sampled_at: Instant,
    samples: VecDeque<Sample>,
}

#[derive(Debug)]
pub struct Monitor {
    started: Instant,
    sampling: AtomicBool,
    responses: [AtomicU64; 5],
    duration_us: AtomicU64,
    git: [AtomicU64; 2],
    history: Mutex<History>,
}

impl Monitor {
    pub fn new() -> Arc<Self> {
        let now = Instant::now();
        Arc::new(Self {
            started: now,
            sampling: AtomicBool::new(false),
            responses: std::array::from_fn(|_| AtomicU64::new(0)),
            duration_us: AtomicU64::new(0),
            git: std::array::from_fn(|_| AtomicU64::new(0)),
            history: Mutex::new(History { previous: Totals::default(), sampled_at: now, samples: VecDeque::with_capacity(HISTORY_LIMIT) }),
        })
    }

    /// Called once when constructing the router. Dropping the router/state releases
    /// the monitor; the task never keeps the instance alive between ticks.
    pub fn start(self: &Arc<Self>) {
        if self.sampling.swap(true, Ordering::Relaxed) {
            return;
        }
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SAMPLE_INTERVAL).await;
                let Some(monitor) = weak.upgrade() else { break };
                monitor.sample(Instant::now(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs());
            }
        });
    }

    pub fn record(&self, status: u16, duration: Duration, git: Option<usize>) {
        // Like Prometheus counters, independent atomic reads are approximate at a
        // sampling boundary. Never serialize ordinary requests on the history lock.
        self.duration_us.fetch_add(u64::try_from(duration.as_micros()).unwrap_or(u64::MAX), Ordering::Relaxed);
        self.responses[usize::from(status / 100).clamp(1, 5) - 1].fetch_add(1, Ordering::Relaxed);
        if let Some(index) = git {
            self.git[index].fetch_add(1, Ordering::Relaxed);
        }
    }

    fn totals(&self) -> Totals {
        Totals {
            responses: std::array::from_fn(|i| self.responses[i].load(Ordering::Relaxed)),
            duration_us: self.duration_us.load(Ordering::Relaxed),
            git: std::array::from_fn(|i| self.git[i].load(Ordering::Relaxed)),
        }
    }

    fn sample(&self, now: Instant, timestamp: u64) {
        let mut history = self.history.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let totals = self.totals();
        let sample =
            Sample { timestamp, seconds: now.duration_since(history.sampled_at).as_secs_f64(), totals: totals.since(history.previous) };
        history.previous = totals;
        history.sampled_at = now;
        if history.samples.len() == HISTORY_LIMIT {
            history.samples.pop_front();
        }
        history.samples.push_back(sample);
    }

    pub fn snapshot(&self) -> Snapshot {
        let history = self.history.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Snapshot { uptime: self.started.elapsed(), samples: history.samples.iter().cloned().collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_deltas_idle_periods_and_bounded_history() {
        let monitor = Monitor::new();
        monitor.record(200, Duration::from_millis(10), Some(0));
        monitor.record(503, Duration::from_millis(30), Some(1));
        monitor.sample(monitor.started + Duration::from_secs(5), 5);
        let snapshot = monitor.snapshot();
        let first = &snapshot.samples[0];
        assert_eq!(first.totals.requests(), 2);
        assert_eq!(first.totals.responses, [0, 1, 0, 0, 1]);
        assert_eq!(first.totals.duration_us, 40_000);
        assert_eq!(first.totals.git, [1, 1]);
        assert_eq!(first.seconds, 5.0);
        for i in 2..=65 {
            monitor.sample(monitor.started + Duration::from_secs(i * 5), i * 5);
        }
        let snapshot = monitor.snapshot();
        assert_eq!(snapshot.samples.len(), HISTORY_LIMIT);
        assert_eq!(snapshot.samples[0].timestamp, 30);
        assert!(snapshot.samples.iter().all(|sample| sample.totals.requests() == 0));
    }

    #[tokio::test]
    async fn middleware_excludes_monitoring_and_counts_failed_git_exchanges() {
        use axum::{Router, body::Body, http::Request, middleware::from_fn_with_state, routing::get};
        use tower::ServiceExt;

        let monitor = Monitor::new();
        let app = Router::new()
            .route("/admin/monitoring", get(|| async { "snapshot" }))
            .route("/metrics", get(|| async { "metrics" }))
            .route("/{owner}/{repo}/git-upload-pack", get(|| async { axum::http::StatusCode::BAD_GATEWAY }))
            .route("/{owner}/{repo}/info/refs", get(|| async { "advertisement" }))
            .layer(from_fn_with_state(monitor.clone(), crate::metrics::track));
        for path in ["/admin/monitoring", "/metrics", "/a/b/git-upload-pack", "/a/b/info/refs?service=git-upload-pack"] {
            app.clone().oneshot(Request::builder().uri(path).body(Body::empty()).unwrap()).await.unwrap();
        }
        let totals = monitor.totals();
        assert_eq!(totals.responses, [0, 1, 0, 0, 1]);
        assert_eq!(totals.git, [1, 0]);
    }
}
