//! The rendering suite as `rbit-web` calls it: documents with a [`DocumentBase`] go
//! through the cache and resolve links to repository URLs.

use super::{DocumentCache, render_markdown};
use crate::urls::{DocumentBase, RepoUrls};

fn in_docs() -> DocumentBase {
    DocumentBase::new(RepoUrls::new("alice", "proj"), "main", "docs")
}

#[test]
fn documents_with_a_base_are_rendered_once() {
    // Unique to this test, so other tests sharing the process cache cannot hit it.
    let source = format!("# Cached once\n\n{}", "A [relative link](guide.md) in a long README. ".repeat(30));
    let first = render_markdown(&source, Some(&in_docs()));
    let hits = DocumentCache::shared().stats().hits;
    assert_eq!(render_markdown(&source, Some(&in_docs())), first);
    assert!(DocumentCache::shared().stats().hits > hits, "the second view is a hit");
    assert!(first.contains(r#"href="/alice/proj/blob/main/docs/guide.md""#));
    // Another base resolves the same links elsewhere.
    let root = render_markdown(&source, Some(&DocumentBase::new(RepoUrls::new("alice", "proj"), "main", "")));
    assert!(root.contains(r#"href="/alice/proj/blob/main/guide.md""#));
}

#[test]
fn strips_scripts() {
    let html = render_markdown("hello <script>alert(1)</script> <img src=x onerror=alert(1)>", None);
    assert!(!html.contains("<script"));
    assert!(!html.contains("onerror"));
}

#[test]
fn renders_tables_and_tasks() {
    let html = render_markdown("| a |\n|---|\n| b |\n\n- [x] done", None);
    assert!(html.contains("<table>"));
    assert!(html.contains("checkbox"));
}

#[test]
fn relative_links_and_images_resolve_against_the_document() {
    let html = render_markdown("[guide](guide.md) [up](../README.md) ![logo](img/logo.png) [web](https://example.com)", Some(&in_docs()));
    assert!(html.contains(r#"href="/alice/proj/blob/main/docs/guide.md""#), "{html}");
    assert!(html.contains(r#"href="/alice/proj/blob/main/README.md""#), "{html}");
    assert!(html.contains(r#"src="/alice/proj/raw/main/docs/img/logo.png""#), "{html}");
    assert!(html.contains(r#"href="https://example.com""#), "{html}");
}

#[test]
fn raw_html_links_and_images_resolve_too() {
    let html = render_markdown("<p align=\"center\"><img src=\"../logo.svg\"> <a href=\"setup.md\">Setup</a></p>", Some(&in_docs()));
    assert!(html.contains(r#"src="/alice/proj/raw/main/logo.svg""#), "{html}");
    assert!(html.contains(r#"href="/alice/proj/blob/main/docs/setup.md""#), "{html}");
}

#[test]
fn picture_and_responsive_images_resolve_all_candidates() {
    let html = render_markdown(
        r#"<picture><source media="(prefers-color-scheme: dark)" srcset="../docs/dark.avif"><img src="light.avif" srcset="small.avif 1x, ../large%20image.avif 2x" sizes="100vw"></picture>"#,
        Some(&in_docs()),
    );
    assert!(html.contains(r#"srcset="/alice/proj/raw/main/docs/dark.avif""#), "{html}");
    assert!(html.contains(r#"srcset="/alice/proj/raw/main/docs/small.avif 1x, /alice/proj/raw/main/large%20image.avif 2x""#), "{html}");
    assert!(html.contains(r#"src="/alice/proj/raw/main/docs/light.avif""#), "{html}");
}

#[test]
fn srcset_removes_unsafe_candidates_and_preserves_external_urls() {
    let source = r#"<source srcset="javascript:alert(1) 1x, https://cdn.example/a,b.avif 2x, ../../escape.avif 3x"><img srcset="data:text/html,bad 1x, safe.avif 2x">"#;
    for base in [None, Some(in_docs())] {
        let html = render_markdown(source, base.as_ref());
        assert!(!html.contains("javascript:"), "{html}");
        assert!(!html.contains("data:"), "{html}");
        assert!(html.contains("https://cdn.example/a,b.avif 2x"), "{html}");
        if base.is_some() {
            assert!(!html.contains("escape.avif"), "{html}");
        }
    }
}

#[test]
fn mermaid_fences_keep_only_the_supported_class_and_escaped_source() {
    let html = render_markdown("```mermaid\ngraph TD\n A[<script>alert(1)</script>] --> B\n```\n\n```rust\nfn main() {}\n```", None);
    assert!(html.contains(r#"<code class="language-mermaid">"#), "{html}");
    assert!(html.contains("&lt;script&gt;"), "{html}");
    assert!(!html.contains("<script>"), "{html}");
    assert!(!html.contains("language-rust"), "{html}");
}

#[test]
fn links_escaping_the_root_become_plain_text() {
    let html = render_markdown("[secret](../../outside.md) and ![x](../../x.png)", Some(&in_docs()));
    assert!(!html.contains("href="), "{html}");
    assert!(!html.contains("src="), "{html}");
    assert!(html.contains("secret"), "the link text stays: {html}");
}

#[test]
fn anchors_and_mail_links_are_untouched() {
    let html = render_markdown("[top](#intro) [mail](mailto:ada@example.com)", Some(&in_docs()));
    assert!(html.contains(r##"href="#user-content-intro""##), "{html}");
    assert!(html.contains(r#"href="mailto:ada@example.com""#), "{html}");
}

#[test]
fn script_urls_are_still_removed_with_a_base() {
    let html = render_markdown("[x](javascript:alert(1)) <a href=\"JaVaScRiPt:alert(1)\">y</a>", Some(&in_docs()));
    assert!(!html.to_ascii_lowercase().contains("javascript"), "{html}");
}

#[test]
fn headings_get_stable_unique_ids() {
    let html = render_markdown("# Getting started\n\n## Getting started\n\n### `cargo` & Rust!\n\n## Custom {#mine}\n", None);
    let anchor = |id: &str| format!(r#"<a id="user-content-{id}" rel="noopener noreferrer nofollow"></a>"#);
    assert!(
        html.contains(&format!(
            "{}
<h1>Getting started</h1>",
            anchor("getting-started")
        )),
        "{html}"
    );
    assert!(
        html.contains(&format!(
            "{}
<h2>",
            anchor("getting-started-1")
        )),
        "{html}"
    );
    assert!(
        html.contains(&format!(
            "{}
<h3>",
            anchor("cargo--rust")
        )),
        "{html}"
    );
    assert!(html.contains(r#"<h2 id="user-content-mine">"#), "{html}");
    assert!(
        !render_markdown(
            "# !!!
", None
        )
        .contains("<a id"),
        "no empty anchors"
    );
}

#[test]
fn cached_documents_keep_prefixed_ids_and_fragment_links() {
    let source = format!("<div id=\"issuecomment-1\">text</div>\n\n[comment](#issuecomment-1)\n\n{}", "Some text. ".repeat(100));
    let base = in_docs();
    let first = render_markdown(&source, Some(&base));
    let cached = render_markdown(&source, Some(&base));
    assert_eq!(first, cached);
    assert!(cached.contains(r#"id="user-content-issuecomment-1""#), "{cached}");
    assert!(cached.contains(r##"href="#user-content-issuecomment-1""##), "{cached}");
    assert!(!cached.contains(r#"id="issuecomment-1""#), "{cached}");
}

#[test]
fn user_ids_cannot_collide_with_page_ids() {
    for base in [None, Some(in_docs())] {
        let html = render_markdown(
            "<div id=\"issuecomment-1\" name=\"x\">a</div> <span id=\"user-content-kept\"></span>\n\n[c](#issuecomment-1) [k](#user-content-kept)\n\nNote[^1]\n\n[^1]: Footnote.\n",
            base.as_ref(),
        );
        assert!(!html.contains(r#"id="issuecomment-1""#) && html.contains(r#"id="user-content-issuecomment-1""#), "{html}");
        assert!(html.contains(r##"href="#user-content-issuecomment-1""##), "links follow the prefixed ids: {html}");
        assert!(html.contains(r#"id="user-content-kept""#) && html.contains(r##"href="#user-content-kept""##), "{html}");
        assert!(!html.contains("name="), "{html}");
        assert!(!html.contains(r#" id="1""#) && !html.contains(r##"href="#1""##), "footnotes are prefixed too: {html}");
    }
}
