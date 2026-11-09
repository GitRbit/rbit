//! Real view-model fixtures. URLs and machine values use existing constructors.

use rbit_core::{RepoSort, repository::ArchivedFilter};
use rbit_localization::{Catalog, Locale, Message};
use rbit_services::updates::{AvailableRelease, UpdateState, UpdateStatus};

use super::{ExplorePage, LoginPage, Page};
use crate::{
    themes::Theme,
    views::{
        components::{Avatar, FormError, Pagination, RepoCard},
        explore::{ExploreLink, ExploreResults, ExploreText, explore_url},
        external_auth::ProviderButton,
        layout::NavUser,
        test_support,
    },
};

pub(super) const UNTRUSTED: &str = "<script>probe</script> & \"' </title><!--partial:main--> 雪";

#[derive(Debug, Clone, Copy)]
pub(super) enum Actor {
    Anonymous,
    Member,
    Admin,
}

pub(super) fn page(path: &str, actor: Actor, theme: Theme, locale: Locale) -> Page {
    let mut page = test_support::page(path).with_theme(theme).with_locale(locale).titled(UNTRUSTED);
    page.site_name = format!("Forge {UNTRUSTED}");
    page.branding.description = UNTRUSTED.into();
    page.branding.footer_text = UNTRUSTED.into();
    page.branding.primary_color = "#123456".into();
    page.branding.primary_text_color = "#ffffff".into();
    page.branding.support_url = "/support?from=probe&help=true".into();
    page.branding.terms_url = "/terms".into();
    page.branding.privacy_url = "/privacy".into();
    page.branding.validate().unwrap();
    page.updates = UpdateStatus {
        revision: 123,
        state: UpdateState::Available,
        release: Some(AvailableRelease { version: "1.2.3".into(), url: "https://github.com/GitRbit/rbit/releases/tag/v1.2.3".into() }),
    };
    if !matches!(actor, Actor::Anonymous) {
        page.viewer = Some(NavUser {
            name: "alice".into(),
            label: UNTRUSTED.into(),
            avatar: Avatar::for_identity("Alice Example", "alice"),
            is_admin: matches!(actor, Actor::Admin),
            can_create_org: matches!(actor, Actor::Admin),
            has_actions: true,
        });
        page.has_unread_notifications = true;
    }
    page
}

pub(super) fn explore(actor: Actor, theme: Theme, locale: Locale, empty: bool) -> ExplorePage {
    let catalog = Catalog::new(locale);
    let pagination = Pagination {
        page: 2,
        page_count: 3,
        previous_url: Some(explore_url(UNTRUSTED, RepoSort::Name, ArchivedFilter::Only, 1)),
        next_url: Some(explore_url(UNTRUSTED, RepoSort::Name, ArchivedFilter::Only, 3)),
    };
    let page = page("/explore", actor, theme, locale);
    let labels = ExploreText::new(catalog);
    let link = |message, sort, archived| ExploreLink {
        label: catalog.text(message),
        url: explore_url(UNTRUSTED, sort, archived, 1),
        is_active: sort == RepoSort::Name,
    };
    let repos = if empty {
        Vec::new()
    } else {
        vec![RepoCard {
            owner: "keeper".into(),
            name: "project".into(),
            url: "/keeper/project".into(),
            description: Some(UNTRUSTED.into()),
            is_private: false,
            is_archived: true,
            updated: rbit_core::Timestamp(rbit_core::Timestamp::now().seconds() - 3660).into(),
            updated_iso: "2026-11-05T12:00:00Z".into(),
            owner_avatar: Avatar::for_identity("Keeper", "keeper"),
            star_count: 2,
        }]
    };
    ExplorePage {
        page,
        query: UNTRUSTED.into(),
        sort: "name",
        archived: Some("true"),
        results: ExploreResults {
            locale,
            repos,
            sorts: vec![
                link(Message::SortUpdated, RepoSort::RecentlyUpdated, ArchivedFilter::Only),
                link(Message::SortName, RepoSort::Name, ArchivedFilter::Only),
            ],
            archived_filters: vec![link(Message::ArchiveAll, RepoSort::Name, ArchivedFilter::Only)],
            pagination,
            query_is_empty: false,
            total: 61,
            labels,
        },
    }
}

pub(super) fn login(actor: Actor, theme: Theme, extra: bool) -> LoginPage {
    LoginPage {
        page: page("/login", actor, theme, Locale::English),
        error: if extra { FormError::new(UNTRUSTED) } else { FormError::default() },
        login: if extra { UNTRUSTED.into() } else { String::new() },
        next: explore_url(UNTRUSTED, RepoSort::Name, ArchivedFilter::Only, 2),
        registration_open: extra,
        providers: if extra {
            vec![ProviderButton { name: UNTRUSTED.into(), url: "/login/provider?next=%2Fexplore&flow=probe".into() }]
        } else {
            Vec::new()
        },
    }
}
