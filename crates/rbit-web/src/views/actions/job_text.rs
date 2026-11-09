//! One set of job/log labels per document, with borrowed English row statuses.
use super::StateLook;
use rbit_localization::{ActionsJobMessage as Message, Catalog, JobDisplay, Locale, Localized};

#[derive(Debug)]
pub struct JobText {
    catalog: Catalog,
    pub details: Localized,
    pub steps: Localized,
    pub count: JobDisplay,
    pub runs_on: Localized,
    pub duration: Localized,
    pub raw_log: Localized,
    pub rerun: Localized,
    pub not_started: Localized,
    pub not_started_description: Localized,
    pub retained_window: Localized,
    pub expired: Localized,
    pub no_output: Localized,
    pub earlier_lines: Localized,
    pub show_more: Localized,
    pub or: Localized,
    pub download_all: Localized,
    pub truncated: Localized,
    pub step_not_started: Localized,
}

/// A state's label in the catalog's locale, for job rows and the run heading alike.
pub(super) fn state_label(catalog: Catalog, look: &StateLook) -> Localized {
    catalog.localized(match look.key {
        "success" => Message::Success,
        "failure" => Message::Failure,
        "cancelled" => Message::Cancelled,
        "skipped" => Message::Skipped,
        "queued" => Message::Queued,
        "running" => Message::Running,
        "waiting" => Message::Waiting,
        "approval" => Message::Approval,
        _ => Message::WorkflowError,
    })
}

/// A state as `actions/state.html` shows it, labelled in the catalog's locale.
pub(super) fn localized_state(catalog: Catalog, look: &StateLook) -> LocalizedState {
    LocalizedState { key: look.key, tone: look.tone, label: state_label(catalog, look), locale: catalog.locale() }
}

/// Same state key/icon/tone; only its plain, automatically escaped label changes.
#[derive(Debug)]
pub struct LocalizedState {
    pub key: &'static str,
    pub tone: &'static str,
    pub label: Localized,
    locale: Locale,
}

impl LocalizedState {
    /// The language of the label: the page's, or English where it is untranslated.
    pub fn language(&self) -> &'static str {
        self.label.marked_lang().unwrap_or(self.locale.tag())
    }
}

impl JobText {
    pub fn new(locale: Locale, count: usize) -> Self {
        let catalog = Catalog::new(locale);
        Self {
            catalog,
            details: catalog.localized(Message::Details),
            steps: catalog.localized(Message::Steps),
            count: catalog.job_display(Message::StepCount { count: u64::try_from(count).unwrap_or(u64::MAX) }),
            runs_on: catalog.localized(Message::RunsOn),
            duration: catalog.localized(Message::Duration),
            raw_log: catalog.localized(Message::RawLog),
            rerun: catalog.localized(Message::Rerun),
            not_started: catalog.localized(Message::NotStarted),
            not_started_description: catalog.localized(Message::NotStartedDescription),
            retained_window: catalog.localized(Message::RetainedWindow),
            expired: catalog.localized(Message::Expired),
            no_output: catalog.localized(Message::NoOutput),
            earlier_lines: catalog.localized(Message::EarlierLines),
            show_more: catalog.localized(Message::ShowMore),
            or: catalog.localized(Message::Or),
            download_all: catalog.localized(Message::DownloadAll),
            truncated: catalog.localized(Message::Truncated),
            step_not_started: catalog.localized(Message::StepNotStarted),
        }
    }

    pub fn look(&self, look: &StateLook) -> LocalizedState {
        localized_state(self.catalog, look)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbit_core::actions::Status;

    #[test]
    fn localization_keeps_every_state_key_tone_and_english_label() {
        let english = JobText::new(Locale::English, 512);
        let rtl = JobText::new(Locale::PseudoRtl, 512);
        for source in Status::ALL.into_iter().map(StateLook::of).chain([StateLook::WORKFLOW_ERROR]) {
            let localized = english.look(&source);
            assert_eq!(localized.key, source.key);
            assert_eq!(localized.tone, source.tone);
            assert_eq!(localized.label.as_str(), source.label);
            assert_eq!(rtl.look(&source).key, source.key);
            assert_ne!(rtl.look(&source).label.as_str(), source.label);
        }
        assert_eq!(english.count.to_string(), "512 steps");
    }
}
