//! Labels prepared once for the run's document locale; templates escape all text.
//! Review and approval text may be untranslated: it is [`Localized`], and templates
//! mark it English through `ui::localized` only when it fell back.
use rbit_localization::{ActionsMessage, Catalog, Locale, Localized, OverviewControlDisplay, OverviewControlMessage, RunPageMessage};

use super::{
    DeploymentView, StateLook,
    job_text::{LocalizedState, localized_state, state_label},
};

#[derive(Debug)]
pub struct RunText {
    catalog: Catalog,
    pub actions: Localized,
    pub workflow: Localized,
    pub commit: Localized,
    pub duration: Localized,
    pub attempt: Localized,
    pub triggered_by: Localized,
    pub jobs: Localized,
    pub artifacts: Localized,
    pub details: Localized,
    pub cancel: Localized,
    pub rerun_failed: Localized,
    pub rerun_all: Localized,
    pub download_tar: Localized,
    pub expires: Localized,
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
            actions: catalog.localized(ActionsMessage::Actions),
            workflow: catalog.localized(ActionsMessage::Workflow),
            commit: catalog.localized(ActionsMessage::Commit),
            duration: catalog.localized(ActionsMessage::Duration),
            attempt: catalog.localized(ActionsMessage::Attempt),
            triggered_by: catalog.localized(ActionsMessage::TriggeredBy),
            jobs: catalog.localized(ActionsMessage::Jobs),
            artifacts: catalog.localized(ActionsMessage::Artifacts),
            details: catalog.localized(ActionsMessage::RunDetails),
            cancel: catalog.localized(ActionsMessage::CancelRun),
            rerun_failed: catalog.localized(ActionsMessage::RerunFailed),
            rerun_all: catalog.localized(ActionsMessage::RerunAll),
            download_tar: catalog.localized(ActionsMessage::DownloadTar),
            expires: catalog.localized(ActionsMessage::Expires),
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

    /// A state icon's label in the page's locale.
    pub fn look(&self, look: &StateLook) -> LocalizedState {
        localized_state(self.catalog, look)
    }

    /// The run's state, as the job rows name it.
    pub fn state(&self, look: &StateLook) -> Localized {
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
