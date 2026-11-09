//! Messages of the runners section that take arguments.
use crate::views::when::When;
use rbit_localization::{ActionsRunnersMessage as Message, Localized};

use super::ui_text::{Mark, Rich, SLOTS, UiText};
use crate::views::actions_runners::{RunnerRow, RunnersSection, RunnersSubject, TokenRow};

/// The runner program, named in commands and sentences.
const PROGRAM: &str = "rbit-runner";
/// The command that starts a registered runner.
const RUN_COMMAND: &str = "rbit-runner run";
/// The configuration key that opens instance runners to every owner.
const FOR_ALL_SETTING: &str = "instance_runners_for_all";
/// Label syntax examples.
const CONTAINER_LABEL: &str = "name:container:image";
const HOST_LABEL: &str = "name:host";

impl UiText {
    pub fn admin_runners_intro(self) -> Localized {
        self.get(Message::AdminIntro { setting: FOR_ALL_SETTING })
    }

    pub fn token_created_hint(self) -> Rich {
        self.rich(Message::TokenCreatedHint { command: SLOTS[0] }, &[Mark::code(RUN_COMMAND, "font-mono text-ink")])
    }

    pub fn labels_hint(self) -> Rich {
        self.rich(
            Message::LabelsHint { container: SLOTS[0], host: SLOTS[1] },
            &[Mark::code(CONTAINER_LABEL, "font-mono"), Mark::code(HOST_LABEL, "font-mono")],
        )
    }

    /// "Machines running rbit-runner that take the jobs of …".
    pub fn runners_intro(self, section: &RunnersSection) -> Rich {
        let program = SLOTS[0];
        let message = match &section.subject {
            RunnersSubject::Repository => Message::RepositoryIntro { program },
            RunnersSubject::Owner(owner) => Message::OwnerIntro { program, owner },
            RunnersSubject::Account => Message::AccountIntro { program },
            RunnersSubject::Instance => Message::InstanceIntro { program },
        };
        self.rich(message, &[Mark::code(PROGRAM, "font-mono text-ink")])
    }

    /// Where a runner was registered, unless it is the repository itself.
    pub fn runner_scope(self, runner: &RunnerRow) -> Option<Localized> {
        match runner.scope {
            "owner" => Some(self.get(Message::OwnerScope)),
            "instance" => Some(self.get(Message::InstanceScope)),
            _ => None,
        }
    }

    pub fn capacity(self, runner: &RunnerRow) -> Localized {
        self.get(Message::Capacity { count: u64::from(runner.capacity) })
    }

    pub fn seen(self, when: &When) -> Localized {
        let when = when.relative(self.locale());
        self.get(Message::Seen { when: when.as_str() })
    }

    pub fn remove_runner(self, name: &str) -> Localized {
        self.get(Message::RemoveRunner { name })
    }

    pub fn token_created(self, token: &TokenRow) -> Localized {
        let when = token.created.relative(self.locale());
        self.get(Message::Created { when: when.as_str() })
    }

    pub fn token_expires(self, token: &TokenRow) -> Localized {
        let when = token.expires.until(self.locale());
        self.get(Message::Expires { when: when.as_str() })
    }
}
