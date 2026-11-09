//! GitHub-style heading anchors, inserted before sanitising.

use std::collections::HashMap;

use pulldown_cmark::{CowStr, Event, Tag, TagEnd};

/// Collects the events, adding an anchor named after its text before every heading that
/// has no explicit id.
pub(super) fn with_heading_ids<'a>(events: impl Iterator<Item = Event<'a>>) -> Vec<Event<'a>> {
    let mut output: Vec<Event<'a>> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    // Index of the open heading's start event and its text so far.
    let mut open: Option<(usize, String)> = None;
    for event in events {
        match &event {
            Event::Start(Tag::Heading { id: None, .. }) => open = Some((output.len(), String::new())),
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, heading_text)) = open.as_mut() {
                    heading_text.push_str(text);
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((index, text)) = open.take() {
                    let slug = unique_slug(&mut seen, &text);
                    if !slug.is_empty() {
                        // Slugs hold only letters, digits, `-` and `_`: nothing to escape.
                        output.insert(index, Event::Html(CowStr::from(format!("<a id=\"{slug}\"></a>"))));
                    }
                }
            }
            _ => {}
        }
        output.push(event);
    }
    output
}

/// GitHub's anchor slug: lower-case, letters, digits, `-` and `_` kept, spaces turned
/// into `-`, everything else removed; repeats get `-1`, `-2`, ….
fn unique_slug(seen: &mut HashMap<String, usize>, text: &str) -> String {
    let slug: String = text
        .trim()
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect();
    let count = seen.entry(slug.clone()).or_insert(0);
    let unique = if *count == 0 { slug.clone() } else { format!("{slug}-{count}") };
    *count += 1;
    unique
}
