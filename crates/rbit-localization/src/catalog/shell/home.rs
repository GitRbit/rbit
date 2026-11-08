//! The front page for visitors, the signed-in dashboard and the personal issue and
//! pull request dashboards.
use crate::catalog::fallback::{MessageFamily, MessageWriter};

/// A landing-page feature: its title and its one-line description.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Feature {
    Binary,
    Protocols,
    Teams,
    Releases,
    Security,
    Screens,
}

/// Which of a personal dashboard's involvement tabs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Involvement {
    Created,
    Assigned,
    Mentioned,
    ReviewRequested,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeMessage<'a> {
    SearchPublic,
    Search,
    ExploreAll,
    ThisInstance,
    /// How many public repositories, users or organizations; the label of a figure.
    RepositoryCount(i64),
    UserCount(i64),
    OrganizationCount(i64),
    RecentlyUpdated,
    ViewAll,
    NoPublicRepositories,
    NoPublicRepositoriesText,
    FreshServer,
    BuiltForSelfHosting,
    FeatureTitle(Feature),
    FeatureText(Feature),
    Dashboard,
    ViewProfile,
    New,
    NewRepository,
    PersonalDashboards,
    Issues,
    PullRequests,
    Repositories,
    /// How many of the listed repositories are private.
    PrivateCount(usize),
    FindRepository,
    FilterRepositories,
    NoRepositories,
    NoRepositoriesText,
    CreateRepository,
    NoFilterMatch,
    Organizations,
    NewOrganization,
    NotAMember,
    CreateOne,
    ShareWithTeam,
    Explore,
    NothingPublic,
    YourInvolvement,
    InvolvementTab(Involvement),
    /// The dashboard's title, as in "Search Issues".
    SearchIn(&'a str),
    SearchHelp,
    SearchHelpText,
    Results,
    OpenCount(u64),
    ClosedCount(u64),
    NoMatchingThreads,
    NoMatchingThreadsText,
    CommentCount(u32),
}

