//! Owned filesystem operations, not authentication or Wiki publication authority.
#![deny(unsafe_code)]

mod error;
mod name;
pub use error::PlatformError;
pub use name::Component;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{OwnedDirectory, PublishedFile, StagedFile};
