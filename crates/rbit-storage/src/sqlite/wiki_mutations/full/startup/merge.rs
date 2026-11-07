//! Merge two separately bounded index ranges, preserving orphan identity exactly.
use super::{CatalogRow, PendingRow, WikiStartupScope, WikiWriteError};
use rbit_core::RepoId;
use rbit_wikis::{WikiGeneration, WikiId, policy::WikiHead};

pub(super) fn scopes(
    catalog: Vec<CatalogRow>,
    pending: Vec<PendingRow>,
    limit: usize,
) -> Result<(Vec<WikiStartupScope>, bool), WikiWriteError> {
    let mut catalog = catalog.into_iter().peekable();
    let mut pending = pending.into_iter().peekable();
    let mut result = Vec::with_capacity(limit);
    while result.len() < limit {
        let Some(wiki) = catalog.peek().map(|row| row.wiki).into_iter().chain(pending.peek().map(|row| row.wiki)).min() else {
            break;
        };
        let c = if catalog.peek().is_some_and(|row| row.wiki == wiki) { catalog.next() } else { None };
        let p = if pending.peek().is_some_and(|row| row.wiki == wiki) { pending.next() } else { None };
        let wiki = WikiId::new(wiki).ok_or(WikiWriteError::Binding)?;
        let repository =
            c.as_ref().map(|row| row.repository).or_else(|| p.as_ref().map(|row| row.repository)).ok_or(WikiWriteError::Binding)?;
        if repository <= 0 || p.as_ref().is_some_and(|row| row.repository != repository) {
            return Err(WikiWriteError::Binding);
        }
        let enabled = match c.as_ref().map(|row| row.enabled) {
            None | Some(0) => false,
            Some(1) => true,
            _ => return Err(WikiWriteError::Binding),
        };
        let head = match c.as_ref().map(|row| (&row.generation, &row.revision)) {
            None | Some((None, None)) => None,
            Some((Some(generation), Some(revision))) => Some(WikiHead {
                generation: WikiGeneration::new(*generation).ok_or(WikiWriteError::Binding)?,
                revision: revision.parse().map_err(|_| WikiWriteError::Binding)?,
            }),
            _ => return Err(WikiWriteError::Binding),
        };
        result.push(WikiStartupScope { wiki, repository: RepoId(repository), enabled, head, pending: p.is_some(), orphaned: c.is_none() });
    }
    Ok((result, catalog.peek().is_some() || pending.peek().is_some()))
}
