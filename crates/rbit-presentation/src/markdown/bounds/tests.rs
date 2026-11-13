use pulldown_cmark::{Options, Parser, Tag, TagEnd};
use std::fmt::Write as _;

use super::*;

fn depth(events: &[Event<'_>]) -> usize {
    let (mut open, mut deepest) = (0_usize, 0);
    for event in events {
        match event {
            Event::Start(_) => {
                open += 1;
                deepest = deepest.max(open);
            }
            Event::End(_) => open -= 1,
            _ => {}
        }
    }
    deepest
}

#[test]
fn deep_markdown_is_flattened_with_its_content_kept() {
    for source in [">".repeat(500) + " deep", "1. ".repeat(500) + "deep", "- ".repeat(500) + "deep"] {
        let events: Vec<_> = bounded(Parser::new_ext(&source, Options::all())).collect();
        assert!(depth(&events) <= MAX_NESTING, "{} levels", depth(&events));
        assert!(events.iter().any(|event| matches!(event, Event::Text(text) if text.contains("deep"))));
        let starts = events.iter().filter(|event| matches!(event, Event::Start(_))).count();
        assert_eq!(starts, events.iter().filter(|event| matches!(event, Event::End(_))).count(), "kept containers stay paired");
    }
    let shallow: Vec<_> = bounded(Parser::new("> > quote")).collect();
    assert!(matches!(shallow[0], Event::Start(Tag::BlockQuote(_))));
    assert!(matches!(shallow.last(), Some(Event::End(TagEnd::BlockQuote(_)))));
}

/// The fastest of three renderings of `source`, each given ten seconds at most.
fn render_time(source: &str) -> std::time::Duration {
    (0..3)
        .map(|_| {
            let (source, (done, finished)) = (source.to_owned(), std::sync::mpsc::channel());
            std::thread::spawn(move || {
                let started = std::time::Instant::now();
                drop(super::super::render(&source, None));
                let _ = done.send(started.elapsed());
            });
            finished.recv_timeout(std::time::Duration::from_secs(10)).expect("a hostile document rendered within ten seconds")
        })
        .min()
        .unwrap()
}

#[test]
fn hostile_nesting_renders_in_linear_time() {
    for unit in [">", "1. ", "- > ", "<div>", "<div/>", "<div></bogus>", "<rt>", "<span id=1>x", "<a><b><i><em><span>"] {
        let small = render_time(&unit.repeat(16 * 1024 / unit.len()));
        let large = render_time(&unit.repeat(64 * 1024 / unit.len()));
        // Four times the input: about four times the time when linear, sixteen when
        // quadratic. Renderings too fast to measure reliably pass.
        assert!(
            large < std::time::Duration::from_millis(50) || large < small * 10,
            "{unit:?}: 16 KiB took {small:?}, 64 KiB took {large:?}"
        );
    }
}

#[test]
fn syntax_independent_budget_bounds_html5_parser_edge_cases() {
    for unit in [
        "<div/>",
        "<div></bogus>",
        "<x-a></x-b>",
        "<rt>",
        "<rp>",
        "<div title='</div>'>",
        "<div\u{b}></div>",
        "<!--><div/>",
        "<textarea><!--</textarea><div/>",
    ] {
        let source = format!("<div>\n{}", unit.repeat(MAX_RAW_HTML_MARKUP + 1));
        let events: Vec<_> = bounded(Parser::new(&source)).collect();
        let raw: usize = events
            .iter()
            .filter_map(|event| match event {
                Event::Html(html) | Event::InlineHtml(html) => Some(html.bytes().filter(|&byte| byte == b'<').count()),
                _ => None,
            })
            .sum();
        assert!(raw <= MAX_RAW_HTML_MARKUP, "{unit:?}: emitted {raw} markup introducers");
        assert!(events.iter().any(|event| matches!(event, Event::Text(text) if text.contains('<'))), "{unit:?}");
    }
}

#[test]
fn raw_markup_budget_applies_across_inline_events_and_keeps_ordinary_tables() {
    let source = format!("text {}", "<span/> text ".repeat(MAX_RAW_HTML_MARKUP + 10));
    let events: Vec<_> = bounded(Parser::new(&source)).collect();
    assert_eq!(events.iter().filter(|event| matches!(event, Event::InlineHtml(_))).count(), MAX_RAW_HTML_MARKUP);
    let source = format!("<table>\n{}\n</table>", "<tr><td>cell</td><td>cell</td></tr>\n".repeat(500));
    let events: Vec<_> = bounded(Parser::new(&source)).collect();
    assert!(events.iter().any(|event| matches!(event, Event::Html(html) if html.contains("<table>"))));
    assert!(!events.iter().any(|event| matches!(event, Event::Text(text) if text.contains('<'))));
}

#[test]
fn raw_table_scopes_and_formatting_are_excluded_in_both_orders() {
    for name in TABLE_CONTEXT {
        let name = std::str::from_utf8(name).unwrap().to_ascii_uppercase();
        for table in [format!("<{name}>"), format!("</{name}>"), format!("<{name}")] {
            for reversed in [false, true] {
                let mut parts = [Event::InlineHtml("<b title='kept'>".into()), Event::Html(table.clone().into())];
                if reversed {
                    parts.reverse();
                }
                let events: Vec<_> = bounded(parts.into_iter()).collect();
                assert_eq!(
                    events.iter().filter(|event| matches!(event, Event::Html(_) | Event::InlineHtml(_))).count(),
                    1,
                    "raw formatting must never share a table scope: {table:?}, reversed={reversed}"
                );
            }
        }
    }
    let events: Vec<_> = bounded([Event::Html("<table><b title='kept'>".into())].into_iter()).collect();
    assert!(matches!(&events[..], [Event::Text(_)]));
}

#[test]
fn raw_formatting_budget_prevents_html5_reconstruction_amplification() {
    let mut source = String::from("x ");
    for n in 0..2048 {
        write!(source, "<b id=\"{n}\"/>").unwrap();
    }
    source.push_str(&"<p>x</p>".repeat(1000));
    let html = super::super::render(&source, None);
    assert!(html.matches("<b").count() <= MAX_RAW_FORMATTING * 1002, "active formatting reconstruction stays bounded");
    assert!(html.len() < 1024 * 1024, "small hostile input must not expand into millions of HTML nodes");
    for (source, expected) in [
        ("<b><B/><strong class=x></strong><em\t><i\n><font\r><u\x0c>", 7),
        ("<!--<b>--><div title='<em>'><b", 3),
        ("<button><body><blockquote><b\u{b}><b-x><b0></b>< b>", 0),
    ] {
        assert_eq!(formatting_starts(source), expected, "{source:?}");
    }
}

#[test]
fn formatting_attribute_bytes_cannot_expand_during_reconstruction() {
    let mut source = String::from("x ");
    for n in 0..64 {
        write!(source, "<b title=\"{n}{}\"/>", "x".repeat(8192)).unwrap();
    }
    source.push_str(&"<p>x</p>".repeat(500));
    let html = super::super::render(&source, None);
    assert!(html.len() < source.len() * 2, "large formatting attributes must be escaped before the parser can copy them");
    let events =
        [Event::Html("<b title=\"".into()), Event::Html("x".repeat(MAX_RAW_FORMATTING_BYTES + 1).into()), Event::Html("\">text".into())];
    let events: Vec<_> = bounded(events.into_iter()).collect();
    assert!(matches!(events[1], Event::Text(_)), "attribute fragments cannot evade the cumulative byte budget");
}

#[test]
fn split_raw_formatting_names_cannot_bypass_the_budget() {
    let events = (0..=MAX_RAW_FORMATTING).flat_map(|_| [Event::Html("<str".into()), Event::Html("ong>".into())]);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, bounded(events));
    assert_eq!(html.matches("<strong>").count(), MAX_RAW_FORMATTING);
    assert!(html.contains("&lt;str"));
    for fragment in ["<", "<a", "<b", "<st", "<str", "<stro", "<STRON"] {
        assert_eq!(formatting_starts(fragment), 1, "{fragment}");
    }
}

