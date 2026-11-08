//! Repository settings: general options, deploy keys, the pull mirror and push mirrors.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

labels! {
    /// Fixed text of the repository settings pages.
    RepoSettingsLabel {
        Settings => "repo.settings.title", "Settings";
        AdminsOf => "repo.settings.admins-of", "Only administrators of";
        AdminsOfEnd => "repo.settings.admins-of-end", "can see this page.";
        SettingsSaved => "repo.settings.saved", "Settings saved.";
        General => "repo.settings.general", "General";
        Description => "repo.settings.description", "Description";
        Optional => "repo.settings.optional", "optional";
        Visibility => "repo.settings.visibility", "Visibility";
        Public => "repo.settings.public", "Public";
        PublicHint => "repo.settings.public-hint", "Anyone can see and clone this repository.";
        Private => "repo.settings.private", "Private";
        PrivateHint => "repo.settings.private-hint", "Only people with access can see and clone it.";
        DefaultBranch => "repo.settings.default-branch", "Default branch";
        PushABranch => "repo.settings.no-branches", "Push a branch to choose the default branch.";
        DefaultBranchHint => "repo.settings.default-branch-hint", "Shown first when browsing, and checked out by git clone.";
        SaveChanges => "repo.settings.save-changes", "Save changes";
        RepositoryName => "repo.settings.name-title", "Repository name";
        Name => "repo.settings.name", "Name";
        RenameHint => "repo.settings.rename-hint", "Links and clone URLs that use the old name stop working.";
        Rename => "repo.settings.rename", "Rename";
        DangerZone => "repo.settings.danger-zone", "Danger zone";
        UnarchiveTitle => "repo.settings.unarchive-title", "Unarchive this repository";
        UnarchiveHint => "repo.settings.unarchive-hint", "It accepts pushes, issues, pull requests, comments and releases again, and its mirrors resume.";
        Unarchive => "repo.settings.unarchive", "Unarchive repository";
        ArchiveTitle => "repo.settings.archive-title", "Archive this repository";
        ArchiveHint => "repo.settings.archive-hint", "It becomes read-only: people can still clone, fork and star it, but nobody can push, merge or change its issues, pull requests or releases, and its mirrors pause. You can unarchive it at any time.";
        Archive => "repo.settings.archive", "Archive repository";
        DeleteTitle => "repo.settings.delete-title", "Delete this repository";
        DeleteHint => "repo.settings.delete-hint", "Its files, history and stars are removed permanently. This cannot be undone.";
        Type => "repo.settings.confirm-type", "Type";
        ToConfirm => "repo.settings.confirm-end", "to confirm";
        DeleteRepository => "repo.settings.delete", "Delete repository";
        DeployKeys => "repo.keys.title", "Deploy keys";
        DeployKeysHint => "repo.keys.card-hint", "Keys that give a server or CI job SSH access to this repository only.";
        DeployKeysLeadStart => "repo.keys.lead-start", "A deploy key gives a server or CI job SSH access to";
        DeployKeysLeadEnd => "repo.keys.lead-end", "only. Keys are read-only unless you allow them to push.";
        NewDeployKey => "repo.keys.new", "New deploy key";
        Title => "repo.keys.key-title", "Title";
        TitlePlaceholder => "repo.keys.title-placeholder", "e.g. Production server";
        TitleHint => "repo.keys.title-hint", "Defaults to the key's comment.";
        Key => "repo.keys.key", "Key";
        KeyHint => "repo.keys.key-hint", "A key can belong to only one repository or account; generate a separate key for each.";
        AllowWrite => "repo.keys.allow-write", "Allow write access";
        AllowWriteHint => "repo.keys.allow-write-hint", "The key can push to this repository as well as clone it.";
        AddDeployKey => "repo.keys.add", "Add deploy key";
        Keys => "repo.keys.list", "Keys";
        NoDeployKeys => "repo.keys.empty-title", "No deploy keys";
        NoDeployKeysText => "repo.keys.empty", "Add a key to let a server or CI job clone this repository over SSH.";
        Delete => "repo.keys.delete", "Delete";
        Mirror => "repo.mirror-settings.title", "Mirror";
        MirrorHint => "repo.mirror-settings.card-hint", "Upstream, fetch interval and the outcome of the last fetch.";
        MirrorLead => "repo.mirror-settings.lead", "This repository fetches the branches and tags of its upstream and does not accept pushes.";
        Upstream => "repo.mirror-settings.upstream", "Upstream";
        LastFetch => "repo.mirror-settings.last-fetch", "Last fetch";
        Failed => "repo.mirror-settings.failed", "Failed";
        Ok => "repo.mirror-settings.ok", "OK";
        NotYet => "repo.mirror-settings.not-yet", "Not yet";
        NextFetch => "repo.mirror-settings.next-fetch", "Next fetch";
        Error => "repo.mirror-settings.error", "Error";
        FetchNow => "repo.mirror-settings.fetch-now", "Fetch now";
        FetchEvery => "repo.mirror-settings.interval", "Fetch every";
        Minutes => "repo.mirror-settings.minutes", "minutes";
        Save => "repo.mirror-settings.save", "Save";
        Saved => "repo.mirror-settings.saved", "Saved.";
        FetchQueued => "repo.mirror-settings.queued", "A fetch is queued; refresh in a moment to see its outcome.";
        PushQueued => "repo.push-mirrors.queued", "A push is queued; refresh in a moment to see its outcome.";
        StopMirroring => "repo.mirror-settings.stop", "Stop mirroring";
        StopHint => "repo.mirror-settings.stop-hint", "The repository keeps its content, stops fetching and accepts pushes like any other.";
        PushMirrors => "repo.push-mirrors.title", "Push mirrors";
        PushMirrorsHint => "repo.push-mirrors.card-hint", "Remotes that receive this repository's branches and tags after every push.";
        PushMirrorsLead => "repo.push-mirrors.lead", "Remotes that receive this repository's branches and tags. Each push overwrites the remote's branches and tags and deletes those that no longer exist here.";
        PushMirrorsDisabled => "repo.push-mirrors.disabled-title", "Push mirrors are disabled";
        PushMirrorsDisabledText => "repo.push-mirrors.disabled", "An administrator of this instance has turned push mirrors off.";
        NoPushMirrors => "repo.push-mirrors.empty-title", "No push mirrors yet";
        NoPushMirrorsText => "repo.push-mirrors.empty", "Add a remote below to keep a copy of this repository elsewhere.";
        Paused => "repo.push-mirrors.paused", "Paused";
        OnPush => "repo.push-mirrors.on-push", "On push";
        LastPush => "repo.push-mirrors.last-push", "Last push";
        NotPushedYet => "repo.push-mirrors.not-pushed", "Not pushed yet";
        As => "repo.push-mirrors.as", "as";
        SecretConfigured => "repo.push-mirrors.secret", "secret configured";
        PushNow => "repo.push-mirrors.push-now", "Push now";
        Edit => "repo.push-mirrors.edit", "Edit";
        Remove => "repo.push-mirrors.remove", "Remove";
        AddPushMirrorTitle => "repo.push-mirrors.add-title", "Add a push mirror";
        AddPushMirror => "repo.push-mirrors.add", "Add push mirror";
        PushMirrorAdded => "repo.push-mirrors.added", "Push mirror added. The first push is on its way; refresh in a moment to see its outcome.";
        PushMirrorRemoved => "repo.push-mirrors.removed", "Push mirror removed. The remote keeps what it has.";
        EditPushMirror => "repo.push-mirrors.edit-title", "Edit push mirror";
        PushMirror => "repo.push-mirrors.document-title", "Push mirror";
        Enabled => "repo.push-mirrors.enabled", "Enabled";
        EnabledHint => "repo.push-mirrors.enabled-hint", "A paused mirror keeps its settings and is not pushed.";
        Cancel => "repo.push-mirrors.cancel", "Cancel";
        RemoteUrl => "repo.push-mirrors.url", "Remote URL";
        UrlHintStart => "repo.push-mirrors.url-hint-start", "An";
        UrlHintOr => "repo.push-mirrors.url-hint-or", "or";
        UrlHintEnd => "repo.push-mirrors.url-hint-end", "URL without a user name or password in it.";
        UserName => "repo.push-mirrors.username", "User name";
        PasswordOrToken => "repo.push-mirrors.secret-label", "Password or token";
        SecretPlaceholder => "repo.push-mirrors.secret-placeholder", "Configured — leave blank to keep it";
        SecretHint => "repo.push-mirrors.secret-hint", "Sent to the remote over HTTP Basic authentication. It cannot be shown again.";
        SecretHintKept => "repo.push-mirrors.secret-hint-kept", "Sent to the remote over HTTP Basic authentication. It cannot be shown again, and must be entered again if the remote moves to another host.";
        PushAfterPush => "repo.push-mirrors.sync-on-push", "Push after every push";
        PushAfterPushHint => "repo.push-mirrors.sync-on-push-hint", "A few seconds after this repository changes. Every mirror is also pushed on a schedule.";
    }
}

/// Named or counted text of the settings pages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepoSettingsMessage<'a> {
    /// Under the fetch interval: "At least 10 minutes."
    AtLeastMinutes { minutes: u64 },
    /// The schedule of push mirrors: "Every mirror is pushed at least every 60 minutes."
    PushSchedule { minutes: u64 },
    /// A deploy key's delete button: "Delete deploy@ci".
    DeleteKey { title: &'a str },
}

impl MessageFamily for RepoSettingsMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::AtLeastMinutes { .. } => "repo.mirror-settings.min-interval",
            Self::PushSchedule { .. } => "repo.push-mirrors.schedule",
            Self::DeleteKey { .. } => "repo.keys.delete-label",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        match self {
            Self::AtLeastMinutes { minutes } => {
                text.literal("At least ");
                text.number(minutes);
                text.literal(if minutes == 1 { " minute." } else { " minutes." });
            }
            Self::PushSchedule { minutes } => {
                text.literal("Every mirror is pushed at least every ");
                text.number(minutes);
                text.literal(if minutes == 1 { " minute." } else { " minutes." });
            }
            Self::DeleteKey { title } => {
                text.literal("Delete ");
                text.argument(title);
            }
        }
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

lookup!(RepoSettingsMessage<'_>);
