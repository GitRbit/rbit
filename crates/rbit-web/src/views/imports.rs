//! Importing from GitHub and Gitea: the form at `/new/import` and the progress page at
//! `/{owner}/{repo}/import`.
//!
//! Milestone 3 workstream M (importing and push mirrors) owns this file.

use askama::Template;
use askama_web::WebTemplate;
use rbit_core::import::{ImportPhase, ImportStatus, RepoImport, SourceKind};

use crate::views::{components::FormError, home::OwnerOption, layout::Page, repo::RepoHeader};

/// One choice of the "Import from" radio group.
#[derive(Debug, Clone)]
pub struct SourceChoice {
    pub value: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub checked: bool,
}

impl SourceChoice {
    pub fn all(selected: SourceKind) -> Vec<Self> {
        SourceKind::ALL
            .into_iter()
            .map(|kind| Self {
                value: kind.as_str(),
                label: kind.label(),
                hint: match kind {
                    SourceKind::GitHub => "github.com",
                    SourceKind::GitHubEnterprise => "Your company's GitHub Enterprise Server",
                    SourceKind::Gitea => "Any Gitea or Forgejo server, such as codeberg.org",
                },
                checked: kind == selected,
            })
            .collect()
    }
}

/// One "what to import" checkbox.
#[derive(Debug, Clone)]
pub struct OptionChoice {
    pub field: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub checked: bool,
}

/// `/new/import`
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/new_import.html")]
pub struct NewImportPage {
    pub page: Page,
    pub error: FormError,
    /// Importing is off on this instance: a notice replaces the form.
    pub disabled: bool,
    pub sources: Vec<SourceChoice>,
    pub host: String,
    pub repository: String,
    pub owners: Vec<OwnerOption>,
    pub name: String,
    pub private: bool,
    pub options: Vec<OptionChoice>,
}

/// One row of the progress table.
#[derive(Debug, Clone)]
pub struct PhaseRow {
    pub label: &'static str,
    /// `done`, `current`, `failed`, `skipped` or `waiting`.
    pub state: &'static str,
    /// What was brought over in this phase, e.g. "12 issues, 4 pull requests".
    pub detail: String,
}

/// `/{owner}/{repo}/import`
#[derive(Debug, Template, WebTemplate)]
#[template(path = "repo/import_status.html")]
pub struct ImportStatusPage {
    pub page: Page,
    pub header: RepoHeader,
    pub source_url: String,
    pub source_label: &'static str,
    /// Badge text and whether it is a status colour (`success`, `danger`) or neutral.
    pub status_label: &'static str,
    pub status_class: &'static str,
    pub active: bool,
    pub phases: Vec<PhaseRow>,
    pub warnings: Vec<String>,
    pub dropped_warnings: u64,
    pub error: Option<String>,
    /// `(relative, ISO 8601)` of the start and the end.
    pub started: Option<(String, String)>,
    pub finished: Option<(String, String)>,
    pub self_url: String,
}

fn plural(count: u64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

impl ImportStatusPage {
    /// The rows of the progress table for `import`: every phase it was asked for, in order.
    pub fn phases(import: &RepoImport) -> Vec<PhaseRow> {
        let options = import.options;
        let counts = import.counts;
        let wanted = |phase: ImportPhase| match phase {
            ImportPhase::Repository => true,
            ImportPhase::Lfs => options.lfs,
            ImportPhase::Labels => options.labels,
            ImportPhase::Milestones => options.milestones,
            ImportPhase::Issues => options.issues,
            ImportPhase::Releases => options.releases,
            ImportPhase::Finished => false,
        };
        ImportPhase::ALL
            .into_iter()
            .filter(|phase| wanted(*phase))
            .map(|phase| {
                let state = match (import.status, phase.cmp(&import.phase)) {
                    (ImportStatus::Succeeded, _) | (_, std::cmp::Ordering::Less) => "done",
                    (ImportStatus::Failed, std::cmp::Ordering::Equal) => "failed",
                    (ImportStatus::Failed, std::cmp::Ordering::Greater) => "skipped",
                    (ImportStatus::Running, std::cmp::Ordering::Equal) => "current",
                    (ImportStatus::Queued, _) | (ImportStatus::Running, std::cmp::Ordering::Greater) => "waiting",
                };
                let detail = match phase {
                    ImportPhase::Repository | ImportPhase::Finished => String::new(),
                    ImportPhase::Lfs => plural(counts.lfs_objects, "object", "objects"),
                    ImportPhase::Labels => plural(counts.labels, "label", "labels"),
                    ImportPhase::Milestones => plural(counts.milestones, "milestone", "milestones"),
                    ImportPhase::Issues => format!(
                        "{}, {}, {}",
                        plural(counts.issues, "issue", "issues"),
                        plural(counts.pulls, "pull request", "pull requests"),
                        plural(counts.comments, "comment", "comments")
                    ),
                    ImportPhase::Releases => {
                        format!("{}, {}", plural(counts.releases, "release", "releases"), plural(counts.assets, "file", "files"))
                    }
                };
                PhaseRow { label: phase.label(), state, detail }
            })
            .collect()
    }

    /// Badge text and class for a status.
    pub fn status_badge(status: ImportStatus) -> (&'static str, &'static str) {
        match status {
            ImportStatus::Queued => ("Queued", ""),
            ImportStatus::Running => ("Running", ""),
            ImportStatus::Succeeded => ("Finished", "border-success/40 text-success"),
            ImportStatus::Failed => ("Failed", "border-danger/40 text-danger"),
        }
    }
}

#[cfg(test)]
mod tests {
    use rbit_core::{
        RepoId, Timestamp,
        import::{ImportCounts, ImportOptions},
    };

    use super::*;

    fn import(status: ImportStatus, phase: ImportPhase) -> RepoImport {
        RepoImport {
            repo_id: RepoId(1),
            importer: None,
            kind: SourceKind::GitHub,
            source_url: "https://github.com/a/b".into(),
            options: ImportOptions { lfs: false, ..ImportOptions::default() },
            status,
            phase,
            counts: ImportCounts { issues: 1, pulls: 2, comments: 3, ..ImportCounts::default() },
            warnings: Vec::new(),
            error: None,
            created_at: Timestamp(0),
            started_at: None,
            finished_at: None,
        }
    }

    #[test]
    fn phases_show_where_an_import_is() {
        let running = ImportStatusPage::phases(&import(ImportStatus::Running, ImportPhase::Issues));
        let states: Vec<_> = running.iter().map(|row| (row.label, row.state)).collect();
        assert_eq!(
            states,
            [
                ("Copying the Git repository", "done"),
                ("Importing labels", "done"),
                ("Importing milestones", "done"),
                ("Importing issues and pull requests", "current"),
                ("Importing releases", "waiting"),
            ],
            "LFS was not asked for"
        );
        assert_eq!(running[3].detail, "1 issue, 2 pull requests, 3 comments");
        let failed = ImportStatusPage::phases(&import(ImportStatus::Failed, ImportPhase::Labels));
        assert_eq!(failed.iter().map(|row| row.state).collect::<Vec<_>>(), ["done", "failed", "skipped", "skipped", "skipped"]);
        let done = ImportStatusPage::phases(&import(ImportStatus::Succeeded, ImportPhase::Finished));
        assert!(done.iter().all(|row| row.state == "done"));
        assert_eq!(ImportStatusPage::status_badge(ImportStatus::Failed).0, "Failed");
        assert_eq!(SourceChoice::all(SourceKind::Gitea).iter().filter(|c| c.checked).count(), 1);
    }
}
