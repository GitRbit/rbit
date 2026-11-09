//! Git LFS decisions: batch admission and per-object answers, upload and verify
//! checks, and lock rules. Object ids, bytes, tokens and SQL stay with the caller.
//!
//! Rules:
//! * Uploads need write access and a repository that is not archived; a batch names at
//!   most [`MAX_BATCH_OBJECTS`] objects, for downloads too.
//! * A repository may use an object linked to it, or one linked to another repository
//!   of its fork network that the requester may read (which then links it here too).
//!   Knowing an object id never grants its content.
//! * An upload is skipped only when the object is linked with the same size and its
//!   bytes are on disk; otherwise it is refused above the size limit, or requested.
//! * Locks are listed by readers; creating, verifying and unlocking need write access
//!   and an account; owners unlock their own locks and administrators anyone's with
//!   `force`. Lock pages are ordered by id with at most [`MAX_LOCKS_PER_PAGE`] entries.

mod batch;
mod locks;

pub use batch::{
    BatchItem, BatchPort, LfsRefusal, MAX_BATCH_OBJECTS, ObjectDecision, Verification, admit_store, admit_transfer, decide_batch,
    upload_refusal, verify_stored,
};
pub use locks::{LockIdFilter, MAX_LOCKS_PER_PAGE, list_lock_id, locker, may_unlock, next_cursor, page_size, parse_cursor};
