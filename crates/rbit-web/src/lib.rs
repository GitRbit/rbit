//! # rbit-web
//!
//! The HTTP face of rbit. Server-rendered HTML (Askama) progressively enhanced with
//! htmx, plus the Git smart-HTTP endpoints.
//!
//! Module map:
//! * [`routes`] — one module per URL area; handlers are thin and delegate to services.
//! * [`views`] — Askama template structs and the view models they render.
//! * [`render`] — Markdown and syntax highlighting.
//! * [`middleware`] — sessions, security headers, CSRF origin checks, error pages.
//! * [`extract`] — request extractors (`Viewer`, `SignedIn`, `Page`).
//! * [`themes`] — the colour theme catalog and the theme cookie.
//! * [`timing`] — the footer's per-request render time.
//! * [`partial`] — partial responses for navigation inside a repository.
//! * `supervise` — idle and time limits for Git exchanges and archive downloads.

pub mod assets;
pub mod client_ip;
pub mod error;
pub mod extract;
pub mod i18n;
pub mod metrics;
pub mod middleware;
pub mod partial;
pub mod render;
pub mod routes;
pub mod state;
mod supervise;
pub mod themes;
pub mod timing;
pub mod updates;
pub mod urls;
pub mod views;

pub use routes::router;
pub use state::AppState;
