//! Secrets and variables: the key file (`<data_dir>/actions/secrets.key`, never in SQLite),
//! sealing and opening with AES-256-GCM bound to the secret's scope and id, owner and
//! repository CRUD, and what a task receives.
//!
//! Until workstream U lands, no secret can be stored, so a task receives none.
//!
//! Workstream U (interface and secrets) owns this file.

use std::collections::BTreeMap;

use rbit_core::actions::Run;

use super::ActionsService;
use crate::ServiceError;

impl ActionsService {
    /// The secrets a task of `run` receives, by name. Untrusted runs receive none.
    ///
    /// Reading and opening stored secrets will await; until secrets can be stored there are
    /// none, so the answer is ready at once.
    pub fn task_secrets(&self, run: &Run) -> impl Future<Output = Result<BTreeMap<String, String>, ServiceError>> + Send + use<> {
        let _ = (self, run);
        std::future::ready(Ok(BTreeMap::new()))
    }
}
