//! Discover compilation targets and Rust module files without invoking a compiler.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use syn::{Item, UseTree, visit::Visit};

use crate::model::{Implementation, Key, Lines, Sources};

pub fn read_crate(root: &Path) -> Sources {
    let mut sources = Sources::default();
    let mut entries = Vec::new();
    for (name, path) in [("lib", root.join("src/lib.rs")), ("main", root.join("src/main.rs"))] {
        if path.is_file() {
            entries.push((name.to_owned(), path));
        }
    }
    discover_entries(&root.join("tests"), "test", &mut entries);
    discover_entries(&root.join("src/bin"), "bin", &mut entries);
    discover_entries(&root.join("examples"), "example", &mut entries);
    discover_entries(&root.join("benches"), "bench", &mut entries);
    read_manifest(root, &mut sources, &mut entries);
    entries.sort();
    entries.dedup();
    if entries.is_empty() {
        sources.errors.push(format!("{}: no conventional Rust target found", root.display()));
    }
    for (target, file) in entries {
        let directory = file.parent().unwrap_or(root).to_path_buf();
        read_file(&mut sources, &vec![target], &file, &directory, &mut BTreeSet::new());
    }
    sources
}

fn discover_entries(directory: &Path, prefix: &str, entries: &mut Vec<(String, PathBuf)>) {
    let Ok(files) = fs::read_dir(directory) else { return };
    for file in files {
        let Ok(file) = file else { continue };
        let path = file.path();
        if path.extension().is_some_and(|extension| extension == "rs") {
            if let Some(name) = path.file_stem().and_then(|name| name.to_str()) {
                entries.push((format!("{prefix}:{name}"), path));
            }
        } else if prefix == "bin" && path.join("main.rs").is_file() {
            let name = file.file_name().to_string_lossy().into_owned();
            entries.push((format!("bin:{name}"), path.join("main.rs")));
        }
    }
}

fn read_file(sources: &mut Sources, scope: &Key, file: &Path, module_dir: &Path, active: &mut BTreeSet<PathBuf>) {
    let canonical = match file.canonicalize() {
        Ok(file) => file,
        Err(error) => {
            sources.errors.push(format!("{}: {error}", file.display()));
            return;
        }
    };
    if !active.insert(canonical.clone()) {
        sources.errors.push(format!("{}: recursive module file", file.display()));
        return;
    }
    match fs::read_to_string(file)
        .map_err(|error| error.to_string())
        .and_then(|text| syn::parse_file(&text).map_err(|error| error.to_string()))
    {
        Ok(parsed) => scan_items(sources, scope, &parsed.items, file, module_dir, false, active),
        Err(error) => sources.errors.push(format!("{}: {error}", file.display())),
    }
    active.remove(&canonical);
}

