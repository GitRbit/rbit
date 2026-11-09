//! The phase plan of one import and its progress bookkeeping: what is counted, when it
//! is saved and which warnings are kept.
use super::model::{ImportCounts, ImportOptions, ImportPhase, MAX_IMPORT_WARNINGS, clean_message};

/// Progress is saved at least this often while items are imported.
pub const SAVE_EVERY: u64 = 25;

/// The phases an import with `options` goes through after copying the Git repository,
/// in order. LFS objects are copied only while the instance serves LFS.
pub fn phase_plan(options: ImportOptions, lfs_enabled: bool) -> Vec<ImportPhase> {
    let wanted = [
        (options.lfs && lfs_enabled, ImportPhase::Lfs),
        (options.labels, ImportPhase::Labels),
        (options.milestones, ImportPhase::Milestones),
        (options.issues, ImportPhase::Issues),
        (options.releases, ImportPhase::Releases),
    ];
    wanted.into_iter().filter_map(|(wanted, phase)| wanted.then_some(phase)).collect()
}

/// What a running import has brought over, and what it still has to save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportProgress {
    pub phase: ImportPhase,
    pub counts: ImportCounts,
    /// At most [`MAX_IMPORT_WARNINGS`]; the rest are counted in `counts`.
    pub warnings: Vec<String>,
    unsaved: u64,
}

impl Default for ImportProgress {
    fn default() -> Self {
        Self { phase: ImportPhase::Repository, counts: ImportCounts::default(), warnings: Vec::new(), unsaved: 0 }
    }
}

impl ImportProgress {
    /// Enters `phase`; the caller saves now.
    pub fn enter(&mut self, phase: ImportPhase) {
        self.phase = phase;
        self.saved();
    }

    /// Marks everything saved.
    pub fn saved(&mut self) {
        self.unsaved = 0;
    }

    /// Counts one imported item; `true` when the caller should save now.
    pub fn tick(&mut self) -> bool {
        self.unsaved += 1;
        self.unsaved >= SAVE_EVERY
    }

    /// Keeps a cleaned warning (or counts it once the cap is reached) and returns it,
    /// for the log.
    pub fn warn(&mut self, message: &str) -> String {
        let message = clean_message(message);
        if self.warnings.len() < MAX_IMPORT_WARNINGS {
            self.warnings.push(message.clone());
        } else {
            self.counts.dropped_warnings += 1;
        }
        message
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plan_follows_the_options_and_lfs_needs_the_instance() {
        let all = ImportOptions::default();
        assert_eq!(
            phase_plan(all, true),
            [ImportPhase::Lfs, ImportPhase::Labels, ImportPhase::Milestones, ImportPhase::Issues, ImportPhase::Releases]
        );
        assert_eq!(phase_plan(all, false)[0], ImportPhase::Labels);
        let none = ImportOptions { issues: false, labels: false, milestones: false, releases: false, lfs: false };
        assert_eq!(phase_plan(none, true), []);
    }

    #[test]
    fn progress_saves_every_25_items_and_caps_warnings() {
        let mut progress = ImportProgress::default();
        assert!((1..SAVE_EVERY).all(|_| !progress.tick()));
        assert!(progress.tick());
        assert!(progress.tick(), "unsaved until the caller saves");
        progress.enter(ImportPhase::Labels);
        assert!(!progress.tick());
        for index in 0..MAX_IMPORT_WARNINGS + 3 {
            assert_eq!(progress.warn(&format!(" w{index}\u{7} ")), format!("w{index}"));
        }
        assert_eq!((progress.warnings.len(), progress.counts.dropped_warnings), (MAX_IMPORT_WARNINGS, 3));
    }
}
