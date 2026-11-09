//! htmx fragments carry the catalog they were rendered in, a stale document's catalog gets
//! a full refresh instead of a swap, and fragments pinned for a year are the same bytes in
//! every catalog, so no browser cache can serve one language's text in another.
use rbit_localization::Locale;
use rbit_web::middleware::PINNED_CACHE_CONTROL;

use super::fixture::{Fixture, LOCALES, REPO};

#[tokio::test]
async fn boosted_and_targeted_fragments_name_their_catalog() {
    let fixture = Fixture::new().await;
    for locale in Locale::LANGUAGES {
        let (tag, key) = (locale.tag(), locale.catalog_key());
        let partial = fixture
            .get(
                "/safety-owner/demo/issues",
                &[("Accept-Language", tag), ("HX-Request", "true"), ("Rbit-Partial", REPO), ("Rbit-Document-Locale", key)],
            )
            .await;
        assert!(partial.body.contains("<main") && !partial.body.contains("<html"), "{tag}: a partial page");
        assert_eq!((partial.header("content-language"), partial.header("rbit-catalog-key")), (tag, key), "{tag}");
        assert!(partial.header("hx-refresh").is_empty(), "{tag}: no refresh when the catalogs agree");

        let explore = fixture.get("/explore", &[("Accept-Language", tag), ("HX-Request", "true"), ("HX-Target", "explore-results")]).await;
        assert!(!explore.body.contains("<html"), "{tag}: a fragment");
        assert_eq!((explore.header("content-language"), explore.header("rbit-catalog-key")), (tag, key), "{tag}");

        // A page still showing another catalog never swaps this one in: htmx reloads it.
        for stale in Locale::LANGUAGES.into_iter().filter(|other| *other != locale) {
            let swapped = fixture
                .get(
                    "/safety-owner/demo/issues",
                    &[
                        ("Accept-Language", tag),
                        ("HX-Request", "true"),
                        ("Rbit-Partial", REPO),
                        ("Rbit-Document-Locale", stale.catalog_key()),
                    ],
                )
                .await;
            assert_eq!(swapped.header("hx-refresh"), "true", "{tag} into a {} page", stale.tag());
            assert_eq!(swapped.header("cache-control"), "no-store", "{tag} into a {} page", stale.tag());
        }
    }
}

/// The URL in the first `hx-get="…"` attribute that contains `marker`.
fn deferred_url(body: &str, marker: &str) -> Option<String> {
    body.split("hx-get=\"").skip(1).map(|rest| &rest[..rest.find('"').unwrap()]).find(|url| url.contains(marker)).map(str::to_owned)
}

#[tokio::test]
async fn pinned_fragments_are_language_neutral() {
    let fixture = Fixture::new().await;
    let home = fixture.get(REPO, &[]).await;
    let entries = deferred_url(&home.body, "/tree-commits/").expect("the tree defers its last commits");
    let (base, head) = &fixture.tips;
    let counts = format!("{REPO}/pull/2/counts/{base}...{head}");
    for (path, marker) in [(entries, "hx-swap-oob"), (counts, "pull-commits-phrase")] {
        let mut english = None;
        for locale in LOCALES {
            let tag = locale.tag();
            let page =
                fixture.get(&path, &[("Accept-Language", tag), ("HX-Request", "true"), ("Cookie", &format!("rbit-locale={tag}"))]).await;
            assert_eq!(page.header("cache-control"), PINNED_CACHE_CONTROL, "{path} in {tag}: pinned");
            assert!(page.header("rbit-catalog-key").is_empty() && page.header("content-language").is_empty(), "{path} in {tag}");
            assert!(!page.header("vary").contains("Accept-Language"), "{path} in {tag}: one entry for every language");
            assert!(page.body.contains(marker), "{path} in {tag}: {}", page.body);
            let english = english.get_or_insert_with(|| page.body.clone());
            assert_eq!(&page.body, english, "{path} in {tag}: a pinned fragment is the same in every catalog");
        }
    }
}

/// The pinned counts are English in every page they are swapped into, so they say so.
#[tokio::test]
async fn the_pinned_counts_phrase_is_marked_english() {
    let fixture = Fixture::new().await;
    let (base, head) = &fixture.tips;
    let page = fixture.get(&format!("{REPO}/pull/2/counts/{base}...{head}"), &[("Accept-Language", "de")]).await;
    let phrase = r#"<span id="pull-commits-phrase" hx-swap-oob="true" lang="en">1 commit</span>"#;
    assert!(page.body.contains(phrase), "{}", page.body);
}