fn scan_items(
    sources: &mut Sources,
    scope: &Key,
    items: &[Item],
    file: &Path,
    module_dir: &Path,
    inside_inline: bool,
    active: &mut BTreeSet<PathBuf>,
) {
    sources.scopes.entry(scope.clone()).or_default();
    for item in items {
        match item {
            Item::Struct(item) => sources.define(scope, &item.ident, Lines::of(file, item), None),
            Item::Enum(item) => sources.define(scope, &item.ident, Lines::of(file, item), None),
            Item::Union(item) => sources.define(scope, &item.ident, Lines::of(file, item), None),
            Item::Trait(item) => sources.define(scope, &item.ident, Lines::of(file, item), None),
            Item::Type(item) => sources.define(scope, &item.ident, Lines::of(file, item), Some(*item.ty.clone())),
            Item::Use(item) => record_use(sources, scope, &item.tree, Vec::new()),
            Item::Impl(implementation) => {
                sources.implementations.push(Implementation {
                    scope: scope.clone(),
                    self_type: *implementation.self_ty.clone(),
                    generic_types: implementation.generics.type_params().map(|parameter| parameter.ident.to_string()).collect(),
                    inherent: implementation.trait_.is_none(),
                    lines: Lines::of(file, implementation),
                });
                scan_nested(sources, scope, item, file, module_dir, inside_inline, active);
            }
            Item::Mod(item) => {
                if item.attrs.iter().any(|attr| attr.path().is_ident("cfg_attr")) {
                    sources
                        .errors
                        .push(format!("{}: cfg_attr on a module needs explicit module paths for structural checking", file.display()));
                }
                let mut child = scope.clone();
                child.push(item.ident.to_string());
                let child_dir = module_dir.join(item.ident.to_string());
                if let Some((_, items)) = &item.content {
                    if item.attrs.iter().any(|attr| attr.path().is_ident("path")) {
                        sources.errors.push(format!("{}: #[path] on an inline module needs explicit support", file.display()));
                    }
                    scan_items(sources, &child, items, file, &child_dir, true, active);
                } else {
                    let custom = item.attrs.iter().find(|attr| attr.path().is_ident("path"));
                    if let Some(custom) = custom {
                        if let syn::Meta::NameValue(value) = &custom.meta
                            && let syn::Expr::Lit(value) = &value.value
                            && let syn::Lit::Str(value) = &value.lit
                        {
                            let base = if inside_inline { module_dir } else { file.parent().unwrap_or(module_dir) };
                            let path = base.join(value.value());
                            let directory = path.parent().unwrap_or(module_dir);
                            read_file(sources, &child, &path, directory, active);
                            continue;
                        }
                        sources.errors.push(format!("{}: unsupported #[path] attribute", file.display()));
                        continue;
                    }
                    let flat = module_dir.join(format!("{}.rs", item.ident));
                    let nested = child_dir.join("mod.rs");
                    if flat.is_file() && nested.is_file() {
                        sources.errors.push(format!("{}: ambiguous module {}", file.display(), item.ident));
                    } else {
                        let path = if flat.is_file() { flat } else { nested };
                        read_file(sources, &child, &path, &child_dir, active);
                    }
                }
            }
            _ => scan_nested(sources, scope, item, file, module_dir, inside_inline, active),
        }
        if matches!(item, Item::Struct(_) | Item::Enum(_) | Item::Union(_) | Item::Trait(_) | Item::Type(_)) {
            scan_nested(sources, scope, item, file, module_dir, inside_inline, active);
        }
    }
}

fn read_manifest(root: &Path, sources: &mut Sources, entries: &mut Vec<(String, PathBuf)>) {
    let path = root.join("Cargo.toml");
    let manifest = match fs::read_to_string(&path)
        .map_err(|error| error.to_string())
        .and_then(|text| toml::from_str::<toml::Value>(&text).map_err(|error| error.to_string()))
    {
        Ok(value) => value,
        Err(error) => {
            sources.errors.push(format!("{}: {error}", path.display()));
            return;
        }
    };
    // Cargo excludes conventional test files when autotests=false; explicit targets
    // below remain authoritative. Scanning disabled files as extra crate roots gives
    // their nested modules the wrong parent and can miss actual distributed impls.
    if manifest.get("package").and_then(|package| package.get("autotests")).and_then(toml::Value::as_bool) == Some(false) {
        entries.retain(|(name, _)| !name.starts_with("test:"));
    }
    sources.foreign_roots.extend(["std", "core", "alloc"].map(str::to_owned));
    if let Some(name) = manifest.get("package").and_then(|package| package.get("name")).and_then(toml::Value::as_str) {
        sources.foreign_roots.insert(name.replace('-', "_"));
    }
    if let Some(name) = manifest.get("lib").and_then(|library| library.get("name")).and_then(toml::Value::as_str) {
        sources.foreign_roots.insert(name.replace('-', "_"));
    }
    dependency_roots(&manifest, &mut sources.foreign_roots);
    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for target in targets.values() {
            dependency_roots(target, &mut sources.foreign_roots);
        }
    }
    if let Some(path) = manifest.get("lib").and_then(|library| library.get("path")).and_then(toml::Value::as_str) {
        entries.retain(|(name, _)| name != "lib");
        entries.push(("lib".into(), root.join(path)));
    }
    for kind in ["bin", "test", "example", "bench"] {
        if let Some(targets) = manifest.get(kind).and_then(toml::Value::as_array) {
            for target in targets {
                if let Some(path) = target.get("path").and_then(toml::Value::as_str) {
                    let name = target.get("name").and_then(toml::Value::as_str).unwrap_or(path);
                    entries.push((format!("{kind}:{name}"), root.join(path)));
                }
            }
        }
    }
}