#[test]
fn generated_markdown_formatting_survives_raw_html_parser_contexts() {
    for prefix in [
        "<table>",
        "<select>",
        "<textarea>",
        "<template>",
        "<svg>",
        "<math>",
        "<table><b id=\"x\"/>",
        "<svg><foreignObject>",
        "<math><mtext>",
    ] {
        let source = format!("{prefix}\n\n{}", "**bold** *em* ~~strike~~ [link](https://example.com)\n\n".repeat(1000));
        let html = super::super::render(&source, None);
        assert!(
            html.len() < source.len() * 5,
            "{prefix}: formatting reconstruction expanded {} input bytes to {} output bytes",
            source.len(),
            html.len()
        );
    }
    let source = "**bold** *em* ~~strike~~ [link](https://example.com)\n\n".repeat(1000);
    let html = super::super::render(&source, None);
    assert_eq!(html.matches("<strong>").count(), 1000, "ordinary Markdown formatting keeps working past the raw HTML budget");
    assert_eq!(html.matches("<em>").count(), 1000);
}

#[test]
fn retained_raw_markup_budget_has_a_bounded_render_cost() {
    let source = format!("x {}\n\n{}", "<div/>".repeat(MAX_RAW_HTML_MARKUP), "paragraph\n\n".repeat(10_000));
    let elapsed = render_time(&source);
    eprintln!("maximum retained raw markup plus 10000 paragraphs: {elapsed:?}");
    assert!(elapsed < std::time::Duration::from_secs(2), "the maximum retained markup budget took {elapsed:?}");
    let malformed = format!("<p {}>", "<p ".repeat(MAX_RAW_HTML_MARKUP - 1));
    let elapsed = render_time(&malformed);
    eprintln!("malformed tag scan: {elapsed:?}");
    assert!(elapsed < std::time::Duration::from_secs(2), "malformed tag scanning took {elapsed:?}");
}

