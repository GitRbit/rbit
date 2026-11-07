//! Source records shared by module discovery, resolution and line aggregation.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use proc_macro2::Span;
use syn::spanned::Spanned;

pub type Key = Vec<String>;

#[derive(Debug, Clone)]
pub struct Lines {
    pub file: PathBuf,
    pub first: usize,
    pub last: usize,
}

impl Lines {
    pub fn of(file: &std::path::Path, item: &impl Spanned) -> Self {
        Self::from_span(file, item.span())
    }

    pub fn from_span(file: &std::path::Path, span: Span) -> Self {
        Self { file: file.into(), first: span.start().line, last: span.end().line }
    }
}

#[derive(Debug, Default)]
pub struct Scope {
    pub imports: BTreeMap<String, Vec<Key>>,
    pub globs: Vec<Key>,
    pub foreign_imports: BTreeSet<String>,
}

pub struct Definition {
    pub lines: Vec<Lines>,
    pub alias: Option<(Key, syn::Type)>,
}

pub struct Implementation {
    pub scope: Key,
    pub self_type: syn::Type,
    pub generic_types: BTreeSet<String>,
    pub inherent: bool,
    pub lines: Lines,
}

#[derive(Default)]
pub struct Sources {
    pub scopes: BTreeMap<Key, Scope>,
    pub definitions: BTreeMap<Key, Definition>,
    pub implementations: Vec<Implementation>,
    pub errors: Vec<String>,
    pub foreign_roots: BTreeSet<String>,
}

impl Sources {
    pub fn define(&mut self, scope: &Key, name: &syn::Ident, lines: Lines, alias: Option<syn::Type>) {
        let mut key = scope.clone();
        key.push(name.to_string());
        let definition = self.definitions.entry(key).or_insert_with(|| Definition { lines: Vec::new(), alias: None });
        definition.lines.push(lines);
        if let Some(alias) = alias {
            definition.alias = Some((scope.clone(), alias));
        }
    }
}
