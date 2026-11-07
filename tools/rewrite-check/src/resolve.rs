//! Resolve local self types and import aliases; ambiguity is a diagnostic, never a skip.

use std::collections::BTreeSet;

use crate::{
    model::{Implementation, Key, Sources},
    scan::absolute_import,
};

impl Sources {
    pub fn implementation_key(&self, implementation: &Implementation) -> Result<Option<Key>, String> {
        let resolved = self.resolve_type(&implementation.scope, &implementation.self_type, &mut BTreeSet::new())?;
        if resolved.is_some() {
            return Ok(resolved);
        }
        let ty = unwrap_reference(&implementation.self_type);
        if let syn::Type::Path(path) = ty {
            let first = path.path.segments.first().map(|part| part.ident.to_string()).unwrap_or_default();
            let known_foreign = [
                "String", "Vec", "Option", "Result", "Box", "bool", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32",
                "i64", "i128", "isize", "f32", "f64", "char",
            ];
            if !implementation.inherent
                && (known_foreign.contains(&first.as_str())
                    || implementation.generic_types.contains(&first)
                    || self.foreign_path(&implementation.scope, &path.path, &mut BTreeSet::new()))
            {
                return Ok(None);
            }
            return Err(format!("cannot resolve self type {first}; use an explicit local path or declare its import"));
        }
        if implementation.inherent { Err("cannot resolve inherent implementation self type".into()) } else { Ok(None) }
    }

    fn foreign_path(&self, scope: &Key, path: &syn::Path, active: &mut BTreeSet<Key>) -> bool {
        let parts: Key = path.segments.iter().map(|segment| segment.ident.to_string()).collect();
        let Some(first) = parts.first() else { return false };
        let key = [scope.clone(), parts.clone()].concat();
        if !active.insert(key) {
            return false;
        }
        if self.foreign_roots.contains(first) && !self.scopes.contains_key(&[scope[..1].to_vec(), vec![first.clone()]].concat()) {
            return true;
        }
        let mut current = scope.clone();
        loop {
            if let Some(imports) = self.scopes.get(&current).and_then(|scope| scope.imports.get(first)) {
                if self.scopes.get(&current).is_some_and(|module| module.foreign_imports.contains(first)) {
                    return true;
                }
                for target in imports {
                    if let Some((last, parent)) = target.split_last()
                        && let Ok(path) = syn::parse_str::<syn::Path>(last)
                        && self.foreign_path(&parent.to_vec(), &path, active)
                    {
                        return true;
                    }
                }
            }
            if !current.last().is_some_and(|part| part.starts_with("@local:")) {
                break;
            }
            current.pop();
        }
        false
    }

    pub fn resolve_type(&self, scope: &Key, ty: &syn::Type, active: &mut BTreeSet<Key>) -> Result<Option<Key>, String> {
        match unwrap_reference(ty) {
            syn::Type::Path(path) if path.qself.is_none() => self.resolve_path(scope, &path.path, active),
            _ => Ok(None),
        }
    }

    fn resolve_path(&self, scope: &Key, path: &syn::Path, active: &mut BTreeSet<Key>) -> Result<Option<Key>, String> {
        let parts: Key = path.segments.iter().map(|segment| segment.ident.to_string()).collect();
        let Some(first) = parts.first() else { return Ok(None) };
        if ["crate", "self", "super"].contains(&first.as_str()) {
            return self.resolve_qualified(&absolute_import(scope, &parts), active);
        }
        let mut current = scope.clone();
        loop {
            let mut candidates = BTreeSet::new();
            if let Some(key) = self.resolve_qualified(&[current.clone(), parts.clone()].concat(), active)? {
                candidates.insert(key);
            }
            if let Some(module) = self.scopes.get(&current) {
                for base in &module.globs {
                    if let Some(key) = self.resolve_qualified(&[base.clone(), parts.clone()].concat(), active)? {
                        candidates.insert(key);
                    }
                }
            }
            if candidates.len() > 1 {
                return Err(format!("ambiguous local type {} in {}", parts.join("::"), current.join("::")));
            }
            if let Some(key) = candidates.into_iter().next() {
                return Ok(Some(key));
            }
            // Only lexical local-item scopes inherit unqualified names. Modules do not.
            if !current.last().is_some_and(|part| part.starts_with("@local:")) {
                break;
            }
            current.pop();
        }
        Ok(None)
    }

    fn resolve_qualified(&self, key: &Key, active: &mut BTreeSet<Key>) -> Result<Option<Key>, String> {
        if active.len() >= 64 {
            return Err("type/import resolution exceeded 64 steps; use explicit qualified local paths".into());
        }
        if !active.insert(key.clone()) {
            return Err(format!("cyclic type/import alias {}", key.join("::")));
        }
        let result = self.resolve_qualified_inner(key, active);
        active.remove(key);
        result
    }

    fn resolve_qualified_inner(&self, key: &Key, active: &mut BTreeSet<Key>) -> Result<Option<Key>, String> {
        if let Some(definition) = self.definitions.get(key) {
            if let Some((scope, alias)) = &definition.alias
                && let Some(target) = self.resolve_type(scope, alias, active)?
            {
                return Ok(Some(target));
            }
            return Ok(Some(key.clone()));
        }
        let mut candidates = BTreeSet::new();
        for index in 1..key.len() {
            let scope = &key[..index];
            if let Some(module) = self.scopes.get(scope) {
                // A value export can share a module name. It cannot redirect an
                // already-bound module prefix in a type path into itself.
                let bound_module = self.scopes.contains_key(&key[..=index]);
                if !bound_module && let Some(imports) = module.imports.get(&key[index]) {
                    for target in imports {
                        let target = [target.clone(), key[index + 1..].to_vec()].concat();
                        if let Some(found) = self.resolve_qualified(&target, active)? {
                            candidates.insert(found);
                        }
                    }
                }
                // Existing module prefixes are already bound: a root glob must not
                // reinterpret `models::Missing` as `models::models::Missing`.
                if !bound_module {
                    for base in &module.globs {
                        if let Some(found) = self.resolve_qualified(&[base.clone(), key[index..].to_vec()].concat(), active)? {
                            candidates.insert(found);
                        }
                    }
                }
            }
        }
        if candidates.len() > 1 {
            return Err(format!("ambiguous local import {}", key.join("::")));
        }
        Ok(candidates.into_iter().next())
    }
}

fn unwrap_reference(ty: &syn::Type) -> &syn::Type {
    match ty {
        syn::Type::Reference(reference) => unwrap_reference(&reference.elem),
        syn::Type::Paren(paren) => unwrap_reference(&paren.elem),
        syn::Type::Group(group) => unwrap_reference(&group.elem),
        _ => ty,
    }
}
