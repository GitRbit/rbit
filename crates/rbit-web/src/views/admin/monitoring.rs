//! Presentation of bounded, in-memory monitoring samples. SVGs use the site's tokens.
use askama::Template;
use askama_web::WebTemplate;

use crate::monitoring::{HISTORY_LIMIT, Sample, Snapshot, Totals};

#[expect(clippy::cast_precision_loss, reason = "rates and chart coordinates are approximate; exact counts stay u64")]
fn approximate(value: u64) -> f64 {
    value as f64
}

#[derive(Debug)]
pub struct Chart {
    pub title: &'static str,
    pub unit: &'static str,
    pub value: String,
    pub ceiling: String,
    pub path: String,
    pub last_x: f64,
    pub last_y: f64,
    pub has_point: bool,
}

impl Chart {
    fn new(title: &'static str, unit: &'static str, samples: &[Sample], value: impl Fn(&Sample) -> Option<f64>) -> Self {
        use std::fmt::Write;
        let values: Vec<_> = samples.iter().map(&value).collect();
        let peak = values.iter().flatten().copied().fold(1.0_f64, f64::max);
        let latest = samples.last().map_or(0, |sample| sample.timestamp);
        let mut path = String::new();
        let mut connected = false;
        let mut last_x = 0.0;
        let mut last_y = 0.0;
        for (sample, value) in samples.iter().zip(&values) {
            let Some(value) = value else {
                connected = false;
                continue;
            };
            let age = u32::try_from(latest.saturating_sub(sample.timestamp).min(300)).unwrap_or(300);
            let x = 10.0 + (300.0 - f64::from(age)) / 300.0 * 580.0;
            let y = 125.0 - value / peak * 110.0;
            let _ = write!(path, "{}{x:.1},{y:.1} ", if connected { 'L' } else { 'M' });
            connected = true;
            last_x = x;
            last_y = y;
        }
        Self {
            title,
            unit,
            value: values.last().copied().flatten().map_or_else(|| "—".to_owned(), |v| format!("{v:.1}")),
            ceiling: format!("{peak:.1}"),
            path,
            last_x,
            last_y,
            has_point: values.last().is_some_and(Option::is_some),
        }
    }
}

#[derive(Debug)]
pub struct ResponseRow {
    pub label: &'static str,
    pub meaning: &'static str,
    pub count: u64,
    pub share: String,
    pub width: u32,
}

#[derive(Debug, Template, WebTemplate)]
#[template(path = "admin/monitoring.html")]
pub struct MonitoringPanel {
    pub charts: Vec<Chart>,
    pub responses: Vec<ResponseRow>,
    pub git: [u64; 2],
    pub requests: u64,
    pub errors: String,
    pub uptime: String,
    pub coverage: String,
    pub ready: bool,
    pub updated: String,
}

impl MonitoringPanel {
    pub fn new(snapshot: &Snapshot) -> Self {
        let samples = &snapshot.samples;
        let totals = samples.iter().fold(Totals::default(), |mut total, sample| {
            for i in 0..5 {
                total.responses[i] += sample.totals.responses[i];
            }
            for i in 0..2 {
                total.git[i] += sample.totals.git[i];
            }
            total
        });
        let requests = totals.requests();
        let percent =
            |n: u64| if requests == 0 { "—".to_owned() } else { format!("{:.1}%", approximate(n) / approximate(requests) * 100.0) };
        let responses =
            [("1xx", "Informational"), ("2xx", "Successful"), ("3xx", "Redirects"), ("4xx", "Client errors"), ("5xx", "Server errors")]
                .into_iter()
                .enumerate()
                .map(|(i, (label, meaning))| ResponseRow {
                    label,
                    meaning,
                    count: totals.responses[i],
                    share: percent(totals.responses[i]),
                    width: if requests == 0 {
                        0
                    } else {
                        u32::try_from((u128::from(totals.responses[i]) * 100 + u128::from(requests) / 2) / u128::from(requests))
                            .unwrap_or(100)
                    },
                })
                .collect();
        let updated = samples.last().map_or_else(
            || "Waiting for first sample".to_owned(),
            |sample| {
                let seconds = sample.timestamp % 86_400;
                format!("Updated {:02}:{:02}:{:02} UTC", seconds / 3600, seconds / 60 % 60, seconds % 60)
            },
        );
        Self {
            charts: vec![
                Chart::new("Request traffic", "req/s", samples, |s| Some(approximate(s.totals.requests()) / s.seconds.max(0.001))),
                Chart::new("Response time", "ms avg", samples, |s| {
                    (s.totals.requests() > 0).then(|| approximate(s.totals.duration_us) / approximate(s.totals.requests()) / 1000.0)
                }),
            ],
            responses,
            git: totals.git,
            requests,
            errors: percent(totals.responses[4]),
            uptime: super::uptime(snapshot.uptime),
            coverage: format!("{} of {HISTORY_LIMIT} samples", samples.len()),
            ready: !samples.is_empty(),
            updated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn empty_and_idle_are_distinct_from_measured_latency() {
        let panel = MonitoringPanel::new(&Snapshot { uptime: Duration::ZERO, samples: vec![] });
        let html = panel.render().unwrap();
        assert!(html.contains("Waiting for first sample"));
        assert_eq!(panel.errors, "—");
        let samples = vec![
            Sample { timestamp: 5, seconds: 5.0, totals: Totals { responses: [0, 9, 0, 0, 1], duration_us: 200_000, git: [3, 1] } },
            Sample { timestamp: 10, seconds: 5.0, totals: Totals::default() },
        ];
        let panel = MonitoringPanel::new(&Snapshot { uptime: Duration::from_secs(10), samples });
        assert_eq!(panel.charts[0].value, "0.0");
        assert_eq!(panel.charts[1].value, "—");
        assert!(!panel.charts[1].has_point);
        assert_eq!(panel.errors, "10.0%");
        assert_eq!(panel.git, [3, 1]);
        assert!(panel.charts[0].path.contains('L'));
        assert!(!panel.charts[1].path.contains('L'), "idle latency must be a gap");
        assert_eq!(panel.responses[1].width, 90);
    }
}
