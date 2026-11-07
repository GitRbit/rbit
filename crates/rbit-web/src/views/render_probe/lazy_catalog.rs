//! Exact pre-change owned-catalog oracle against real full/fragment templates.
use super::fixtures::{self, Actor};
use crate::{
    assets,
    themes::Theme,
    views::{
        explore::{ExploreFragment, ExploreResults},
        layout::Page,
    },
};
use askama::Template;
use rbit_localization::{Catalog, Locale, LocalizedText, Message};
use std::ops::Deref;

/// Shadow only the changed dynamic methods with the original owned-text behavior.
/// All other fields/data and actual template sources remain identical.
#[derive(Debug)]
struct OwnedResults<'a>(&'a ExploreResults);
impl Deref for OwnedResults<'_> {
    type Target = ExploreResults;
    fn deref(&self) -> &Self::Target {
        self.0
    }
}
impl OwnedResults<'_> {
    fn count(&self, query: &str) -> LocalizedText {
        Catalog::new(self.locale).text(Message::CountForQuery { count: self.total, query })
    }
    #[allow(clippy::trivially_copy_pass_by_ref, reason = "match the borrowed numeric argument in the actual Askama template")]
    fn star_label(&self, count: &i64) -> LocalizedText {
        Catalog::new(self.locale).text(Message::Stars { count: *count })
    }
    fn subtitle(&self, site: &str) -> LocalizedText {
        Catalog::new(self.locale).text(Message::ExploreSubtitle { site })
    }
    fn page_of(&self) -> LocalizedText {
        Catalog::new(self.locale).text(Message::PageOf { page: self.pagination.page, pages: self.pagination.page_count })
    }
}
#[derive(Template)]
#[template(path = "home/explore.html")]
struct FullReference<'a> {
    page: &'a Page,
    query: &'a str,
    sort: &'static str,
    archived: Option<&'static str>,
    results: OwnedResults<'a>,
}
#[derive(Template)]
#[template(path = "home/explore_fragment.html")]
struct FragmentReference<'a> {
    query: &'a str,
    page: &'a Page,
    results: OwnedResults<'a>,
}

#[tokio::test]
async fn real_full_fragment_and_shared_shell_keep_exact_owned_catalog_bytes() {
    for locale in [Locale::English, Locale::Spanish, Locale::PseudoExpanded, Locale::PseudoRtl] {
        for actor in [Actor::Anonymous, Actor::Member] {
            for (count, page, pages) in [(i64::MIN, 0, 0), (0, 1, 1), (1, 1, 2), (i64::MAX, u32::MAX, u32::MAX)] {
                let mut view = fixtures::explore(actor, Theme::Dark, locale, false);
                view.page.site_name = "<script>CATALOG_SUBTITLE_ONLY</script>&\"' 雪 café \u{2066}עברית\u{2069}".into();
                view.results.repos[0].star_count = count;
                view.results.total = match count {
                    i64::MIN => 0,
                    0 => 1,
                    1 => 2,
                    _ => u64::MAX,
                };
                if count == i64::MIN {
                    view.query.clear();
                    view.results.query_is_empty = true;
                }
                view.results.pagination.page = page;
                view.results.pagination.page_count = pages;
                for include_head in [false, true] {
                    assets::with_head_styles(include_head, async {
                        let old = FullReference {
                            page: &view.page,
                            query: &view.query,
                            sort: view.sort,
                            archived: view.archived,
                            results: OwnedResults(&view.results),
                        }
                        .render()
                        .unwrap();
                        let actual = view.render().unwrap();
                        assert_eq!(actual, old, "full {locale:?} {actor:?} {count} {page}/{pages} head={include_head}");
                        let old = FragmentReference { page: &view.page, query: &view.query, results: OwnedResults(&view.results) }
                            .render()
                            .unwrap();
                        let actual = ExploreFragment { page: &view.page, query: &view.query, results: &view.results }.render().unwrap();
                        assert_eq!(actual, old, "fragment {locale:?} {count} {page}/{pages}");
                        assert!(!actual.contains("<script>CATALOG_SUBTITLE_ONLY</script>"));
                        // The title retains the escaped site name; results never
                        // request the full-document subtitle.
                        let (title, results) = actual.split_once("</title>").expect("fragment document title");
                        assert!(title.contains("CATALOG_SUBTITLE_ONLY"));
                        assert!(!results.contains("CATALOG_SUBTITLE_ONLY"));
                    })
                    .await;
                }
            }
        }
    }
}
