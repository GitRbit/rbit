//! Askama template structs and the view models they render.
//!
//! Every page struct holds a [`layout::Page`] (shared chrome: title, navigation, assets)
//! plus page-specific data that is already shaped for display — templates contain no
//! business logic and never build URLs by hand.

pub mod account;
pub mod admin;
pub mod auth;
pub mod blame;
pub mod collaborators;
pub mod commit;
pub mod compare;
pub mod components;
pub mod errors;
pub mod forks;
pub mod home;
pub mod insights;
pub mod issues;
pub mod labels;
pub mod layout;
pub mod milestones;
pub mod mirrors;
pub mod notifications;
pub mod org;
pub mod protection;
pub mod pulls;
pub mod pwa;
pub mod recovery;
pub mod refs;
pub mod releases;
pub mod repo;
pub mod repo_settings;
pub mod security;
pub mod settings;
pub mod ssh_keys;
pub mod teams;
pub mod themes;
pub mod timeline;
pub mod transfer;
pub mod webhooks;
