//! Test-only representative composition experiment. Production renderers stay intact.
//!
//! A is the existing inherited view, B uses separately compiled bodies, and C extracts
//! original content blocks. All composition writes to the caller's buffer; no strings
//! are accepted as markup and no escaping/HTML-safety implementation is added here.

use askama::Template;

use super::{auth::LoginPage, explore::ExplorePage, layout::Page};

mod bodies;
mod fixtures;

#[derive(Debug, Clone, Copy)]
enum Mode {
    Inherited,
    Segments,
    Blocks,
}

const MODES: [Mode; 3] = [Mode::Inherited, Mode::Segments, Mode::Blocks];

mod sealed {
    pub(super) trait Approved {}
}

trait ApprovedBody: sealed::Approved {
    fn render_body(&self, output: &mut dyn std::fmt::Write) -> askama::Result<()>;
}

#[derive(Debug, Template)]
#[template(path = "render_probe/shell_open.html")]
struct ShellOpen<'a> {
    page: &'a Page,
}

#[derive(Debug, Template)]
#[template(path = "render_probe/shell_close.html")]
struct ShellClose<'a> {
    page: &'a Page,
}

// A non-generic shell deliberately avoids specializing it for each body context.
fn compose(page: &Page, body: &dyn ApprovedBody, output: &mut String) -> askama::Result<()> {
    ShellOpen { page }.render_into(output)?;
    body.render_body(output)?;
    ShellClose { page }.render_into(output)
}

fn explore_into(mode: Mode, view: &ExplorePage, output: &mut String) -> askama::Result<()> {
    match mode {
        Mode::Inherited => view.render_into(output),
        Mode::Segments => compose(&view.page, &bodies::ExploreSegments::from(view), output),
        Mode::Blocks => compose(&view.page, &bodies::ExploreBlocks::from(view), output),
    }
}

fn login_into(mode: Mode, view: &LoginPage, output: &mut String) -> askama::Result<()> {
    match mode {
        Mode::Inherited => view.render_into(output),
        Mode::Segments => compose(&view.page, &bodies::LoginSegments::from(view), output),
        Mode::Blocks => compose(&view.page, &bodies::LoginBlocks::from(view), output),
    }
}

mod protocol_tests;
mod tests;
