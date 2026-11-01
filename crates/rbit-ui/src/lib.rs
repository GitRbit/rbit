//! Shared presentation primitives for forge pages and trusted feature templates.
//!
//! Locale scopes, view models and URL construction remain independent of HTTP,
//! host service composition and optional feature implementations. Hosts retain
//! authentication, authorization, request handling and full-page shell assembly.

pub mod blocks;
pub mod components;
pub mod i18n;
pub mod pagination;
pub mod template;
pub mod text;
pub mod urls;