impl MessageFamily for HomeMessage<'_> {
    fn id(self) -> &'static str {
        match self {
            Self::SearchPublic => "home.landing.search-public",
            Self::Search => "home.search",
            Self::ExploreAll => "home.landing.explore-all",
            Self::ThisInstance => "home.landing.stats",
            Self::RepositoryCount(_) => "home.landing.stat-repositories",
            Self::UserCount(_) => "home.landing.stat-users",
            Self::OrganizationCount(_) => "home.landing.stat-organizations",
            Self::RecentlyUpdated => "home.recently-updated",
            Self::ViewAll => "home.landing.view-all",
            Self::NoPublicRepositories => "home.landing.empty-title",
            Self::NoPublicRepositoriesText => "home.landing.empty-text",
            Self::FreshServer => "home.landing.fresh",
            Self::BuiltForSelfHosting => "home.landing.features",
            Self::FeatureTitle(feature) => feature_ids(feature).0,
            Self::FeatureText(feature) => feature_ids(feature).1,
            Self::Dashboard => "home.dashboard.title",
            Self::ViewProfile => "home.dashboard.view-profile",
            Self::New => "home.dashboard.new",
            Self::NewRepository => "home.dashboard.new-repository",
            Self::PersonalDashboards => "home.dashboards.label",
            Self::Issues => "home.dashboards.issues",
            Self::PullRequests => "home.dashboards.pulls",
            Self::Repositories => "home.dashboard.repositories",
            Self::PrivateCount(_) => "home.dashboard.private-count",
            Self::FindRepository => "home.dashboard.find",
            Self::FilterRepositories => "home.dashboard.filter",
            Self::NoRepositories => "home.dashboard.empty-title",
            Self::NoRepositoriesText => "home.dashboard.empty-text",
            Self::CreateRepository => "home.dashboard.create-repository",
            Self::NoFilterMatch => "home.dashboard.no-filter-match",
            Self::Organizations => "home.dashboard.organizations",
            Self::NewOrganization => "home.dashboard.new-organization",
            Self::NotAMember => "home.dashboard.no-organizations",
            Self::CreateOne => "home.dashboard.create-organization",
            Self::ShareWithTeam => "home.dashboard.share-with-team",
            Self::Explore => "home.dashboard.explore",
            Self::NothingPublic => "home.dashboard.nothing-public",
            Self::YourInvolvement => "home.dashboards.involvement",
            Self::InvolvementTab(tab) => involvement_id(tab),
            Self::SearchIn(_) => "home.dashboards.search-label",
            Self::SearchHelp => "home.dashboards.search-help",
            Self::SearchHelpText => "home.dashboards.search-help-text",
            Self::Results => "home.dashboards.results",
            Self::OpenCount(_) => "home.dashboards.open",
            Self::ClosedCount(_) => "home.dashboards.closed",
            Self::NoMatchingThreads => "home.dashboards.empty-title",
            Self::NoMatchingThreadsText => "home.dashboards.empty-text",
            Self::CommentCount(_) => "home.dashboards.comments",
        }
    }

    fn english(self, text: &mut MessageWriter) {
        let literal = match self {
            Self::RepositoryCount(count) => plural(count == 1, "Repository", "Repositories"),
            Self::UserCount(count) => plural(count == 1, "User", "Users"),
            Self::OrganizationCount(count) => plural(count == 1, "Organization", "Organizations"),
            Self::FeatureTitle(feature) => feature_title(feature),
            Self::FeatureText(feature) => feature_text(feature),
            Self::InvolvementTab(tab) => involvement(tab),
            Self::PrivateCount(count) => {
                text.number(u64::try_from(count).unwrap_or(u64::MAX));
                " private"
            }
            Self::SearchIn(title) => {
                text.literal("Search ");
                return text.argument(title);
            }
            Self::OpenCount(count) => return counted(text, count, " Open", " Open"),
            Self::ClosedCount(count) => return counted(text, count, " Closed", " Closed"),
            Self::CommentCount(count) => return counted(text, u64::from(count), " comment", " comments"),
            other => static_text(other),
        };
        text.literal(literal);
    }

    fn translate(self, locale: crate::locale::Locale, text: &mut MessageWriter) -> bool {
        crate::catalog::translations::write(self, locale, text)
    }
}

const fn plural(one: bool, singular: &'static str, plural: &'static str) -> &'static str {
    if one { singular } else { plural }
}

fn counted(text: &mut MessageWriter, count: u64, singular: &str, plural: &str) {
    text.number(count);
    text.literal(if count == 1 { singular } else { plural });
}

/// The title's and the description's catalog ids.
const fn feature_ids(feature: Feature) -> (&'static str, &'static str) {
    match feature {
        Feature::Binary => ("home.landing.feature.binary", "home.landing.feature.binary-text"),
        Feature::Protocols => ("home.landing.feature.protocols", "home.landing.feature.protocols-text"),
        Feature::Teams => ("home.landing.feature.teams", "home.landing.feature.teams-text"),
        Feature::Releases => ("home.landing.feature.releases", "home.landing.feature.releases-text"),
        Feature::Security => ("home.landing.feature.security", "home.landing.feature.security-text"),
        Feature::Screens => ("home.landing.feature.screens", "home.landing.feature.screens-text"),
    }
}

const fn involvement_id(tab: Involvement) -> &'static str {
    match tab {
        Involvement::Created => "home.dashboards.tab.created",
        Involvement::Assigned => "home.dashboards.tab.assigned",
        Involvement::Mentioned => "home.dashboards.tab.mentioned",
        Involvement::ReviewRequested => "home.dashboards.tab.review-requested",
    }
}

const fn feature_title(feature: Feature) -> &'static str {
    match feature {
        Feature::Binary => "One fast binary",
        Feature::Protocols => "Git over HTTPS and SSH",
        Feature::Teams => "Organizations and teams",
        Feature::Releases => "Releases, mirrors and webhooks",
        Feature::Security => "Secure by default",
        Feature::Screens => "On every screen",
    }
}

