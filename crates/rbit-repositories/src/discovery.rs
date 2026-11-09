//! Public repository discovery policy behind a static, persistence-independent port.

mod application;
mod listings;
mod page;
mod repository;

pub use application::{EXPLORE_PAGE_SIZE, RepositoryDiscovery};
pub use listings::{DASHBOARD_LIMIT, RepositoryListingCatalog, RepositoryListings};
pub use page::{RepoSearch, RepoSearchResults};
pub use repository::PublicRepositoryCatalog;
