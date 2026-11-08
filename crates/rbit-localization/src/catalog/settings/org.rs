//! Organization pages: the header and tabs, creation, members and settings.

labels! {
    /// Fixed labels of the organization pages.
    OrgMessage, "org" {
        Organization = "organization" => "Organization",
        Repositories = "tab.repositories" => "Repositories",
        Members = "tab.members" => "Members",
        Teams = "tab.teams" => "Teams",
        Settings = "tab.settings" => "Settings",
        NewOrganization = "new.title" => "New organization",
        NewIntro = "new.intro" => "An organization owns repositories on behalf of a team. You will be its first owner.",
        Name = "new.name" => "Name",
        NameHint = "new.name-hint" => "Used in URLs. Letters, digits, “-”, “_” and “.”. Users and organizations share one namespace.",
        DisplayName = "display-name" => "Display name",
        Optional = "optional" => "optional",
        Description = "description" => "Description",
        Cancel = "cancel" => "Cancel",
        CreateOrganization = "new.create" => "Create organization",
        New = "profile.new" => "New",
        NoRepositories = "profile.empty-title" => "No repositories yet",
        NoRepositoriesText = "profile.empty-text" => "Repositories owned by this organization will be listed here.",
        Profile = "settings.profile" => "Profile",
        Website = "settings.website" => "Website",
        WebsitePlaceholder = "settings.website-placeholder" => "https://",
        SaveProfile = "settings.save-profile" => "Save profile",
        MemberPermission = "settings.member-permission" => "Member permission",
        PermissionLegend = "settings.permission-legend" => "What every member gets on every repository",
        PermissionHint = "settings.permission-hint" => "Teams and collaborators can add to this, never take away. Owners always administer every repository.",
        SavePermission = "settings.save-permission" => "Save permission",
        NoPermission = "settings.base-none" => "No permission",
        NoPermissionText = "settings.base-none-text" => "Members only see public repositories and those their teams can access.",
        Read = "settings.base-read" => "Read",
        ReadText = "settings.base-read-text" => "Members can browse and clone every repository.",
        Write = "settings.base-write" => "Write",
        WriteText = "settings.base-write-text" => "Members can browse, clone and push to every repository.",
        DangerZone = "settings.danger-zone" => "Danger zone",
        DeleteOrganization = "settings.delete-title" => "Delete this organization",
        DeleteText = "settings.delete-text" => "This permanently removes the organization and all of its memberships. It cannot be undone.",
        TypeToConfirmLead = "settings.confirm-lead" => "Type",
        TypeToConfirmTail = "settings.confirm-tail" => "to confirm",
        DeleteOrganizationButton = "settings.delete" => "Delete organization",
        AddMember = "members.add-title" => "Add a member",
        Username = "members.username" => "Username",
        Role = "members.role" => "Role",
        Member = "members.member" => "Member",
        Owner = "members.owner" => "Owner",
        Add = "members.add" => "Add",
        MembersHint = "members.hint" => "Members get the organization's base permission on every repository, plus whatever their teams grant. Owners can also manage members, teams and settings.",
        You = "members.you" => "(you)",
        Leave = "members.leave" => "Leave",
        Remove = "members.remove" => "Remove",
        MakeOwner = "members.make-owner" => "Make owner",
        MakeMember = "members.make-member" => "Make member",
    }
}

messages! {
    /// Organization text with arguments.
    OrgText<'a>, "org" {
        StillOwnsOne(org: &'a str) = "settings.still-owns-one" => [{org}, " still owns 1 repository. Delete or transfer it first."],
        StillOwns(org: &'a str, count: u64) = "settings.still-owns" => [
            {org},
            " still owns ",
            (count),
            " repositories. Delete or transfer them first."
        ],
        RemoveLabel(name: &'a str) = "members.remove-label" => ["Remove ", {name}],
        RoleLabel(action: &'a str, name: &'a str) = "members.role-label" => [{action}, ": ", {name}],
        MembersTitle(org: &'a str) = "members.page-title" => ["Members · ", {org}],
        SettingsTitle(org: &'a str) = "settings.page-title" => ["Settings · ", {org}],
    }
}
