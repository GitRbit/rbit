//! Runners and registration tokens, for every scope that manages them.
use super::super::fallback::{MessageFamily, MessageWriter};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionsRunnersMessage<'a> {
    Title,
    /// `setting` is the configuration key that opens instance runners to every owner.
    AdminIntro {
        setting: &'a str,
    },
    TokenCreated,
    /// `command` starts a registered runner.
    TokenCreatedHint {
        command: &'a str,
    },
    Token,
    CopyToken,
    Copy,
    Command,
    CopyCommand,
    /// `container` and `host` are label syntax examples.
    LabelsHint {
        container: &'a str,
        host: &'a str,
    },
    /// `program` is the runner program's name.
    RepositoryIntro {
        program: &'a str,
    },
    OwnerIntro {
        program: &'a str,
        owner: &'a str,
    },
    AccountIntro {
        program: &'a str,
    },
    InstanceIntro {
        program: &'a str,
    },
    NoRunners,
    NoRunnersHint,
    Online,
    Offline,
    Disabled,
    NeverSeen,
    OwnerScope,
    InstanceScope,
    Ephemeral,
    HostNoIsolation,
    HostWarning,
    Capacity {
        count: u64,
    },
    /// `when` is a relative time.
    Seen {
        when: &'a str,
    },
    Enable,
    Disable,
    Remove,
    RemoveRunner {
        name: &'a str,
    },
    TokensTitle,
    TokensIntro,
    ExpiresAfter,
    OnlyTheseLabels,
    /// Example runner labels.
    LabelsPlaceholder,
    Optional,
    Reusable,
    NewToken,
    Created {
        when: &'a str,
    },
    Expires {
        when: &'a str,
    },
    NoLongerUsable,
    Revoked,
    Expired,
    UnlimitedUses,
    UsedUp,
    UsesLeft {
        count: u64,
    },
    Revoke,
    Hours {
        count: u64,
    },
    Days {
        count: u64,
    },
}

