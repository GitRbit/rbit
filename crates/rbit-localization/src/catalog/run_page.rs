//! Run page review, approval and job-dependency text. English only so far: every
//! other locale falls back (see [`super::fallback`]).
use super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunPageMessage<'a> {
    ApprovalTitle,
    ApprovalExplanation,
    TrustAuthor,
    ApproveAndRun,
    UntrustedNote,
    UntrustedApprovedNote {
        approver: &'a str,
    },
    DeploymentWaiting,
    /// `jobs` is the list of job names; `count` how many it names.
    DeploymentJobs {
        jobs: &'a str,
        count: u64,
        environment: &'a str,
    },
    Comment,
    Optional,
    ApproveAndDeploy,
    Reject,
    /// `jobs` is the list of jobs this job waits for.
    Needs {
        jobs: &'a str,
    },
}

impl MessageFamily for RunPageMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::ApprovalTitle => "actions.run.approval.title",
            Self::ApprovalExplanation => "actions.run.approval.explanation",
            Self::TrustAuthor => "actions.run.approval.trust-author",
            Self::ApproveAndRun => "actions.run.approval.approve",
            Self::UntrustedNote => "actions.run.untrusted",
            Self::UntrustedApprovedNote { .. } => "actions.run.untrusted-approved",
            Self::DeploymentWaiting => "actions.run.deployment.waiting",
            Self::DeploymentJobs { .. } => "actions.run.deployment.jobs",
            Self::Comment => "actions.run.deployment.comment",
            Self::Optional => "actions.run.deployment.optional",
            Self::ApproveAndDeploy => "actions.run.deployment.approve",
            Self::Reject => "actions.run.deployment.reject",
            Self::Needs { .. } => "actions.job.needs",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::ApprovalTitle => text.literal("This run waits for approval"),
            Self::ApprovalExplanation => text.literal(
                "It comes from a fork or from someone without write access. Its workflow may run code you have not \
                 reviewed, so a writer has to approve it first. Even approved, it runs with a read-only token and without \
                 secrets or variables.",
            ),
            Self::TrustAuthor => text.literal("Always run this author's pull requests without approval"),
            Self::ApproveAndRun => text.literal("Approve and run"),
            Self::UntrustedNote => {
                text.literal("This run comes from a fork or an outside author: it has a read-only token and no secrets or variables.");
            }
            Self::UntrustedApprovedNote { approver } => {
                text.literal(
                    "This run comes from a fork or an outside author: it has a read-only token and no secrets or variables, \
                     and was approved by ",
                );
                text.argument(approver);
                text.literal(".");
            }
            Self::DeploymentWaiting => text.literal("Waiting for review:"),
            Self::DeploymentJobs { jobs, count, environment } => {
                text.argument(jobs);
                text.literal(if count == 1 { " deploys to " } else { " deploy to " });
                text.argument(environment);
                text.literal(", whose reviewers must approve it first.");
            }
            Self::Comment => text.literal("Comment"),
            Self::Optional => text.literal("(optional)"),
            Self::ApproveAndDeploy => text.literal("Approve and deploy"),
            Self::Reject => text.literal("Reject"),
            Self::Needs { jobs } => {
                text.literal("needs ");
                text.argument(jobs);
            }
        }
    }
}
