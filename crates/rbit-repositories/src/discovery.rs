//! Public repository discovery policy behind a static, persistence-independent port.

mod application;
mod page;
mod repository;

pub use application::{EXPLORE_PAGE_SIZE, RepositoryDiscovery};
pub use page::{RepoSearch, RepoSearchResults};
pub use repository::PublicRepositoryCatalog;
