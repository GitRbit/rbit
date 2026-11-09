//! Messages of the environment pages that take arguments, and the protection summary of
//! an environment in the list.
use rbit_core::actions::{BranchPolicy, Environment};
use rbit_localization::{ActionsEnvironmentMessage as Message, Localized};

use super::ui_text::{Mark, Rich, SLOTS, UiText};

/// The workflow key that names an environment.
const ENVIRONMENT_KEY: &str = "environment:";
/// Pattern examples of the selected-branches hint, shown as code.
const PATTERN_EXAMPLES: [&str; 4] = ["release/*", "refs/tags/", "refs/tags/v*", "!"];

/// What protects an environment, as its row in the list summarizes it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvironmentProtection {
    pub reviewers: usize,
    pub protected_branches: bool,
    /// The selected branch and tag patterns, comma-separated.
    pub patterns: Option<String>,
}

impl From<&Environment> for EnvironmentProtection {
    fn from(environment: &Environment) -> Self {
        let (protected_branches, patterns) = match &environment.branch_policy {
            BranchPolicy::All => (false, None),
            BranchPolicy::Protected => (true, None),
            BranchPolicy::Selected(patterns) => (false, Some(patterns.join(", "))),
        };
        Self { reviewers: environment.reviewers.len(), protected_branches, patterns }
    }
}

impl UiText {
    pub fn environments_intro(self) -> Rich {
        self.rich(Message::Intro { key: SLOTS[0] }, &[Mark::code(ENVIRONMENT_KEY, "font-mono text-ink")])
    }

    pub fn no_environments_hint(self) -> Localized {
        self.get(Message::NoEnvironmentsHint { key: ENVIRONMENT_KEY })
    }

    /// "2 reviewers · branches main, release/*", or "No protection".
    pub fn protection(self, protection: &EnvironmentProtection) -> Rich {
        let mut parts = Vec::new();
        if protection.reviewers > 0 {
            parts.push(self.get(Message::Reviewers { count: u64::try_from(protection.reviewers).unwrap_or(u64::MAX) }));
        }
        if protection.protected_branches {
            parts.push(self.get(Message::ProtectedBranches));
        }
        if let Some(patterns) = &protection.patterns {
            parts.push(self.get(Message::Branches { patterns }));
        }
        if parts.is_empty() {
            parts.push(self.get(Message::NoProtection));
        }
        self.joined(parts)
    }

    pub fn patterns_hint(self) -> Rich {
        let [branch, tags, tag, exclude] = PATTERN_EXAMPLES.map(|example| Mark::code(example, "font-mono"));
        self.rich(
            Message::PatternsHint { branch: SLOTS[0], tags: SLOTS[1], tag: SLOTS[2], exclude: SLOTS[3] },
            &[branch, tags, tag, exclude],
        )
    }
}

#[cfg(test)]
mod tests {
    use rbit_localization::Locale;

    use super::*;

    fn summary(protection: &EnvironmentProtection) -> String {
        UiText::new(Locale::English).protection(protection).pieces.iter().map(|piece| piece.text.as_str()).collect()
    }

    #[test]
    fn the_summary_names_reviewers_and_branches_or_says_there_is_no_protection() {
        assert_eq!(summary(&EnvironmentProtection::default()), "No protection");
        let selected = EnvironmentProtection { reviewers: 2, protected_branches: false, patterns: Some("main, release/*".into()) };
        assert_eq!(summary(&selected), "2 reviewers · branches main, release/*");
        let protected = EnvironmentProtection { reviewers: 1, protected_branches: true, patterns: None };
        assert_eq!(summary(&protected), "1 reviewer · protected branches");
        let spanish = UiText::new(Locale::Spanish).protection(&protected);
        assert!(!spanish.fallback);
        assert_eq!(spanish.pieces.iter().map(|piece| piece.text.as_str()).collect::<String>(), "1 revisor · ramas protegidas");
        assert!(!UiText::new(Locale::German).protection(&protected).fallback);
    }
}