fn dependency_roots(manifest: &toml::Value, roots: &mut BTreeSet<String>) {
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = manifest.get(kind).and_then(toml::Value::as_table) {
            roots.extend(table.keys().map(|name| name.replace('-', "_")));
        }
    }
}

/// Local item scopes include the enclosing syntax location, preventing collisions.
fn scan_nested(
    sources: &mut Sources,
    scope: &Key,
    item: &Item,
    file: &Path,
    module_dir: &Path,
    inside_inline: bool,
    active: &mut BTreeSet<PathBuf>,
) {
    struct Nested(Vec<Item>);
    impl<'ast> Visit<'ast> for Nested {
        fn visit_item(&mut self, item: &'ast Item) {
            self.0.push(item.clone());
        }
    }
    let mut nested = Nested(Vec::new());
    syn::visit::visit_item(&mut nested, item);
    if !nested.0.is_empty() {
        let line = Lines::of(file, item).first;
        let mut child = scope.clone();
        child.push(format!("@local:{line}"));
        scan_items(sources, &child, &nested.0, file, module_dir, inside_inline, active);
    }
}

fn record_use(sources: &mut Sources, scope: &Key, tree: &UseTree, prefix: Key) {
    match tree {
        UseTree::Path(path) => {
            let mut prefix = prefix;
            prefix.push(path.ident.to_string());
            record_use(sources, scope, &path.tree, prefix);
        }
        UseTree::Group(group) => {
            for tree in &group.items {
                record_use(sources, scope, tree, prefix.clone());
            }
        }
        UseTree::Name(name) => {
            let mut target = prefix;
            let alias = if name.ident == "self" { target.last().cloned().unwrap_or_default() } else { name.ident.to_string() };
            if name.ident != "self" {
                target.push(name.ident.to_string());
            }
            mark_foreign_import(sources, scope, &alias, &target);
            let target = absolute_import(scope, &target);
            sources.scopes.entry(scope.clone()).or_default().imports.entry(alias).or_default().push(target);
        }
        UseTree::Rename(rename) => {
            let mut target = prefix;
            if rename.ident != "self" {
                target.push(rename.ident.to_string());
            }
            mark_foreign_import(sources, scope, &rename.rename.to_string(), &target);
            let target = absolute_import(scope, &target);
            sources.scopes.entry(scope.clone()).or_default().imports.entry(rename.rename.to_string()).or_default().push(target);
        }
        UseTree::Glob(_) => {
            sources.scopes.entry(scope.clone()).or_default().globs.push(absolute_import(scope, &prefix));
        }
    }
}

fn mark_foreign_import(sources: &mut Sources, scope: &Key, alias: &str, path: &Key) {
    if path.first().is_some_and(|root| sources.foreign_roots.contains(root)) {
        sources.scopes.entry(scope.clone()).or_default().foreign_imports.insert(alias.to_owned());
    }
}

pub fn absolute_import(scope: &Key, path: &Key) -> Key {
    match path.first().map(String::as_str) {
        Some("crate") => [scope[..1].to_vec(), path[1..].to_vec()].concat(),
        Some("self") => [scope.clone(), path[1..].to_vec()].concat(),
        Some("super") => {
            let count = path.iter().take_while(|part| part.as_str() == "super").count();
            let keep = scope.len().saturating_sub(count).max(1);
            [scope[..keep].to_vec(), path[count..].to_vec()].concat()
        }
        _ => [scope[..1].to_vec(), path.clone()].concat(),
    }
}
