//! Actions settings pages: the repository, organization and account settings, their
//! secrets and variables, deployment environments and runners. English only so far:
//! every other locale falls back (see [`super::fallback`]).
//!
//! Arguments carry names, times and code fragments; the web layer may style an
//! argument (a monospace path, a `<time>`) by finding it in the formatted text.
pub mod environments;
pub mod runners;
pub mod secrets;
pub mod settings;
