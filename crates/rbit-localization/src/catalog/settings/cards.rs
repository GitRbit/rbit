//! The cards that link a repository's or an organization's settings to their sub-pages.

labels! {
    /// Titles and descriptions of the settings index cards.
    CardMessage, "settings.cards" {
        Actions = "actions" => "Actions",
        RepoActions = "repo-actions" => "Run workflows on pushes and pull requests, with the secrets and variables they use.",
        OrgActions = "org-actions" => "Secrets and variables for the workflows of every repository of this organization.",
        Collaborators = "collaborators" => "Collaborators",
        CollaboratorsText = "collaborators-text" => "People and teams with access to this repository, and what each may do.",
        Mirror = "mirror" => "Mirror",
        MirrorText = "mirror-text" => "Upstream, fetch interval and the outcome of the last fetch.",
        Protection = "protection" => "Branch protection",
        ProtectionText = "protection-text" => "Keep important branches from being force-pushed, deleted or pushed to without a pull request.",
        PushMirrors = "push-mirrors" => "Push mirrors",
        PushMirrorsText = "push-mirrors-text" => "Remotes that receive this repository's branches and tags after every push.",
        DeployKeys = "deploy-keys" => "Deploy keys",
        DeployKeysText = "deploy-keys-text" => "SSH keys that give a server or CI job access to this repository only.",
        Teams = "teams" => "Teams",
        TeamsText = "teams-text" => "Group members and give each group its own access to repositories.",
        Transfer = "transfer" => "Transfer ownership",
        TransferText = "transfer-text" => "Move this repository to another user or organization. Its URL changes.",
        Webhooks = "webhooks" => "Webhooks",
        RepoWebhooks = "repo-webhooks" => "Notify other services when code is pushed, releases are published and more.",
        OrgWebhooks = "org-webhooks" => "Notify other services about events in every repository of this organization.",
    }
}
