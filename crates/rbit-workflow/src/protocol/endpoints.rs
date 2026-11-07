//! Protocol endpoints contracts and original validation.
use super::limits::{KIB, MIB};

/// The endpoints of protocol v1, apart from artifacts (see [`crate::protocol::ARTIFACTS_PATH`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Endpoint {
    /// `POST register`: [`crate::protocol::RegisterRequest`] → [`crate::protocol::RegisterResponse`].
    Register,
    /// `POST declare`: [`crate::protocol::DeclareRequest`] → [`crate::protocol::DeclareResponse`].
    Declare,
    /// `POST poll`: [`crate::protocol::PollRequest`] → [`crate::protocol::PollResponse`], or `204` when the poll ends empty.
    Poll,
    /// `POST tasks/{id}/state`: [`crate::protocol::TaskStateRequest`] → [`crate::protocol::TaskStateResponse`].
    TaskState,
    /// `POST tasks/{id}/logs?from=<line>` ([`crate::protocol::LogsQuery`]): `text/plain` lines →
    /// [`crate::protocol::LogsResponse`], or `409` with [`crate::protocol::ErrorBody::next_line`] on a gap.
    TaskLogs,
    /// `POST unregister`: [`crate::protocol::UnregisterRequest`] → `204`.
    Unregister,
}

impl Endpoint {
    pub const ALL: [Self; 6] = [Self::Register, Self::Declare, Self::Poll, Self::TaskState, Self::TaskLogs, Self::Unregister];

    /// The route below [`crate::protocol::BASE_PATH`], with `{id}` for the task id.
    pub const fn path(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Declare => "declare",
            Self::Poll => "poll",
            Self::TaskState => "tasks/{id}/state",
            Self::TaskLogs => "tasks/{id}/logs",
            Self::Unregister => "unregister",
        }
    }

    /// The largest request body, after decompression.
    pub const fn body_cap(self) -> usize {
        match self {
            Self::Register | Self::Declare => 64 * KIB,
            Self::Poll => 4 * KIB,
            // Outputs (up to `MAX_OUTPUT_BYTES` before JSON escapes them), step states and
            // summaries (`MAX_TASK_SUMMARY_BYTES`), with room to spare: a final call that is
            // too large could never report how its task ended.
            Self::TaskState => 4 * MIB,
            Self::TaskLogs => 8 * MIB,
            Self::Unregister => KIB,
        }
    }
}
