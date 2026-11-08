//! Labels prepared once for the run's document locale; templates escape all text.
//! Review and approval text may be untranslated: it is [`Localized`], and templates
//! mark it English through `ui::localized` only when it fell back.
use rbit_localization::{
    ActionsMessage, Catalog, Locale, Localized, LocalizedText, OverviewControlDisplay, OverviewControlMessage, RunPageMessage,
};

use super::{DeploymentView, StateLook, job_text::state_label};

#[derive(Debug)]
pub struct RunText {
    catalog: Catalog,
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
    pub approval_title: Localized,
    pub approval_explanation: Localized,
    pub trust_author: Localized,
    pub approve_and_run: Localized,
    pub deployment_waiting: Localized,
    pub comment: Localized,
    pub optional: Localized,
    pub approve_and_deploy: Localized,
    pub reject: Localized,
}

impl RunText {
    pub fn new(locale: Locale) -> Self {
        let catalog = Catalog::new(locale);
        Self {
            catalog,
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
            approval_title: catalog.localized(RunPageMessage::ApprovalTitle),
            approval_explanation: catalog.localized(RunPageMessage::ApprovalExplanation),
            trust_author: catalog.localized(RunPageMessage::TrustAuthor),
            approve_and_run: catalog.localized(RunPageMessage::ApproveAndRun),
            deployment_waiting: catalog.localized(RunPageMessage::DeploymentWaiting),
            comment: catalog.localized(RunPageMessage::Comment),
            optional: catalog.localized(RunPageMessage::Optional),
            approve_and_deploy: catalog.localized(RunPageMessage::ApproveAndDeploy),
            reject: catalog.localized(RunPageMessage::Reject),
        }
    }

    /// The run's state, as the job rows name it.
    pub fn state(&self, look: &StateLook) -> LocalizedText {
        state_label(self.catalog, look)
    }

    /// What started the run, as the overview's event filter names it.
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "match Askama's borrowed enum field argument")]
    pub const fn event(&self, message: &OverviewControlMessage) -> OverviewControlDisplay {
        self.catalog.overview_control(*message)
    }

    /// The note on a run from a fork or an outside author.
    pub fn untrusted(&self, approver: Option<&str>) -> Localized {
        self.catalog.localized(match approver {
            Some(approver) => RunPageMessage::UntrustedApprovedNote { approver },
            None => RunPageMessage::UntrustedNote,
        })
    }

    /// Which jobs wait for an environment's reviewers.
    pub fn deployment_jobs(&self, deployment: &DeploymentView) -> Localized {
        self.catalog.localized(RunPageMessage::DeploymentJobs {
            jobs: &deployment.jobs,
            count: deployment.job_count,
            environment: &deployment.environment,
        })
    }

    /// The jobs a job waits for.
    pub fn needs(&self, jobs: &str) -> Localized {
        self.catalog.localized(RunPageMessage::Needs { jobs })
    }

    /// Text that only exists in English, such as a relative timestamp.
    pub fn english(&self, text: &str) -> Localized {
        Localized::english_only(text.to_owned(), self.catalog.locale())
    }
}
