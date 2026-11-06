//! Default-off real-page composition candidate, compiled with Askama 0.16.1.
//!
//! There is one output buffer and no body registry or intermediate body HTML.
//! Askama itself accepts `dyn fmt::Write` in both this candidate and the inherited
//! baseline. Devirtualization and compile/runtime improvements require measurement.

use std::fmt;

use askama::{FastWritable, Template, Values};

use super::{auth::LoginPage, explore::ExplorePage, layout::Page};

mod contexts;
use contexts::{ExploreBody, LoginBody};

#[derive(Template)]
#[template(path = "render_probe/shell_open.html")]
struct Prefix<'a> {
    page: &'a Page,
}

#[derive(Template)]
#[template(path = "render_probe/shell_close.html")]
struct Suffix<'a> {
    page: &'a Page,
}

// These shared, non-generic functions avoid cloning shell implementations per body.
fn prefix(page: &Page, output: &mut dyn fmt::Write, values: &dyn Values) -> askama::Result<()> {
    Prefix { page }.render_into_with_values(output, values)
}

fn suffix(page: &Page, output: &mut dyn fmt::Write, values: &dyn Values) -> askama::Result<()> {
    Suffix { page }.render_into_with_values(output, values)
}

// Preserve every Template entry point, including caller-owned writers and Values.
// The two bodies are concrete compiled HTML contexts, never arbitrary strings.
macro_rules! composed_page {
    ($page:ty, $body:ident) => {
        impl Template for $page {
            const SIZE_HINT: usize = Prefix::SIZE_HINT + $body::SIZE_HINT + Suffix::SIZE_HINT;

            fn render_into_with_values(&self, output: &mut dyn fmt::Write, values: &dyn Values) -> askama::Result<()> {
                prefix(&self.page, output, values)?;
                $body::from(self).render_into_with_values(output, values)?;
                suffix(&self.page, output, values)
            }

            fn render_with_values(&self, values: &dyn Values) -> askama::Result<String> {
                let mut output = String::new();
                let _ = output.try_reserve(Self::SIZE_HINT);
                // A single concrete caller-owned String for prefix, body and suffix.
                self.render_into_with_values(&mut output, values)?;
                Ok(output)
            }
        }

        impl FastWritable for $page {
            fn write_into(&self, output: &mut dyn fmt::Write, values: &dyn Values) -> askama::Result<()> {
                self.render_into_with_values(output, values)
            }
        }

        impl fmt::Display for $page {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.render_into(formatter).map_err(|_| fmt::Error)
            }
        }
    };
}

composed_page!(ExplorePage, ExploreBody);
composed_page!(LoginPage, LoginBody);
