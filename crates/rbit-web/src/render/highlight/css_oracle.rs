//! Test-only CSS inheritance oracle for compact/nested highlighting parity.

use std::collections::HashMap;

/// One parsed CSS rule: alternative class lists, its position, its declarations.
pub(super) struct CssRule {
    selectors: Vec<Vec<String>>,
    order: usize,
    declarations: Vec<(String, String)>,
}

pub(super) fn parse_css(css: &str) -> Vec<CssRule> {
    let without_comments: String = {
        let mut out = String::new();
        let mut rest = css;
        while let Some(start) = rest.find("/*") {
            out.push_str(&rest[..start]);
            rest = rest[start..].find("*/").map_or("", |end| &rest[start + end + 2..]);
        }
        out.push_str(rest);
        out
    };
    let mut rules = Vec::new();
    for (order, chunk) in without_comments.split('}').enumerate() {
        let Some((selectors, body)) = chunk.split_once('{') else { continue };
        let selectors = selectors
            .split(',')
            .map(|selector| selector.split('.').map(str::trim).filter(|class| !class.is_empty()).map(str::to_owned).collect())
            .collect();
        let declarations = body
            .split(';')
            .filter_map(|declaration| declaration.split_once(':'))
            .map(|(property, value)| (property.trim().to_owned(), value.trim().to_owned()))
            .collect();
        rules.push(CssRule { selectors, order, declarations });
    }
    rules
}

/// The value each property gets on an element with `classes`, by specificity then order.
fn declared(rules: &[CssRule], classes: &[&str]) -> HashMap<String, String> {
    let mut winners: HashMap<String, ((usize, usize), String)> = HashMap::new();
    for rule in rules {
        for selector in &rule.selectors {
            if !selector.iter().all(|class| classes.contains(&class.as_str())) {
                continue;
            }
            let rank = (selector.len(), rule.order);
            for (property, value) in &rule.declarations {
                if winners.get(property).is_none_or(|(best, _)| rank >= *best) {
                    winners.insert(property.clone(), (rank, value.clone()));
                }
            }
        }
    }
    winners.into_iter().map(|(property, (_, value))| (property, value)).collect()
}

/// Every character of a line of markup (entities decoded) with its computed style.
pub(super) fn computed(html: &str, rules: &[CssRule]) -> Vec<(char, Vec<(String, String)>)> {
    computed_with_keyword_weight(html, rules, None)
}

/// The default token is initial (an invalid custom-property value), so `var()` uses
/// its inherit fallback. Contrast themes deliberately supply a numeric weight.
pub(super) fn computed_with_keyword_weight(
    html: &str,
    rules: &[CssRule],
    keyword_weight: Option<&str>,
) -> Vec<(char, Vec<(String, String)>)> {
    let mut stack: Vec<HashMap<String, String>> = Vec::new();
    let mut out = Vec::new();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("</span>") {
            assert!(stack.pop().is_some(), "unbalanced close in {html}");
            rest = after;
        } else if let Some(after) = rest.strip_prefix("<span class=\"") {
            let (classes, after) = after.split_once("\">").expect("tag end");
            stack.push(declared(rules, &classes.split(' ').collect::<Vec<_>>()));
            rest = after;
        } else {
            assert!(!rest.starts_with('<'), "unexpected markup in {html}");
            let (ch, len) = [("&amp;", '&'), ("&lt;", '<'), ("&gt;", '>'), ("&quot;", '"'), ("&#39;", '\'')]
                .iter()
                .find(|(entity, _)| rest.starts_with(entity))
                .map_or_else(
                    || {
                        let ch = rest.chars().next().unwrap();
                        (ch, ch.len_utf8())
                    },
                    |(entity, ch)| (*ch, entity.len()),
                );
            let mut style: Vec<(String, String)> = ["color", "font-weight", "font-style"]
                .iter()
                .filter_map(|property| {
                    stack.iter().rev().find_map(|element| {
                        let value = element.get(*property)?;
                        let resolved =
                            if value == "var(--syntax-keyword-weight, inherit)" { keyword_weight.unwrap_or("inherit") } else { value };
                        (resolved != "inherit").then(|| ((*property).to_owned(), resolved.to_owned()))
                    })
                })
                .collect();
            style.sort();
            out.push((ch, style));
            rest = &rest[len..];
        }
    }
    assert!(stack.is_empty(), "unclosed span in {html}");
    out
}
