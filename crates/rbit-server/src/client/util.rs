//! Small text helpers the client's commands share.

use anyhow::anyhow;
use serde_json::Value;

/// A field's value in a query string: text as is, anything else as JSON.
pub fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

pub fn split_pair(pair: &str) -> anyhow::Result<(String, String)> {
    let (key, value) = pair.split_once('=').ok_or_else(|| anyhow!("`{pair}` is not `key=value`"))?;
    Ok((key.to_owned(), value.to_owned()))
}

/// `text` for a URL's query or path segment.
pub fn encode(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

pub fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width { text.to_owned() } else { format!("{}…", text.chars().take(width - 1).collect::<String>()) }
}

/// Whether `name` matches `pattern`, `*` for any run of characters and `?` for one.
pub fn wildcard(pattern: &str, name: &str) -> bool {
    fn go(pattern: &[char], name: &[char]) -> bool {
        match (pattern.split_first(), name.split_first()) {
            (None, None) => true,
            (Some(('*', rest)), _) => go(rest, name) || (!name.is_empty() && go(pattern, &name[1..])),
            (Some(('?', rest)), Some((_, others))) => go(rest, others),
            (Some((expected, rest)), Some((actual, others))) => expected == actual && go(rest, others),
            _ => false,
        }
    }
    go(&pattern.chars().collect::<Vec<_>>(), &name.chars().collect::<Vec<_>>())
}

pub fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}

/// `completed/success`, or the status while it runs.
pub fn state(value: &Value) -> String {
    match value["conclusion"].as_str() {
        Some(conclusion) => conclusion.to_owned(),
        None => text(value, "status").to_owned(),
    }
}

/// The first 12 characters of a commit id, for people.
pub fn short(sha: &str) -> &str {
    &sha[..sha.len().min(12)]
}

/// A command of the Git repository in the current directory, its trimmed output when it
/// succeeded.
pub fn git(args: &[&str]) -> Option<String> {
    let output = std::process::Command::new("git").args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(split_pair("a=b=c").unwrap(), ("a".to_owned(), "b=c".to_owned()));
        assert!(split_pair("nope").is_err());
        assert_eq!(encode("feature/x y"), "feature%2Fx%20y");
        assert!(wildcard("dist-*", "dist-linux") && !wildcard("dist-?", "dist-xy"));
        assert_eq!(clip("abcdef", 4), "abc…");
        assert_eq!(short("0123456789abcdef"), "0123456789ab");
        assert_eq!(state(&serde_json::json!({ "status": "completed", "conclusion": "failure" })), "failure");
        assert_eq!(state(&serde_json::json!({ "status": "queued", "conclusion": null })), "queued");
    }
}
