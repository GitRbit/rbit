//! End-to-end tests: every test boots its own server (see `rbit_e2e::TestServer`) and
//! drives it over HTTP and with the real `git` CLI. One test binary keeps link times
//! down; each area is a module.

mod access;
mod accounts;
mod api;
mod compatibility;
mod edge;
mod essentials;
mod harness;
mod issues;
mod orgs;
mod protection;
mod pulls;
mod redirects;
mod repositories;
mod security;
mod stars;
mod themes;
mod timeline;
mod tokens;
mod visibility;
