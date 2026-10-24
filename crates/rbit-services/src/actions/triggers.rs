//! Starting runs from domain events: `RepositoryPushed` starts push runs and refreshes
//! schedules, pull request events start pull request runs and cancel those of closed pull
//! requests. Evaluation (Git reads and YAML) runs on the bounded evaluation semaphore inside
//! `spawn_blocking`, never on the request path.
//!
//! Workstream T (triggers and runs) owns this file.
