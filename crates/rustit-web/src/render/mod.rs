//! Turning repository content into safe HTML.

mod highlight;
mod markdown;

pub use highlight::Highlighter;
pub use markdown::render_markdown;

/// Whether a file name denotes Markdown that should be rendered rather than shown as source.
pub fn is_markdown(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    [".md", ".markdown", ".mdown", ".mkd"].iter().any(|ext| lower.ends_with(ext))
}

/// Human-readable byte size (`1.4 KB`).
pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", UNITS[unit]) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1536), "1.5 KB");
    }
}
