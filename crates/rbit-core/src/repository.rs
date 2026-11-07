//! Compatibility paths for repository-owned metadata and listing contracts.

pub use rbit_repositories::{ARCHIVED_READ_ONLY, ArchivedFilter, OwnedRepository, RepoId, RepoSort, Repository, Visibility};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_models_are_the_same_feature_owned_types() {
        use std::any::TypeId;

        assert_eq!(TypeId::of::<crate::RepoName>(), TypeId::of::<rbit_repositories::RepoName>());
        assert_eq!(TypeId::of::<crate::names::RepoName>(), TypeId::of::<rbit_repositories::RepoName>());
        assert_eq!(TypeId::of::<crate::Repository>(), TypeId::of::<rbit_repositories::Repository>());
        assert_eq!(TypeId::of::<Repository>(), TypeId::of::<rbit_repositories::Repository>());
        assert_eq!(TypeId::of::<crate::OwnedRepository>(), TypeId::of::<rbit_repositories::OwnedRepository>());
        assert_eq!(TypeId::of::<OwnedRepository>(), TypeId::of::<rbit_repositories::OwnedRepository>());
        assert_eq!(TypeId::of::<crate::Visibility>(), TypeId::of::<rbit_repositories::Visibility>());
        assert_eq!(TypeId::of::<Visibility>(), TypeId::of::<rbit_repositories::Visibility>());
        assert_eq!(TypeId::of::<crate::RepoSort>(), TypeId::of::<rbit_repositories::RepoSort>());
        assert_eq!(TypeId::of::<RepoSort>(), TypeId::of::<rbit_repositories::RepoSort>());
        assert_eq!(TypeId::of::<ArchivedFilter>(), TypeId::of::<rbit_repositories::ArchivedFilter>());
        assert_eq!(ARCHIVED_READ_ONLY, rbit_repositories::ARCHIVED_READ_ONLY);
    }

    #[test]
    fn legacy_repository_paths_remain_the_same_contract_type() {
        let module_path: super::RepoId = rbit_contracts::RepoId(-1);
        let root_path: crate::RepoId = super::RepoId(module_path.0);
        let contract: rbit_contracts::RepoId = root_path;
        assert_eq!(contract.0, -1);
        assert_eq!(std::any::TypeId::of::<crate::RepoId>(), std::any::TypeId::of::<rbit_contracts::RepoId>());
        assert_eq!(serde_json::to_string(&root_path).unwrap(), "-1");
    }

    #[test]
    fn archived_filters_round_trip() {
        for filter in ArchivedFilter::ALL {
            assert_eq!(ArchivedFilter::parse(filter.query_value().unwrap_or_default()), Some(filter));
        }
        assert_eq!(ArchivedFilter::parse("maybe"), None);
        assert!(ArchivedFilter::Include.shows(true) && ArchivedFilter::Include.shows(false));
        assert!(!ArchivedFilter::Exclude.shows(true) && ArchivedFilter::Only.shows(true) && !ArchivedFilter::Only.shows(false));
    }

    #[test]
    fn sort_round_trips() {
        for sort in RepoSort::ALL {
            assert_eq!(RepoSort::parse(sort.as_str()), Some(sort));
        }
        assert_eq!(RepoSort::parse("stars"), None);
        assert_eq!(RepoSort::default(), RepoSort::RecentlyUpdated);
    }
}
