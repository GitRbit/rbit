//! Object facts for [`rbit_repositories::lfs::decide_batch`]: links, the fork network
//! (read once per batch) and bytes on disk.
use std::{collections::HashMap, sync::Mutex};

use rbit_core::{RepoId, User, lfs::LfsOid};
use rbit_repositories::lfs::BatchPort;

use super::LfsService;
use crate::{RepoContext, ServiceError};

pub(super) struct BatchView<'a> {
    pub service: &'a LfsService,
    pub repo: &'a RepoContext,
    pub viewer: Option<&'a User>,
    /// The readable fork network, cached across the objects of one batch.
    pub network: Mutex<Option<Vec<RepoId>>>,
}

impl BatchPort for BatchView<'_> {
    type Oid = LfsOid;
    type Error = ServiceError;

    fn parse_oid(&self, oid: &str) -> Result<LfsOid, String> {
        LfsOid::parse(oid).map_err(|error| error.to_string())
    }

    async fn linked(&self, oids: &[LfsOid]) -> Result<HashMap<LfsOid, u64>, ServiceError> {
        Ok(self.service.db().linked_objects(self.repo.repo.repo.id, oids).await?)
    }

    async fn through_network(&self, oid: &LfsOid) -> Result<Option<u64>, ServiceError> {
        let mut network = self.network.lock().expect("network cache").take();
        let found = self.service.through_network(self.repo, self.viewer, oid, &mut network).await;
        *self.network.lock().expect("network cache") = network;
        found
    }

    async fn bytes_present(&self, oid: &LfsOid) -> bool {
        self.service.bytes_present(oid).await
    }
}
