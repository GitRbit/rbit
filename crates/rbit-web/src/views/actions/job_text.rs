//! One set of job/log labels per document, with borrowed English row statuses.
use super::StateLook;
use rbit_localization::{ActionsJobMessage as Message, Catalog, JobDisplay, Locale, LocalizedText};

#[derive(Debug)]
pub struct JobText {
    catalog: Catalog,
    pub details: LocalizedText,
    pub steps: LocalizedText,
    pub count: JobDisplay,
    pub runs_on: LocalizedText,
    pub duration: LocalizedText,
    pub raw_log: LocalizedText,
    pub rerun: LocalizedText,
    pub not_started: LocalizedText,
    pub not_started_description: LocalizedText,
    pub retained_window: LocalizedText,
    pub expired: LocalizedText,
    pub no_output: LocalizedText,
    pub earlier_lines: LocalizedText,
    pub show_more: LocalizedText,
    pub or: LocalizedText,
    pub download_all: LocalizedText,
    pub truncated: LocalizedText,
    pub step_not_started: LocalizedText,
}

/// A state's label in the catalog's locale, for job rows and the run heading alike.
pub(super) fn state_label(catalog: Catalog, look: &StateLook) -> LocalizedText {
    catalog.job(match look.key {
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

/// Same state key/icon/tone; only its plain, automatically escaped label changes.
#[derive(Debug)]
pub struct LocalizedState {
    pub key: &'static str,
    pub tone: &'static str,
    pub label: LocalizedText,
    locale: Locale,
}

impl LocalizedState {
    pub const fn language(&self) -> &'static str {
        self.locale.tag()
    }
}

impl JobText {
    pub fn new(locale: Locale, count: usize) -> Self {
        let catalog = Catalog::new(locale);
        Self {
            catalog,
            details: catalog.job(Message::Details),
            steps: catalog.job(Message::Steps),
            count: catalog.job_display(Message::StepCount { count: u64::try_from(count).unwrap_or(u64::MAX) }),
            runs_on: catalog.job(Message::RunsOn),
            duration: catalog.job(Message::Duration),
            raw_log: catalog.job(Message::RawLog),
            rerun: catalog.job(Message::Rerun),
            not_started: catalog.job(Message::NotStarted),
            not_started_description: catalog.job(Message::NotStartedDescription),
            retained_window: catalog.job(Message::RetainedWindow),
            expired: catalog.job(Message::Expired),
            no_output: catalog.job(Message::NoOutput),
            earlier_lines: catalog.job(Message::EarlierLines),
            show_more: catalog.job(Message::ShowMore),
            or: catalog.job(Message::Or),
            download_all: catalog.job(Message::DownloadAll),
            truncated: catalog.job(Message::Truncated),
            step_not_started: catalog.job(Message::StepNotStarted),
        }
    }

    pub fn look(&self, look: &StateLook) -> LocalizedState {
        LocalizedState { key: look.key, tone: look.tone, label: state_label(self.catalog, look), locale: self.catalog.locale() }
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
