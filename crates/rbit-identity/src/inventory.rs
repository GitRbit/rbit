//! Offline metadata only: bounded PHC parsing, never password verification or Argon work.
//! Values are observations, not acceptance/rejection policy or an exhaustive validity proof.
use argon2::{Params, PasswordHash};
use serde::Serialize;
use std::collections::BTreeMap;

mod classify;
pub use classify::{Algorithm, Classification, Parameters, classify};

/// Hard scanner bounds, unrelated to any password/hash acceptance limit.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Limits {
    pub rows: u32,
    pub prefix_bytes: u16,
    pub groups: u16,
}
impl Default for Limits {
    fn default() -> Self {
        Self { rows: 1_000_000, prefix_bytes: 1024, groups: 256 }
    }
}
impl Limits {
    pub fn valid(self) -> bool {
        (1..=1_000_000).contains(&self.rows) && (128..=4096).contains(&self.prefix_bytes) && (1..=4096).contains(&self.groups)
    }
}
#[derive(Debug, Default, Clone, Copy, Serialize)]
pub struct Lengths {
    pub minimum: Option<u64>,
    pub maximum: Option<u64>,
}
impl Lengths {
    fn record(&mut self, value: u64) {
        self.minimum = Some(self.minimum.map_or(value, |old| old.min(value)));
        self.maximum = Some(self.maximum.map_or(value, |old| old.max(value)));
    }
}
#[derive(Debug, Default, Serialize)]
pub struct Counts {
    pub parameters_observed: u32,
    pub incomplete: u32,
    pub malformed: u32,
    pub unsupported: u32,
    pub oversized: u32,
    pub invalid_storage: u32,
    pub groups_omitted: u32,
}
#[derive(Debug, Serialize)]
pub struct Group {
    pub parameters: Parameters,
    pub rows: u32,
}
#[derive(Debug, Serialize)]
pub struct Report {
    pub schema: u8,
    pub complete: bool,
    pub row_limit_reached: bool,
    pub limits: Limits,
    pub rows: u32,
    pub counts: Counts,
    pub phc_bytes: Lengths,
    pub salt_bytes: Lengths,
    pub digest_bytes: Lengths,
    pub groups: Vec<Group>,
}
/// Retains only enum/numeric metadata. Raw credential input is never stored or formatted.
#[derive(Debug)]
pub struct Inventory {
    report: Report,
    groups: BTreeMap<Parameters, u32>,
}
impl Inventory {
    pub fn new(limits: Limits) -> Option<Self> {
        limits.valid().then(|| Self {
            report: Report {
                schema: 1,
                complete: true,
                row_limit_reached: false,
                limits,
                rows: 0,
                counts: Counts::default(),
                phc_bytes: Lengths::default(),
                salt_bytes: Lengths::default(),
                digest_bytes: Lengths::default(),
                groups: Vec::new(),
            },
            groups: BTreeMap::new(),
        })
    }
    /// Returns false on the first row beyond the bound, without counting that row.
    /// `None` length indicates non-TEXT storage; SQL adapters must bound `prefix` first.
    pub fn observe(&mut self, prefix: &[u8], byte_length: Option<u64>) -> bool {
        if self.report.rows == self.report.limits.rows {
            self.report.complete = false;
            self.report.row_limit_reached = true;
            return false;
        }
        self.report.rows += 1;
        let Some(length) = byte_length else {
            self.report.counts.invalid_storage += 1;
            return true;
        };
        self.report.phc_bytes.record(length);
        let classified = if length > u64::from(self.report.limits.prefix_bytes) {
            Classification::Oversized
        } else if length != prefix.len() as u64 {
            Classification::Incomplete
        } else {
            classify(prefix, usize::from(self.report.limits.prefix_bytes))
        };
        match classified {
            Classification::Parameters { parameters, salt_bytes, digest_bytes } => {
                self.report.counts.parameters_observed += 1;
                self.report.salt_bytes.record(salt_bytes);
                self.report.digest_bytes.record(digest_bytes);
                if let Some(rows) = self.groups.get_mut(&parameters) {
                    *rows += 1;
                } else if self.groups.len() < usize::from(self.report.limits.groups) {
                    self.groups.insert(parameters, 1);
                } else {
                    self.report.counts.groups_omitted += 1;
                    self.report.complete = false;
                }
            }
            Classification::Incomplete => self.report.counts.incomplete += 1,
            Classification::Malformed => self.report.counts.malformed += 1,
            Classification::Unsupported => self.report.counts.unsupported += 1,
            Classification::Oversized => self.report.counts.oversized += 1,
        }
        true
    }
    pub fn finish(mut self) -> Report {
        self.report.groups = self.groups.into_iter().map(|(parameters, rows)| Group { parameters, rows }).collect();
        self.report
    }
}
