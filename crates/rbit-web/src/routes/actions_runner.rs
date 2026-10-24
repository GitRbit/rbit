//! The runner endpoint: Forgejo Runner and Gitea `act_runner` speak Connect (binary
//! protobuf over `POST /api/actions/{service}/{method}`) to register, declare their labels,
//! fetch tasks and report state and logs.
//!
//! [`routes`] is merged into the application router only when `Services::actions` exists;
//! with Actions disabled these paths fall through to the API's JSON 404. It is merged outside
//! the page router: no session, no CSRF check, no HTML error pages. [`PATHS`] lists every
//! method this endpoint serves, for the disabled-is-disabled tests.
//!
//! The Actions foundation (workstream F) owns the route list; workstream R (runner protocol)
//! owns the codec and the handlers.

use axum::Router;

use crate::AppState;

/// Where the Connect services are mounted.
pub const PREFIX: &str = "/api/actions";

/// Every RPC of the endpoint, as a path below `/`.
pub const PATHS: &[&str] = &[
    "/api/actions/ping.v1.PingService/Ping",
    "/api/actions/runner.v1.RunnerService/Register",
    "/api/actions/runner.v1.RunnerService/Declare",
    "/api/actions/runner.v1.RunnerService/FetchTask",
    "/api/actions/runner.v1.RunnerService/UpdateTask",
    "/api/actions/runner.v1.RunnerService/UpdateLog",
];

/// The runner endpoint. Merged only when Actions is enabled (see `routes::router`).
pub fn routes() -> Router<AppState> {
    Router::new()
}