#[test]
fn raw_html_beyond_the_depth_budget_becomes_text() {
    assert_eq!(raw_depth("<div><p>a<br><img src=x/><b>x</b></div>", 0), (2, 0));
    assert_eq!(raw_depth("<!-- <div> --><span/>", 3), (3, 3));
    let source = "<div>".repeat(MAX_RAW_HTML_DEPTH + 10);
    let events: Vec<_> = bounded(Parser::new(&source)).collect();
    assert!(events.iter().all(|event| !matches!(event, Event::Html(_))), "the over-deep block is text");
    let inline = format!("x {} y", "<span id=1>z".repeat(MAX_RAW_HTML_DEPTH + 10));
    let events: Vec<_> = bounded(Parser::new(&inline)).collect();
    let kept = events.iter().filter(|event| matches!(event, Event::InlineHtml(_))).count();
    assert_eq!(kept, MAX_RAW_HTML_DEPTH, "inline tags are kept up to the budget");
    let ordinary = "<details><summary>More</summary>\n\nText\n\n</details>";
    assert!(bounded(Parser::new(ordinary)).all(|event| !matches!(event, Event::Text(ref text) if text.contains('<'))));
}

#[test]
fn retained_inline_formatting_attributes_do_not_multiply_across_markdown_paragraphs() {
    let title = format!("attribute-sentinel-{}", "x".repeat(3000));
    let mut failures = Vec::new();
    for prefix in ["", "<table>", "<select>", "<textarea>", "<template>", "<svg>", "<math>", "<svg><foreignObject>", "<math><mtext>"] {
        let source = format!("{prefix}\n\nx <b title=\"{title}\"/>\n\n{}", "paragraph\n\n".repeat(1000));
        let html = super::super::render(&source, None);
        eprintln!("retained formatting {prefix:?}: {} source bytes, {} rendered bytes", source.len(), html.len());
        if html.len() >= source.len() * 5 || html.matches("attribute-sentinel-").count() > 4 {
            failures.push(format!(
                "{prefix:?}: {} input bytes, {} output bytes, {} attribute copies",
                source.len(),
                html.len(),
                html.matches("attribute-sentinel-").count()
            ));
        }
    }
    assert!(failures.is_empty(), "retained attributes amplified during reconstruction: {failures:#?}");
}

