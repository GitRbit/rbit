//! Generated formatting must not hand its closing tags to raw HTML parser state.

fn render(source: &str) -> String {
    super::super::super::render(source, None)
}

#[test]
fn raw_parser_states_cannot_hide_generated_link_closures() {
    let title = format!("scope-sentinel-{}", "x".repeat(3000));
    for (opening, closing) in [
        ("<textarea>", "</textarea>"),
        ("<select>", "</select>"),
        ("<title>", "</title>"),
        ("<style>", "</style>"),
        ("<script>", "</script>"),
        ("<xmp>", "</xmp>"),
        ("<!--", "-->"),
    ] {
        let source = format!("[x{opening}](https://example.com \"{title}\"){closing}<table>\n\n{}", "paragraph\n\n".repeat(1000));
        let html = render(&source);
        assert!(html.len() < source.len() * 5, "{opening}: {} input bytes expanded to {} output bytes", source.len(), html.len());
        assert!(html.matches("scope-sentinel-").count() <= 1, "{opening}: a generated link's attributes were reconstructed");
    }
}

#[test]
fn raw_html_is_escaped_inside_generated_inline_containers() {
    for source in [
        "[x<textarea>](https://example.com)",
        "*x<textarea>*",
        "**x<textarea>**",
        "~~x<textarea>~~",
        "[x</a><select>](https://example.com)",
        "[x<!-- hidden -->](https://example.com)",
        "[<img src=badge.svg>](https://example.com)",
    ] {
        let html = render(source);
        assert!(html.contains("&lt;"), "raw syntax must be text inside generated formatting: {source}");
        assert!(!html.contains("<textarea>") && !html.contains("<select>") && !html.contains("<img"), "{source}: {html}");
    }
}

#[test]
fn nested_generated_scopes_restore_raw_html_only_after_the_last_closure() {
    let html = render("**outer *inner <select>* <textarea>** <details><summary>more</summary>body</details>");
    assert!(html.contains("<strong>") && html.contains("<em>"));
    assert!(html.contains("&lt;select&gt;") && html.contains("&lt;textarea&gt;"));
    assert!(html.contains("<details><summary>more</summary>body</details>"), "raw HTML outside generated scopes remains available: {html}");
}

#[test]
fn native_markdown_links_images_and_formatting_remain_structured() {
    let html = render("[![badge](https://example.com/badge.svg)](https://example.com)\n\n**bold** *em* ~~strike~~");
    assert!(html.contains("<a href=\"https://example.com\""));
    assert!(html.contains("<img src=\"https://example.com/badge.svg\" alt=\"badge\""));
    assert!(html.contains("<strong>bold</strong>") && html.contains("<em>em</em>") && html.contains("<del>strike</del>"));
}
