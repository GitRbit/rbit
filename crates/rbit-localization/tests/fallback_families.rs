//! Every key of the English-first families exists, renders in every catalog and falls
//! back to English wherever a locale has not translated it.
use std::collections::BTreeSet;

use rbit_localization::{Catalog, Locale, MessageFamily, RunPageMessage};

const LOCALES: [Locale; 4] = [Locale::English, Locale::Spanish, Locale::PseudoExpanded, Locale::PseudoRtl];

/// Every key has a distinct id and renders non-empty text in every locale; English and
/// the diagnostic catalogs never report a fallback.
fn check_family<M: MessageFamily + std::fmt::Debug>(messages: &[M], distinct_ids: usize) {
    let ids: BTreeSet<_> = messages.iter().map(|message| message.id()).collect();
    assert_eq!(ids.len(), distinct_ids, "ids are unique");
    for &message in messages {
        for locale in LOCALES {
            let text = Catalog::new(locale).localized(message);
            assert!(!text.as_str().is_empty(), "{message:?} in {locale:?}");
            if locale != Locale::Spanish {
                assert!(!text.fallback(), "{message:?} derives from English in {locale:?}");
            }
        }
    }
}

fn run_page() -> Vec<RunPageMessage<'static>> {
    use RunPageMessage as M;
    vec![
        M::ApprovalTitle,
        M::ApprovalExplanation,
        M::TrustAuthor,
        M::ApproveAndRun,
        M::UntrustedNote,
        M::UntrustedApprovedNote { approver: "alice" },
        M::DeploymentWaiting,
        M::DeploymentJobs { jobs: "deploy", count: 1, environment: "production" },
        M::DeploymentJobs { jobs: "deploy, smoke test", count: 2, environment: "production" },
        M::Comment,
        M::Optional,
        M::ApproveAndDeploy,
        M::Reject,
        M::Needs { jobs: "build, test" },
    ]
}

#[test]
fn run_page_keys_exist_render_and_fall_back_in_spanish() {
    check_family(&run_page(), 13);
    let spanish = Catalog::new(Locale::Spanish);
    for message in run_page() {
        let english = Catalog::new(Locale::English).localized(message);
        let fallback = spanish.localized(message);
        assert_eq!(fallback.as_str(), english.as_str(), "{message:?}");
        assert!(fallback.fallback(), "{message:?}");
    }
    let approved = spanish.localized(RunPageMessage::UntrustedApprovedNote { approver: "<alice>" });
    assert!(approved.as_str().ends_with("approved by <alice>."), "arguments stay plain text: {approved}");
}

#[test]
fn deployment_text_chooses_the_verb_by_job_count() {
    let english = Catalog::new(Locale::English);
    let one = english.localized(RunPageMessage::DeploymentJobs { jobs: "deploy", count: 1, environment: "prod" });
    assert_eq!(one.as_str(), "deploy deploys to prod, whose reviewers must approve it first.");
    let two = english.localized(RunPageMessage::DeploymentJobs { jobs: "deploy, smoke", count: 2, environment: "prod" });
    assert_eq!(two.as_str(), "deploy, smoke deploy to prod, whose reviewers must approve it first.");
}
