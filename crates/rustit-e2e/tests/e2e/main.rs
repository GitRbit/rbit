//! End-to-end tests: every test boots its own server (see `rustit_e2e::TestServer`) and
//! drives it over HTTP and with the real `git` CLI. One test binary keeps link times
//! down; each area is a module.

mod access;
mod accounts;
mod api;
mod compatibility;
mod edge;
mod essentials;
mod harness;
mod orgs;
mod repositories;
mod security;
mod stars;
mod themes;
mod tokens;
mod visibility;
