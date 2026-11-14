//! Git LFS object links for repository templates.

use rbit_core::{RepoId, lfs::LfsPointer};

use super::LfsService;
use crate::{RepoContext, ServiceError};

impl LfsService {
    /// A template inherits objects referenced by the tree and linked to the source
    /// or a publicly readable member of its fork network.
    pub(crate) async fn copy_template_objects(
        &self,
        source: &RepoContext,
        target: RepoId,
        pointers: Vec<LfsPointer>,
    ) -> Result<(), ServiceError> {
        let _placing = self.inner.objects.lock().await;
        let db = self.db();
        let oids: Vec<_> = pointers.iter().map(|pointer| pointer.oid.clone()).collect();
        let linked = db.linked_objects(source.repo.repo.id, &oids).await?;
        let mut public_network = None;
        for pointer in pointers {
            let mut size = linked.get(&pointer.oid).copied();
            if size.is_none() {
                if public_network.is_none() {
                    let network = crate::forks::network(&self.inner.ctx.database, None, source.repo.repo.id).await?;
                    public_network = Some(network.into_iter().map(|repo| repo.repo.id).collect::<Vec<_>>());
                }
                let public_network = public_network.as_deref().unwrap_or_default();
                if db.linked_in_any(public_network, &pointer.oid).await?.is_some() {
                    size = db.find_object(&pointer.oid).await?.map(|object| object.size);
                }
            }
            if size == Some(pointer.size) && self.bytes_present(&pointer.oid).await {
                db.link(target, &pointer.oid).await?;
            }
        }
        Ok(())
    }
}
