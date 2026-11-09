//! Source-only guard against the retired, unadmitted password facade in any crate.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use proc_macro2::{Ident, TokenStream, TokenTree};
use serde::Serialize;
use syn::{UseTree, spanned::Spanned};

#[derive(Debug, Serialize)]
pub struct Diagnostic {
    pub file: PathBuf,
    pub line: usize,
    pub message: String,
}

#[derive(Debug, Default, Serialize)]
pub struct Report {
    pub files: usize,
    pub violations: Vec<Diagnostic>,
}

#[derive(Debug)]
struct Wildcard {
    path: Vec<String>,
    file: PathBuf,
    line: usize,
}

#[derive(Debug, Default)]
struct Imports {
    aliases: Vec<(String, Vec<String>)>,
    wildcards: Vec<Wildcard>,
}

/// The facade is retired, so every Rust file of every workspace crate is checked:
/// sources, integration tests, benches, examples and build scripts. The online service
/// and host source roots remain required anchors, so a moved tree fails closed.
pub fn check(repository: &Path) -> Report {
    let mut report = Report::default();
    let mut files = Vec::new();
    let anchors: Vec<_> =
        ["rbit-services", "rbit-server"].into_iter().filter_map(|member| source_root(repository, member, &mut report)).collect();
    let crates = repository.join("crates");
    if fs::symlink_metadata(&crates).is_ok_and(|metadata| !linked(&metadata)) {
        discover(&crates, &mut files, &mut report);
    }
    for root in anchors {
        if !files.iter().any(|file| file.starts_with(&root)) {
            diagnostic(&mut report, &root, 1, "no Rust sources found under required production source root".into());
        }
    }
    files.sort();
    let mut imports = Imports::default();
    for file in files {
        report.files += 1;
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(error) => {
                diagnostic(&mut report, &file, 1, error.to_string());
                continue;
            }
        };
        if let Err(error) = syn::parse_file(&text) {
            diagnostic(&mut report, &file, error.span().start().line, format!("cannot parse Rust source: {error}"));
            continue;
        }
        match text.parse() {
            Ok(tokens) => inspect(tokens, &file, &mut imports, &mut report),
            Err(error) => diagnostic(&mut report, &file, 1, format!("cannot tokenize Rust source: {error}")),
        }
    }
    reject_wildcards(imports, &mut report);
    report.violations.sort_by(|a, b| (&a.file, a.line, &a.message).cmp(&(&b.file, b.line, &b.message)));
    report.violations.dedup_by(|a, b| a.file == b.file && a.line == b.line && a.message == b.message);
    report
}

fn source_root(repository: &Path, member: &str, report: &mut Report) -> Option<PathBuf> {
    // The caller explicitly chooses the repository anchor, which may itself be an
    // alias. Only that anchor is trusted: inspect every descendant before traversing
    // it, rather than letting lstat(src) follow a linked crates/member ancestor.
    let mut cursor = repository.to_path_buf();
    for component in ["crates", member, "src"] {
        cursor.push(component);
        let metadata = match fs::symlink_metadata(&cursor) {
            Ok(metadata) => metadata,
            Err(error) => {
                diagnostic(report, &cursor, 1, error.to_string());
                return None;
            }
        };
        if linked(&metadata) {
            diagnostic(report, &cursor, 1, "linked source ancestors are refused below the repository anchor".into());
            return None;
        }
        if !metadata.is_dir() {
            diagnostic(report, &cursor, 1, "expected a production source directory".into());
            return None;
        }
    }
    Some(cursor)
}

fn diagnostic(report: &mut Report, file: &Path, line: usize, message: String) {
    report.violations.push(Diagnostic { file: file.to_owned(), line: line.max(1), message });
}

