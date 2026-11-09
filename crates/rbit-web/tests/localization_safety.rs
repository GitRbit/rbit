//! The localization safety regression group (`docs/design/rewrite/security-contracts.md`,
//! group 6) through the real router: every catalog's documents and fragments name the
//! language they are rendered in, escape hostile text once, keep the strict CSP, and never
//! share a cache entry or validator with another catalog.

mod documents;
mod fixture;
mod fragments;
