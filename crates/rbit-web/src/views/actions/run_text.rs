//! Labels prepared once for the run's document locale; templates escape all text.
use rbit_localization::{ActionsMessage, Catalog, Locale, LocalizedText};

#[derive(Debug)]
pub struct RunText {
    pub actions: LocalizedText,
    pub workflow: LocalizedText,
    pub commit: LocalizedText,
    pub duration: LocalizedText,
    pub attempt: LocalizedText,
    pub triggered_by: LocalizedText,
    pub jobs: LocalizedText,
    pub artifacts: LocalizedText,
    pub details: LocalizedText,
    pub cancel: LocalizedText,
    pub rerun_failed: LocalizedText,
    pub rerun_all: LocalizedText,
    pub download_tar: LocalizedText,
    pub expires: LocalizedText,
}

impl RunText {
    pub fn new(locale: Locale) -> Self {
        let catalog = Catalog::new(locale);
        Self {
            actions: catalog.actions(ActionsMessage::Actions),
            workflow: catalog.actions(ActionsMessage::Workflow),
            commit: catalog.actions(ActionsMessage::Commit),
            duration: catalog.actions(ActionsMessage::Duration),
            attempt: catalog.actions(ActionsMessage::Attempt),
            triggered_by: catalog.actions(ActionsMessage::TriggeredBy),
            jobs: catalog.actions(ActionsMessage::Jobs),
            artifacts: catalog.actions(ActionsMessage::Artifacts),
            details: catalog.actions(ActionsMessage::RunDetails),
            cancel: catalog.actions(ActionsMessage::CancelRun),
            rerun_failed: catalog.actions(ActionsMessage::RerunFailed),
            rerun_all: catalog.actions(ActionsMessage::RerunAll),
            download_tar: catalog.actions(ActionsMessage::DownloadTar),
            expires: catalog.actions(ActionsMessage::Expires),
        }
    }
}
