//! Which repository of a fork network a head specification (`owner:branch` or
//! `owner/name:branch`) names.

use rbit_repositories::OwnedRepository;

/// The repository of `network` that `owner` (and `name`, when given) names,
/// case-insensitively. Without a name, `owner`'s repository named like the base
/// (`base_name`, exactly) comes first, then their first one in `network` order.
pub fn select_head<'a>(network: &'a [OwnedRepository], owner: &str, name: Option<&str>, base_name: &str) -> Option<&'a OwnedRepository> {
    let candidates: Vec<&OwnedRepository> = network.iter().filter(|repo| repo.owner.name.as_str().eq_ignore_ascii_case(owner)).collect();
    match name {
        Some(name) => candidates.into_iter().find(|repo| repo.repo.name.as_str().eq_ignore_ascii_case(name)),
        None => candidates.iter().find(|repo| repo.repo.name.as_str() == base_name).or_else(|| candidates.first()).copied(),
    }
}
