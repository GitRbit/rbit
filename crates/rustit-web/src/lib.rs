//! # rustit-web
//!
//! The HTTP face of Rustit. Server-rendered HTML (Askama) progressively enhanced with
//! htmx, plus the Git smart-HTTP endpoints.
//!
//! Module map:
//! * [`routes`] — one module per URL area; handlers are thin and delegate to services.
//! * [`views`] — Askama template structs and the view models they render.
//! * [`render`] — Markdown and syntax highlighting.
//! * [`middleware`] — sessions, security headers, CSRF origin checks, error pages.
//! * [`extract`] — request extractors (`Viewer`, `SignedIn`, `Page`).

pub mod assets;
pub mod client_ip;
pub mod error;
pub mod extract;
pub mod metrics;
pub mod middleware;
pub mod render;
pub mod routes;
pub mod state;
pub mod urls;
pub mod views;

pub use routes::router;
pub use state::AppState;
