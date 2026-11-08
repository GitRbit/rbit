//! Repository access: the collaborators and transfer settings pages.

labels! {
    /// Fixed labels of the collaborators and transfer pages.
    AccessMessage, "settings.access" {
        Settings = "settings" => "Settings",
        Collaborators = "collaborators.title" => "Collaborators",
        IntroLead = "collaborators.intro-lead" => "People added here get access to",
        IntroDirectly = "collaborators.intro-directly" => "directly",
        IntroOrganization = "collaborators.intro-organization" => ", whether or not they belong to the organization",
        IntroTail = "collaborators.intro-tail" => ".",
        AddCollaborator = "collaborators.add-title" => "Add a collaborator",
        Username = "collaborators.username" => "Username",
        Permission = "collaborators.permission" => "Permission",
        Add = "collaborators.add" => "Add",
        People = "collaborators.people" => "People",
        NoCollaborators = "collaborators.empty-title" => "No collaborators",
        NoCollaboratorsText = "collaborators.empty-text" => "Add someone by username to give them access to this repository.",
        You = "collaborators.you" => "(you)",
        Save = "collaborators.save" => "Save",
        Leave = "collaborators.leave" => "Leave",
        Remove = "collaborators.remove" => "Remove",
        Teams = "collaborators.teams" => "Teams",
        ManageTeams = "collaborators.manage-teams" => "Manage teams",
        NoTeams = "collaborators.no-teams-title" => "No teams",
        NoTeamsText = "collaborators.no-teams-text" => "Grant a team access from the team's page; its members then get that permission here.",
        Transfer = "transfer.title" => "Transfer ownership",
        TransferLead = "transfer.intro-lead" => "Move",
        TransferTail = "transfer.intro-tail" => "to another account. Its history, collaborators and stars move with it; links and clone URLs that use the old address stop working, and teams of the current organization lose their access.",
        NewOwner = "transfer.new-owner" => "New owner",
        AdminEntry = "transfer.admin-entry" => "As a site administrator you may enter any user or organization.",
        Organization = "transfer.organization" => "Organization",
        OwnerHint = "transfer.owner-hint" => "Your account, or an organization you belong to.",
        TypeLead = "transfer.type-lead" => "Type",
        TypeTail = "transfer.type-tail" => "to confirm",
        TransferRepository = "transfer.submit" => "Transfer repository",
        Nowhere = "transfer.nowhere-title" => "Nowhere to transfer to",
        NowhereText = "transfer.nowhere-text" => "You belong to no organization where you could create repositories, and this repository is already yours.",
    }
}

messages! {
    /// Collaborator and transfer text with arguments.
    AccessText<'a>, "settings.access" {
        PermissionFor(name: &'a str) = "collaborators.permission-for" => ["Permission for ", {name}],
        SavePermissionFor(name: &'a str) = "collaborators.save-permission-for" => ["Save permission for ", {name}],
        RemoveLabel(name: &'a str) = "collaborators.remove-label" => ["Remove ", {name}],
        Added(date: &'a str) = "collaborators.added" => ["Added ", {date}],
        CollaboratorsTitle(repository: &'a str) = "collaborators.page-title" => ["Collaborators · ", {repository}],
        TransferTitle(repository: &'a str) = "transfer.page-title" => ["Transfer · ", {repository}],
    }
}
