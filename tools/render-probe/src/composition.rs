//! Fixed non-generic shell plus sealed, explicitly approved compiled HTML bodies.

use askama::Template;

use crate::FixtureData;

pub(crate) mod sealed {
    pub(crate) trait ApprovedHtml {}
}

/// Only generated HTML template types implement this private component boundary.
pub(crate) trait ApprovedHtml: sealed::ApprovedHtml {
    fn render_into(&self, output: &mut dyn std::fmt::Write) -> askama::Result<()>;
}

#[derive(Template)]
#[template(path = "shell_open.html")]
struct ShellOpen<'a> {
    data: &'a FixtureData<'a>,
}

#[derive(Template)]
#[template(path = "shell_close.html")]
struct ShellClose<'a> {
    data: &'a FixtureData<'a>,
}

pub(crate) fn render(data: &FixtureData<'_>, body: &dyn ApprovedHtml, output: &mut String) -> askama::Result<()> {
    // Every renderer writes to the same buffer; ordinary fields remain HTML escaped.
    ShellOpen { data }.render_into(output)?;
    body.render_into(output)?;
    ShellClose { data }.render_into(output)
}