const fn feature_text(feature: Feature) -> &'static str {
    match feature {
        Feature::Binary => "SQLite and an embedded store. No other services to install or run.",
        Feature::Protocols => "Smart HTTP with protocol v2, a built-in SSH server and deploy keys.",
        Feature::Teams => "Share repositories with teams and collaborators, role by role.",
        Feature::Releases => "Publish releases, mirror upstream projects and notify other services.",
        Feature::Security => "Two-factor sign-in, scoped access tokens and an audit log.",
        Feature::Screens => "Responsive down to phone size and installable as an app.",
    }
}

const fn involvement(tab: Involvement) -> &'static str {
    match tab {
        Involvement::Created => "Created by me",
        Involvement::Assigned => "Assigned",
        Involvement::Mentioned => "Mentioned",
        Involvement::ReviewRequested => "Review requested",
    }
}

/// Wording without arguments.
const fn static_text(message: HomeMessage<'_>) -> &'static str {
    match message {
        HomeMessage::SearchPublic => "Search public repositories",
        HomeMessage::Search => "Search",
        HomeMessage::ExploreAll => "Explore all repositories",
        HomeMessage::ThisInstance => "This instance",
        HomeMessage::RecentlyUpdated => "Recently updated",
        HomeMessage::ViewAll => "View all",
        HomeMessage::NoPublicRepositories => "No public repositories yet",
        HomeMessage::NoPublicRepositoriesText => "Repositories made public on this server will appear here.",
        HomeMessage::FreshServer => "This server has no accounts yet. The first account to sign up becomes its administrator.",
        HomeMessage::BuiltForSelfHosting => "Built for self-hosting",
        HomeMessage::Dashboard => "Dashboard",
        HomeMessage::ViewProfile => "View profile",
        HomeMessage::New => "New",
        HomeMessage::NewRepository => "New repository",
        HomeMessage::PersonalDashboards => "Personal dashboards",
        HomeMessage::Issues => "Issues",
        HomeMessage::PullRequests => "Pull requests",
        HomeMessage::Repositories => "Repositories",
        HomeMessage::FindRepository => "Find a repository…",
        HomeMessage::FilterRepositories => "Filter your repositories",
        HomeMessage::NoRepositories => "No repositories yet",
        HomeMessage::NoRepositoriesText => "Create a repository to start hosting code on this server.",
        HomeMessage::CreateRepository => "Create repository",
        HomeMessage::NoFilterMatch => "No repositories match your filter.",
        HomeMessage::Organizations => "Organizations",
        HomeMessage::NewOrganization => "New organization",
        HomeMessage::NotAMember => "You are not a member of any organization.",
        HomeMessage::CreateOne => "Create one",
        HomeMessage::ShareWithTeam => "to share repositories with a team.",
        HomeMessage::Explore => "Explore",
        HomeMessage::NothingPublic => "Nothing public yet.",
        HomeMessage::YourInvolvement => "Your involvement",
        HomeMessage::SearchHelp => "Search help",
        HomeMessage::SearchHelpText => {
            "Use is:open, is:closed, author:@me, assignee:@me, label:bug, milestone:\"v1\", no:assignee or \
             sort:updated-desc. Text searches titles and descriptions. Results include only repositories you can \
             currently read. Mentioned includes known mentions in bodies, comments and submitted reviews, even after \
             notifications are read."
        }
        HomeMessage::Results => "Results",
        HomeMessage::NoMatchingThreads => "No matching threads",
        HomeMessage::NoMatchingThreadsText => "Try another filter or involvement tab.",
        HomeMessage::RepositoryCount(_)
        | HomeMessage::UserCount(_)
        | HomeMessage::OrganizationCount(_)
        | HomeMessage::FeatureTitle(_)
        | HomeMessage::FeatureText(_)
        | HomeMessage::PrivateCount(_)
        | HomeMessage::InvolvementTab(_)
        | HomeMessage::SearchIn(_)
        | HomeMessage::OpenCount(_)
        | HomeMessage::ClosedCount(_)
        | HomeMessage::CommentCount(_) => "",
    }
}
