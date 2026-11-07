//! Windows-first implementation; Unix namespace adapter is a separate release gate.
mod binding;
mod directory;
mod native;
mod published;
mod staged;
#[cfg(test)]
mod tests;

pub use directory::OwnedDirectory;
pub use published::PublishedFile;
pub use staged::StagedFile;
