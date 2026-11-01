//! Static compiled inline SVG, independent of host composition and feature code.
use askama::Template;

mod catalog;

/// A fixed static SVG shape and a borrowed, escaped HTML class attribute.
///
/// The constructor selects only from the private compile-time SVG catalog;
/// an unknown name preserves the empty-SVG fallback. Neither argument supplies
/// SVG markup, URLs, event handlers or executable authority. The class remains
/// escaped by Askama exactly as in the host's former inline macro.
// Askama removes the final file LF; the wrapper emits the former macro LF explicitly.
#[derive(Debug, Clone, Copy, Template)]
#[template(path = "icon.html")]
pub struct Icon<'a> {
    shape: &'static str,
    class: &'a str,
}
impl<'a> Icon<'a> {
    #[inline]
    pub const fn new(name: &str, class: &'a str) -> Self {
        Self { shape: catalog::shape(name), class }
    }
}
// The shape is private fixed catalog output, never caller-supplied HTML. The
// wrapper escapes the class exactly as before; only this fixed shape uses safe.
// Nest through FastWritable without collecting intermediary HTML.
impl askama::filters::HtmlSafe for Icon<'_> {}

#[cfg(test)]
mod tests;
