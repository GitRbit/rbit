//! Static edit port; `update_text` retains its existing immediate transaction and SQL.

use rbit_collaboration::threads::{Issue, IssueId, IssueText, IssueTitle, ThreadTextRepository};
use rbit_core::{AccountId, Timestamp};

use crate::{Database, StorageError};

// Borrow the database through this narrow port so rejected/invalid edits do not
// even clone an IssueStore pool handle. Materialize it only at the original write.
impl ThreadTextRepository for Database {
    type Error = StorageError;
    async fn update_text(
        &self,
        id: IssueId,
        title: Option<&IssueTitle>,
        body: Option<&IssueText>,
        actor: AccountId,
        at: Timestamp,
    ) -> Result<Option<Issue>, Self::Error> {
        self.issues().update_text(id, title, body, actor, at).await
    }
}