#[test]
fn multiline_formatting_attributes_cannot_survive_as_reconstruction_payloads() {
    let title = format!("multiline-sentinel-{}", "x".repeat(3000));
    for prefix in ["", "<div>\n", "<table>\n", "<svg>\n", "<math>\n"] {
        for tag in [format!("<b\n title=\"{title}\"\n>"), format!("<b title=\"\n{title}\n\">"), format!("<b\n title='{title}'\n/>")] {
            let source = format!("{prefix}{tag}\n\n{}", "paragraph\n\n".repeat(1000));
            let html = super::super::render(&source, None);
            eprintln!("multiline formatting {prefix:?}: {} source bytes, {} rendered bytes", source.len(), html.len());
            assert!(
                html.len() < source.len() * 5,
                "{prefix:?}: multiline attributes amplified {} input bytes into {} output bytes",
                source.len(),
                html.len()
            );
            assert!(html.matches("multiline-sentinel-").count() <= 4, "{prefix:?}: a split attribute became a reconstruction payload");
        }
    }
}

#[test]
fn generated_link_attributes_do_not_multiply_through_raw_table_scopes() {
    let title = format!("link-sentinel-{}", "x".repeat(3000));
    let mut failures = Vec::new();
    for content in ["x<table>y", "x<table/>y", "x<table><tr><td>y", "x<table></table>y"] {
        for suffix in ["", "</table>"] {
            let source = format!("[{content}](https://example.com \"{title}\"){suffix}\n\n{}", "paragraph\n\n".repeat(1000));
            let html = super::super::render(&source, None);
            eprintln!("generated link {content:?}/{suffix:?}: {} source bytes, {} rendered bytes", source.len(), html.len());
            if html.len() >= source.len() * 5 || html.matches("link-sentinel-").count() > 4 {
                failures.push(format!(
                    "{content:?}/{suffix:?}: {} source bytes, {} rendered bytes, {} title copies",
                    source.len(),
                    html.len(),
                    html.matches("link-sentinel-").count()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "generated link attributes amplified through raw scopes: {failures:#?}");
}

#[test]
fn raw_formatting_attributes_do_not_amplify_through_native_markdown_tables() {
    let title = format!("native-table-sentinel-{}", "x".repeat(3000));
    let header = "| first | second |\n| --- | --- |\n";
    let rows = "| a | b |\n".repeat(1000);
    for source in [
        format!("x <b title=\"{title}\"/>\n\n{header}{rows}\nparagraph\n\n"),
        format!("{header}| x <b title=\"{title}\"/> | y |\n{rows}\nparagraph\n\n"),
        format!("x <b title=\"{title}\"/>\n\n**bold**\n\n{header}{rows}\nparagraph\n\n"),
    ] {
        let html = super::super::render(&source, None);
        eprintln!("native table formatting: {} source bytes, {} rendered bytes", source.len(), html.len());
        assert!(
            html.len() < source.len() * 5,
            "native Markdown tables amplified {} source bytes to {} output bytes",
            source.len(),
            html.len()
        );
        assert!(
            html.matches("native-table-sentinel-").count() <= 4,
            "native Markdown cell boundaries repeatedly reconstructed retained attributes"
        );
        assert!(html.contains("<table>"), "native Markdown tables remain structured");
    }
}

#[test]
fn escaped_multiline_attribute_fragments_cannot_bypass_the_raw_byte_budget() {
    let title = format!("fragment-sentinel-{}", "x".repeat(8192));
    let mut failures = Vec::new();
    for tag in [format!("<b\n title=\"{title}\"\n>"), format!("<b title=\"\n{title}\n\">"), format!("<b\n title='{title}'\n/>")] {
        for prefix in ["", "<div>\n"] {
            let source = format!("{prefix}{tag}\n{}", "<p>x</p>".repeat(500));
            let html = super::super::render(&source, None);
            eprintln!("escaped fragment {prefix:?}: {} source bytes, {} rendered bytes", source.len(), html.len());
            if html.len() >= source.len() * 5 || html.matches("fragment-sentinel-").count() > 4 {
                failures.push(format!(
                    "{prefix:?}: {} source bytes, {} rendered bytes, {} title copies",
                    source.len(),
                    html.len(),
                    html.matches("fragment-sentinel-").count()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "escaped fragments became retained attributes: {failures:#?}");
}
