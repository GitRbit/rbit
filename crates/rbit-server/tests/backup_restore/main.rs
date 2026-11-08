//! The backup and restore regression group: a realistic instance backed up quiescent and
//! during writes restores into a fresh data directory with its repositories, stored files,
//! rows and sealed credentials mutually consistent; interrupted, torn and newer backups,
//! non-empty targets and interrupted restores are refused.

mod consistency;
mod damaged;
mod in_flight;
mod instance;
mod round_trip;
