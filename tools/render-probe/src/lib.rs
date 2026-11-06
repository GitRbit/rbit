//! Development-only synthetic Askama experiment; no production integration.
//!
//! Generated `OUT_DIR` fixtures deliberately repeat concrete derives to investigate
//! compilation scaling. Templates are compiled once; rendering never parses source.
//! Slots accept only privately approved HTML-derived types and share one output buffer.

use std::fmt;

#[cfg(any(feature = "segments", feature = "blocks"))]
mod composition;

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

/// One untrusted row of synthetic display content.
#[derive(Clone, Copy, Debug)]
pub struct Row<'a> {
    pub key: &'a str,
    pub label: &'a str,
}

/// Identical inputs for every mode. These are HTML text/attribute data, never markup.
#[derive(Clone, Copy, Debug)]
pub struct FixtureData<'a> {
    pub title: &'a str,
    pub site_name: &'a str,
    pub viewer: Option<&'a str>,
    pub description: &'a str,
    pub footer: &'a str,
    pub dark: bool,
    pub rows: &'a [Row<'a>],
}

/// Compile-time composition variants, with stable names in measurement records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mode {
    Inheritance,
    Segments,
    Blocks,
}

impl Mode {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Inheritance => "inheritance",
            Self::Segments => "segments",
            Self::Blocks => "blocks",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "inheritance" => Some(Self::Inheritance),
            "segments" => Some(Self::Segments),
            "blocks" => Some(Self::Blocks),
            _ => None,
        }
    }
}

/// Modes actually compiled into this binary; compilation comparisons select one.
pub const COMPILED_MODES: &[Mode] = &[
    #[cfg(feature = "inheritance")]
    Mode::Inheritance,
    #[cfg(feature = "segments")]
    Mode::Segments,
    #[cfg(feature = "blocks")]
    Mode::Blocks,
];

/// Invalid lab configuration or failure from the actual compiled template renderer.
#[derive(Debug)]
pub enum ProbeError {
    ModeUnavailable(Mode),
    CaseOutOfRange { requested: usize, available: usize },
    Render(askama::Error),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ModeUnavailable(mode) => write!(output, "mode {} was not compiled", mode.name()),
            Self::CaseOutOfRange { requested, available } => write!(output, "case {requested} is outside 0..{available}"),
            Self::Render(error) => write!(output, "template rendering failed: {error}"),
        }
    }
}

impl std::error::Error for ProbeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Render(error) => Some(error),
            _ => None,
        }
    }
}

/// Appends one concrete case directly to the caller's buffer. Clear it before reuse.
/// No intermediate HTML string, escaping override or runtime template parser is used.
pub fn render_into(mode: Mode, case: usize, data: &FixtureData<'_>, out: &mut String) -> Result<(), ProbeError> {
    if case >= CASE_COUNT {
        return Err(ProbeError::CaseOutOfRange { requested: case, available: CASE_COUNT });
    }
    match mode {
        Mode::Inheritance => {
            #[cfg(feature = "inheritance")]
            {
                inheritance::render(case, data, out)
            }
            #[cfg(not(feature = "inheritance"))]
            {
                Err(ProbeError::ModeUnavailable(mode))
            }
        }
        Mode::Segments => {
            #[cfg(feature = "segments")]
            {
                segments::render(case, data, out)
            }
            #[cfg(not(feature = "segments"))]
            {
                Err(ProbeError::ModeUnavailable(mode))
            }
        }
        Mode::Blocks => {
            #[cfg(feature = "blocks")]
            {
                blocks::render(case, data, out)
            }
            #[cfg(not(feature = "blocks"))]
            {
                Err(ProbeError::ModeUnavailable(mode))
            }
        }
    }
}
