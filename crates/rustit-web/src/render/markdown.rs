//! GitHub-flavoured Markdown → sanitised HTML.

use std::sync::OnceLock;

use pulldown_cmark::{Options, Parser, html};

/// Renders untrusted Markdown. The output is sanitised with ammonia, so it is safe to
/// insert into a page verbatim even when the input contains raw HTML.
pub fn render_markdown(source: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_GFM;

    let mut unsafe_html = String::with_capacity(source.len() * 3 / 2);
    html::push_html(&mut unsafe_html, Parser::new_ext(source, options));
    sanitizer().clean(&unsafe_html).to_string()
}

fn sanitizer() -> &'static ammonia::Builder<'static> {
    static SANITIZER: OnceLock<ammonia::Builder<'static>> = OnceLock::new();
    SANITIZER.get_or_init(|| {
        let mut builder = ammonia::Builder::default();
        builder
            .add_tags(["input", "details", "summary", "picture", "source"])
            .add_tag_attributes("input", ["type", "checked", "disabled"])
            .add_tag_attributes("source", ["srcset", "media", "type"])
            .add_tag_attributes("img", ["align", "width", "height"])
            .add_tag_attributes("p", ["align"])
            .add_tag_attributes("div", ["align"])
            .add_generic_attributes(["id"])
            .link_rel(Some("noopener noreferrer nofollow"));
        builder
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_scripts() {
        let html = render_markdown("hello <script>alert(1)</script> <img src=x onerror=alert(1)>");
        assert!(!html.contains("<script"));
        assert!(!html.contains("onerror"));
    }

    #[test]
    fn renders_tables_and_tasks() {
        let html = render_markdown("| a |\n|---|\n| b |\n\n- [x] done");
        assert!(html.contains("<table>"));
        assert!(html.contains("checkbox"));
    }
}