impl MessageFamily for ActionsRunnersMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::Title => "actions.runners.title",
            Self::AdminIntro { .. } => "actions.runners.admin-intro",
            Self::TokenCreated => "actions.runners.issued.title",
            Self::TokenCreatedHint { .. } => "actions.runners.issued.hint",
            Self::Token => "actions.runners.issued.token",
            Self::CopyToken => "actions.runners.issued.copy-token",
            Self::Copy => "actions.runners.issued.copy",
            Self::Command => "actions.runners.issued.command",
            Self::CopyCommand => "actions.runners.issued.copy-command",
            Self::LabelsHint { .. } => "actions.runners.issued.labels-hint",
            Self::RepositoryIntro { .. } => "actions.runners.intro.repository",
            Self::OwnerIntro { .. } => "actions.runners.intro.owner",
            Self::AccountIntro { .. } => "actions.runners.intro.account",
            Self::InstanceIntro { .. } => "actions.runners.intro.instance",
            Self::NoRunners => "actions.runners.empty",
            Self::NoRunnersHint => "actions.runners.empty-hint",
            Self::Online => "actions.runners.state.online",
            Self::Offline => "actions.runners.state.offline",
            Self::Disabled => "actions.runners.state.disabled",
            Self::NeverSeen => "actions.runners.state.never-seen",
            Self::OwnerScope => "actions.runners.scope.owner",
            Self::InstanceScope => "actions.runners.scope.instance",
            Self::Ephemeral => "actions.runners.ephemeral",
            Self::HostNoIsolation => "actions.runners.host",
            Self::HostWarning => "actions.runners.host-warning",
            Self::Capacity { .. } => "actions.runners.capacity",
            Self::Seen { .. } => "actions.runners.seen",
            Self::Enable => "actions.runners.enable",
            Self::Disable => "actions.runners.disable",
            Self::Remove => "actions.runners.remove",
            Self::RemoveRunner { .. } => "actions.runners.remove-label",
            Self::TokensTitle => "actions.runners.tokens.title",
            Self::TokensIntro => "actions.runners.tokens.intro",
            Self::ExpiresAfter => "actions.runners.tokens.expires-after",
            Self::OnlyTheseLabels => "actions.runners.tokens.labels",
            Self::LabelsPlaceholder => "actions.runners.tokens.labels-placeholder",
            Self::Optional => "actions.runners.tokens.optional",
            Self::Reusable => "actions.runners.tokens.reusable",
            Self::NewToken => "actions.runners.tokens.new",
            Self::Created { .. } => "actions.runners.tokens.created",
            Self::Expires { .. } => "actions.runners.tokens.expires",
            Self::NoLongerUsable => "actions.runners.tokens.unusable",
            Self::Revoked => "actions.runners.tokens.revoked",
            Self::Expired => "actions.runners.tokens.expired",
            Self::UnlimitedUses => "actions.runners.tokens.unlimited",
            Self::UsedUp => "actions.runners.tokens.used-up",
            Self::UsesLeft { .. } => "actions.runners.tokens.uses-left",
            Self::Revoke => "actions.runners.tokens.revoke",
            Self::Hours { .. } => "actions.runners.tokens.hours",
            Self::Days { .. } => "actions.runners.tokens.days",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let literal = match self {
            Self::AdminIntro { setting } => {
                text.literal("Instance runners take jobs from the owners an administrator allows, or from every owner with ");
                text.argument(setting);
                return text.literal(".");
            }
            Self::TokenCreatedHint { command } => {
                text.literal(
                    "Copy it now: it is not shown again. On the machine that should run jobs, run the command below and \
                     paste the token when it waits for input, then start the runner with ",
                );
                text.argument(command);
                return text.literal(".");
            }
            Self::LabelsHint { container, host } => {
                text.literal("Labels map to how jobs run: ");
                text.argument(container);
                text.literal(" runs in Docker or Podman, ");
                text.argument(host);
                return text.literal(" runs on the machine itself with no isolation — only for trusted repositories.");
            }
            Self::RepositoryIntro { program } => return intro(text, program, None, "this repository"),
            Self::OwnerIntro { program, owner } => return intro(text, program, Some(owner), ""),
            Self::AccountIntro { program } => return intro(text, program, None, "your repositories"),
            Self::InstanceIntro { program } => return intro(text, program, None, "every owner allowed instance runners"),
            Self::Capacity { count } => {
                text.number(count);
                return text.literal(" at once");
            }
            Self::Seen { when } => {
                text.literal("seen ");
                return text.argument(when);
            }
            Self::RemoveRunner { name } => {
                text.literal("Remove ");
                return text.argument(name);
            }
            Self::Created { when } => {
                text.literal("Created ");
                return text.argument(when);
            }
            Self::Expires { when } => {
                text.literal("expires ");
                return text.argument(when);
            }
            Self::UsesLeft { count } => {
                text.number(count);
                return text.literal(if count == 1 { " use left" } else { " uses left" });
            }
            Self::Hours { count } => {
                text.number(count);
                return text.literal(if count == 1 { " hour" } else { " hours" });
            }
            Self::Days { count } => {
                text.number(count);
                return text.literal(if count == 1 { " day" } else { " days" });
            }
            Self::Title => "Runners",
            Self::TokenCreated => "Registration token created",
            Self::Token => "Token",
            Self::CopyToken => "Copy the token",
            Self::Copy => "Copy",
            Self::Command => "Command",
            Self::CopyCommand => "Copy the command",
            Self::NoRunners => "No runners yet",
            Self::NoRunnersHint => {
                "Create a registration token below and register a runner with it. Jobs wait until a runner with their \
                 labels is online."
            }
            Self::Online => "online",
            Self::Offline => "offline",
            Self::Disabled => "disabled",
            Self::NeverSeen => "never seen",
            Self::OwnerScope => "owner",
            Self::InstanceScope => "instance",
            Self::Ephemeral => "ephemeral",
            Self::HostNoIsolation => "host — no isolation",
            Self::HostWarning => "Jobs on its host labels run as the runner's user, with no isolation",
            Self::Enable => "Enable",
            Self::Disable => "Disable",
            Self::Remove => "Remove",
            Self::TokensTitle => "Registration tokens",
            Self::TokensIntro => {
                "A runner trades a registration token for its own credential once. Tokens are single-use and expire \
                 unless you choose otherwise."
            }
            Self::ExpiresAfter => "Expires after",
            Self::OnlyTheseLabels => "Only these labels",
            Self::LabelsPlaceholder => "ubuntu-latest, arm64",
            Self::Optional => "(optional)",
            Self::Reusable => "Allow any number of runners to register with it until it expires",
            Self::NewToken => "New registration token",
            Self::NoLongerUsable => "no longer usable",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
            Self::UnlimitedUses => "unlimited uses",
            Self::UsedUp => "used up",
            Self::Revoke => "Revoke",
        };
        text.literal(literal);
    }
}

/// "Machines running {program} that take the jobs of {owner or subject}."
fn intro(text: &mut MessageWriter, program: &str, owner: Option<&str>, subject: &str) {
    text.literal("Machines running ");
    text.argument(program);
    text.literal(" that take the jobs of ");
    match owner {
        Some(owner) => text.argument(owner),
        None => text.literal(subject),
    }
    text.literal(".");
}
