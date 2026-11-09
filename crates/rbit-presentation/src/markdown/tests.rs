use super::*;

/// Resolves `x.md` to `/docs/x.md` and drops anything that climbs with `..`.
#[derive(Debug, Clone)]
struct Docs;

impl LinkResolver for Docs {
    fn resolve(&self, target: &str, kind: LinkKind) -> ResolvedLink {
        if target.contains(':') || target.starts_with('#') || target.starts_with('/') {
            ResolvedLink::Unchanged
        } else if target.starts_with("..") {
            ResolvedLink::Dropped
        } else {
            let area = if kind == LinkKind::Image { "raw" } else { "blob" };
            ResolvedLink::Rewritten(format!("/{area}/docs/{target}"))
        }
    }

    fn boxed(&self) -> Box<dyn LinkResolver> {
        Box::new(self.clone())
    }
}

#[test]
fn one_sanitiser_serves_every_document_and_forgets_each_base() {
    let threads: Vec<_> = (0..4)
        .map(|_| {
            std::thread::spawn(|| {
                for _ in 0..10 {
                    let html = render("[guide](guide.md) <img srcset=\"javascript:x 1x, a.png 2x\">", Some(&Docs));
                    assert!(html.contains(r#"href="/blob/docs/guide.md""#), "{html}");
                    assert!(html.contains(r#"srcset="/raw/docs/a.png 2x""#), "{html}");
                    assert!(!html.contains("javascript"), "{html}");
                    // The next document on this thread has no base: nothing of the last one's remains.
                    let html = render("[guide](guide.md)", None);
                    assert!(html.contains(r#"href="guide.md""#), "{html}");
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(CONFIGURED.load(std::sync::atomic::Ordering::Relaxed), 1, "configured once, not per document");
    DOCUMENT.with_borrow(|document| assert!(document.is_none()));
}

#[test]
fn dropped_targets_lose_their_link_and_rewritten_links_are_kept_as_written() {
    let html = render("[up](../x.md) ![img](../x.png)", Some(&Docs));
    assert!(!html.contains("href=") && !html.contains("src="), "{html}");
    let rewrite = |text: &str| {
        text.contains("#1").then(|| Rewritten { html: "<a href=\"issues/1\">#1</a>".to_owned(), hrefs: vec!["issues/1".to_owned()] })
    };
    let html = render_rewriting("see #1 and [docs](guide.md)", Some(&Docs), &rewrite);
    assert!(html.contains(r#"href="issues/1""#), "kept as written: {html}");
    assert!(html.contains(r#"href="/blob/docs/guide.md""#), "{html}");
}

#[test]
fn ids_are_prefixed_and_html_is_escaped() {
    let html = render("# Hello world\n\n[top](#hello-world)", None);
    assert!(html.contains(r#"<a id="user-content-hello-world""#), "{html}");
    assert!(html.contains(r##"href="#user-content-hello-world""##), "{html}");
    assert_eq!(escape_html(r#"<a href="x">'&'</a>"#), "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;");
}
