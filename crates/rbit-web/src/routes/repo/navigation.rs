//! Repository scope remains stable while history links retain the selected revision.
use super::{AppState, Avatar, RepoHeader, RepoPage};

impl RepoPage {
    pub(in crate::routes) fn header(&self, state: &AppState, active_tab: &'static str) -> RepoHeader {
        self.navigation_header(state, active_tab, None)
    }

    pub(in crate::routes) fn header_at(&self, state: &AppState, active_tab: &'static str, revision: &str) -> RepoHeader {
        self.navigation_header(state, active_tab, Some(revision))
    }

    fn navigation_header(&self, state: &AppState, active_tab: &'static str, revision: Option<&str>) -> RepoHeader {
        let repo = &self.ctx.repo;
        let landing = self.landing_revision().unwrap_or_else(|| self.default_branch().to_owned());
        RepoHeader {
            owner: repo.owner.name.to_string(),
            name: repo.repo.name.to_string(),
            description: repo.repo.description.clone(),
            is_private: !repo.repo.visibility.is_public(),
            owner_avatar: Avatar::for_account(&repo.owner),
            clone_url: format!("{}/{}.git", state.config.server.base_url(), repo.full_name()),
            ssh_clone_url: state.config.ssh.enabled.then(|| state.config.ssh.clone_url(&state.config.server, &repo.full_name())),
            home_url: self.urls.home(),
            commits_url: match revision.filter(|revision| *revision != landing.as_str()) {
                Some(revision) => self.urls.commits(revision),
                None => format!("{}/commits", self.urls.home()),
            },
            activity_url: format!("{}/activity", self.urls.home()),
            branches_url: self.urls.branches(),
            tags_url: self.urls.tags(),
            releases_url: self.urls.releases(),
            zip_url: self.urls.archive(&landing, ".zip"),
            settings_url: format!("{}/settings", self.urls.home()),
            can_admin: self.ctx.access.can_admin(),
            star_count: self.stars.count,
            starred_by_viewer: self.stars.starred_by_viewer,
            fork_count: self.fork_count,
            fork_url: format!("{}/fork", self.urls.home()),
            forks_url: format!("{}/forks", self.urls.home()),
            forked_from: self.forked_from.clone(),
            mirror_url: self.mirror.as_ref().map(|mirror| mirror.url.to_string()),
            archived: repo.repo.is_archived(),
            issues_url: format!("{}/issues", self.urls.home()),
            pulls_url: format!("{}/pulls", self.urls.home()),
            open_issues: self.open_counts.issues,
            open_pulls: self.open_counts.pulls,
            actions_url: self.actions_on.then(|| format!("{}/actions", self.urls.home())),
            actions_settings_url: (state.services.actions.is_some() && self.ctx.access.can_admin())
                .then(|| format!("{}/settings/actions", self.urls.home())),
            // Administrators see the tab of a turned-off wiki: it is where they turn it on.
            wiki_url: (self.wiki_on || (self.ctx.access.can_admin() && !repo.repo.is_archived()))
                .then(|| format!("{}/wiki", self.urls.home())),
            active_tab,
        }
    }
}