fn linked(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn discover(path: &Path, files: &mut Vec<PathBuf>, report: &mut Report) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            diagnostic(report, path, 1, error.to_string());
            return;
        }
    };
    if linked(&metadata) {
        diagnostic(report, path, 1, "linked source entries are refused; the password boundary does not follow links".into());
    } else if metadata.is_dir() {
        let entries = match fs::read_dir(path)
            .and_then(|entries| entries.map(|entry| entry.map(|entry| entry.path())).collect::<Result<Vec<_>, _>>())
        {
            Ok(entries) => entries,
            Err(error) => {
                diagnostic(report, path, 1, error.to_string());
                return;
            }
        };
        for entry in entries {
            discover(&entry, files, report);
        }
    } else if path.extension().is_some_and(|extension| extension == "rs") {
        files.push(path.to_owned());
    }
}

fn name(identifier: &Ident) -> String {
    identifier.to_string().trim_start_matches("r#").to_owned()
}

fn inspect(stream: TokenStream, file: &Path, imports: &mut Imports, report: &mut Report) {
    let tokens: Vec<_> = stream.into_iter().collect();
    for (index, token) in tokens.iter().enumerate() {
        match token {
            TokenTree::Ident(identifier) if name(identifier) == "Passwords" => {
                diagnostic(
                    report,
                    file,
                    identifier.span().start().line,
                    "retired legacy Passwords facade is forbidden; use admitted PasswordWork".into(),
                );
            }
            TokenTree::Group(group) => inspect(group.stream(), file, imports, report),
            _ => {}
        }
        // Read authored imports inside ordinary code and opaque macro groups alike.
        // Dynamic/generated macro paths are not expanded or resolved by this guard.
        if let TokenTree::Ident(identifier) = token
            && matches!(identifier.to_string().as_str(), "use" | "extern")
            && let Some(end) = tokens[index..].iter().position(|token| matches!(token, TokenTree::Punct(punct) if punct.as_char() == ';'))
        {
            let item: TokenStream = tokens[index..=index + end].iter().cloned().collect();
            if let Ok(item) = syn::parse2::<syn::ItemUse>(item.clone()) {
                record_import(&item.tree, Vec::new(), file, imports);
            } else if let Ok(item) = syn::parse2::<syn::ItemExternCrate>(item) {
                let alias = item.rename.as_ref().map_or(&item.ident, |(_, alias)| alias);
                imports.aliases.push((name(alias), vec![name(&item.ident)]));
            }
        }
    }
}

fn record_import(tree: &UseTree, prefix: Vec<String>, file: &Path, imports: &mut Imports) {
    match tree {
        UseTree::Path(path) => {
            let mut prefix = prefix;
            prefix.push(name(&path.ident));
            record_import(&path.tree, prefix, file, imports);
        }
        UseTree::Group(group) => {
            for item in &group.items {
                record_import(item, prefix.clone(), file, imports);
            }
        }
        UseTree::Name(item) => {
            let mut path = prefix;
            if item.ident != "self" {
                path.push(name(&item.ident));
            }
            if let Some(alias) = path.last() {
                imports.aliases.push((alias.clone(), path));
            }
        }
        UseTree::Rename(item) => {
            let mut path = prefix;
            if item.ident != "self" {
                path.push(name(&item.ident));
            }
            imports.aliases.push((name(&item.rename), path));
        }
        UseTree::Glob(item) => {
            imports.wildcards.push(Wildcard { path: prefix, file: file.to_owned(), line: item.star_token.span().start().line });
        }
    }
}

fn reject_wildcards(imports: Imports, report: &mut Report) {
    let mut namespaces: BTreeSet<_> = ["rbit_auth".to_owned(), "rbit_identity".to_owned()].into();
    loop {
        let before = namespaces.len();
        for (alias, path) in &imports.aliases {
            if path.iter().any(|part| namespaces.contains(part)) {
                namespaces.insert(alias.clone());
            }
        }
        if namespaces.len() == before {
            break;
        }
    }
    for wildcard in imports.wildcards {
        if wildcard.path.iter().any(|part| namespaces.contains(part)) {
            diagnostic(
                report,
                &wildcard.file,
                wildcard.line,
                "auth/identity namespace wildcard can hide the legacy Passwords facade; use explicit imports".into(),
            );
        }
    }
}

#[cfg(test)]
mod tests;
