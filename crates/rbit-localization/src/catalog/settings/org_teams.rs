//! Organization team pages: the list, creation, settings and a team's members and
//! repositories.

labels! {
    /// Fixed labels of the team pages.
    TeamMessage, "org.teams" {
        NewTeam = "new" => "New team",
        NoTeams = "empty-title" => "No teams yet",
        NoTeamsText = "empty-text" => "Teams group members and give each group its own access to repositories.",
        OneMember = "one-member" => "1 member",
        OneRepository = "one-repository" => "1 repository",
        CreateTeam = "create" => "Create team",
        TeamSettings = "settings.title" => "Team settings",
        RenameHint = "settings.rename-hint" => "Renaming changes the team's address.",
        SaveChanges = "settings.save" => "Save changes",
        DeleteTeam = "settings.delete-title" => "Delete this team",
        DeleteTeamText = "settings.delete-text" => "Its members stay in the organization but lose the access this team gave them.",
        DeleteTeamButton = "settings.delete" => "Delete team",
        UsernamePlaceholder = "team.username-placeholder" => "Username",
        OnlyMembers = "team.only-members" => "Only members of the organization can join its teams.",
        NoMembers = "team.no-members" => "No members",
        NoMembersText = "team.no-members-text" => "Members get this team's access to its repositories.",
        AddRepository = "team.add-repository" => "Add a repository",
        RepositoryPlaceholder = "team.repository-placeholder" => "Repository name",
        Permission = "team.permission" => "Permission",
        RepositoryHint = "team.repository-hint" => "Any repository of the organization. Members of the team get this permission on it.",
        NoRepositories = "team.no-repositories" => "No repositories",
        NoRepositoriesText = "team.no-repositories-text" => "This team has not been given access to any repository you can see.",
        Private = "team.private" => "Private",
        Save = "team.save" => "Save",
    }
}

messages! {
    /// Team text with arguments.
    TeamText<'a>, "org.teams" {
        Members(count: u64) = "members" => [(count), " members"],
        Repositories(count: u64) = "repositories" => [(count), " repositories"],
        PermissionOn(repository: &'a str) = "team.permission-on" => ["Permission on ", {repository}],
        SavePermissionOn(repository: &'a str) = "team.save-permission-on" => ["Save permission on ", {repository}],
        TeamsTitle(org: &'a str) = "page-title" => ["Teams · ", {org}],
        NewTeamTitle(org: &'a str) = "new.page-title" => ["New team · ", {org}],
        SettingsTitle(team: &'a str) = "settings.page-title" => ["Settings · ", {team}],
    }
}

/// Team page sentences that embed markup; the template renders each slot.
#[derive(Clone, Copy, Debug)]
pub enum TeamSentence {
    /// Under the team name field. Slots 0, 1 and 2: the characters `-`, `_` and `.`.
    NameHint,
}

impl TeamSentence {
    /// This sentence in `locale`, or its English text marked as a fallback.
    pub fn rich(self, locale: impl std::borrow::Borrow<crate::Locale>) -> crate::catalog::collab::rich::Rich {
        crate::catalog::collab::rich::render(*locale.borrow(), self)
    }
}

impl crate::catalog::collab::rich::RichFamily for TeamSentence {
    fn id(self) -> &'static str {
        match self {
            Self::NameHint => "org.teams.name-hint",
        }
    }

    fn english(self, text: &mut crate::catalog::collab::rich::RichWriter) {
        match self {
            Self::NameHint => {
                text.literal("Letters, digits, ");
                text.slot(0);
                text.literal(", ");
                text.slot(1);
                text.literal(" and ");
                text.slot(2);
                text.literal("; used in the team's address.");
            }
        }
    }

    fn translate(self, locale: crate::Locale, text: &mut crate::catalog::collab::rich::RichWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}
