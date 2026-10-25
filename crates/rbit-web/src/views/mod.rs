//! Askama template structs and the view models they render.
//!
//! Every page struct holds a [`layout::Page`] (shared chrome: title, navigation, assets)
//! plus page-specific data that is already shaped for display — templates contain no
//! business logic and never build URLs by hand.

pub mod account;
// Actions: interface and secrets (U).
pub mod actions;
pub mod admin;
pub mod auth;
pub mod blame;
pub mod blocks;
pub mod branch_graph;
pub mod collaborators;
pub mod commit;
pub mod compare;
pub mod components;
pub mod contributions;
pub mod dashboards;
pub mod edits;
pub mod errors;
// Milestone 3: external sign-in (O).
pub mod external_auth;
pub mod forks;
pub mod home;
// Milestone 3: importing and push mirrors (M).
pub mod imports;
pub mod insights;
pub mod issues;
pub mod labels;
pub mod layout;
pub mod milestones;
pub mod mirrors;
pub mod notification_email;
pub mod notifications;
pub mod org;
pub mod pagination;
pub mod protection;
pub mod pulls;
// Milestone 3: importing and push mirrors (M).
pub mod push_mirrors;
pub mod pwa;
pub mod recovery;
pub mod ref_picker;
pub mod refs;
pub mod releases;
pub mod repo;
pub mod repo_files;
pub mod repo_settings;
pub mod security;
pub mod settings;
pub mod ssh_keys;
// Milestone 3: commit statuses (S).
pub mod statuses;
pub mod teams;
pub mod themes;
pub mod timeline;
pub mod transfer;
pub mod webhooks;
