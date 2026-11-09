//! Personal Issues/Pull Requests dashboards: which involvement views a page offers and
//! what an absent view or query means. Listing itself is [`super::IssueListing`]'s.

use super::DashboardRelation;
use crate::threads::IssueKind;

/// The search of a dashboard opened without one (or with a blank one).
pub const DEFAULT_DASHBOARD_QUERY: &str = "is:open";

impl DashboardRelation {
    /// Every view, in tab order.
    const ALL: [Self; 4] = [Self::Created, Self::Assigned, Self::Mentioned, Self::ReviewRequested];

    /// The views a dashboard of `kind` offers, in tab order: review requests exist only
    /// for pull requests.
    pub fn views(kind: IssueKind) -> impl Iterator<Item = Self> {
        Self::ALL.into_iter().filter(move |view| view.available_for(kind))
    }

    const fn available_for(self, kind: IssueKind) -> bool {
        !matches!(self, Self::ReviewRequested) || matches!(kind, IssueKind::Pull)
    }

    /// The view `view` selects on a dashboard of `kind` (absent: threads the viewer
    /// created); `None` for a view that does not exist or is not offered there.
    pub fn from_view(view: Option<&str>, kind: IssueKind) -> Option<Self> {
        let view = view.unwrap_or(Self::Created.as_str());
        Self::views(kind).find(|relation| relation.as_str() == view)
    }
}

/// The search a dashboard runs for the typed `q`: trimmed, or [`DEFAULT_DASHBOARD_QUERY`]
/// when absent or blank.
pub fn dashboard_query(q: Option<&str>) -> String {
    q.map(str::trim).filter(|q| !q.is_empty()).unwrap_or(DEFAULT_DASHBOARD_QUERY).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn views_follow_the_page_kind() {
        let pulls: Vec<_> = DashboardRelation::views(IssueKind::Pull).map(DashboardRelation::as_str).collect();
        assert_eq!(pulls, ["created", "assigned", "mentioned", "review-requested"]);
        let issues: Vec<_> = DashboardRelation::views(IssueKind::Issue).map(DashboardRelation::as_str).collect();
        assert_eq!(issues, ["created", "assigned", "mentioned"]);

        assert_eq!(DashboardRelation::from_view(None, IssueKind::Issue), Some(DashboardRelation::Created));
        assert_eq!(DashboardRelation::from_view(Some("mentioned"), IssueKind::Issue), Some(DashboardRelation::Mentioned));
        assert_eq!(DashboardRelation::from_view(Some("review-requested"), IssueKind::Pull), Some(DashboardRelation::ReviewRequested));
        assert_eq!(DashboardRelation::from_view(Some("review-requested"), IssueKind::Issue), None, "issues have no reviews");
        for unknown in ["", "Created", " created", "subscribed"] {
            assert_eq!(DashboardRelation::from_view(Some(unknown), IssueKind::Pull), None, "{unknown:?}");
        }
    }

    #[test]
    fn blank_searches_show_open_threads() {
        assert_eq!(dashboard_query(None), "is:open");
        assert_eq!(dashboard_query(Some("  \t")), "is:open");
        assert_eq!(dashboard_query(Some("  label:bug is:closed ")), "label:bug is:closed");
    }
}
