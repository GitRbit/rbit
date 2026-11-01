# Native probe protocol and pure session validation

`rbit-plugin-api::worker` defines passive version-1 values. The independent
`rbit-plugin-native` crate provides a bounded in-memory codec and host-side session
machine. No server feature links this crate. There is no process, pipe, filesystem,
network, timer, host call or executable admission implementation in this layer.
Package execution remains unavailable.

## Wire and resource contract

Each frame is a four-byte big-endian unsigned body length followed by UTF-8 JSON.
The body must contain 1–65,536 bytes and at most 16 nested containers. SDK encoder
bytes define the canonical spelling: field order, whitespace, escapes and numeric
spelling must match reserialization exactly. Unknown or duplicate fields, including
nested package/limit fields, cannot be normalized into an accepted frame. This is
a strict protocol profile, not acceptance of every equivalent JSON representation.
Changing variants, fields or canonical encoding requires a versioned protocol change.

| Limit | Fixed ceiling / behavior |
| --- | --- |
| Frame body | 65,536 bytes; checked before reservation |
| JSON container depth | 16; checked before typed deserialization |
| Shared session frames | 4,096 across host and worker directions |
| Shared session wire bytes | 16 MiB, including four-byte prefixes in both directions |
| Outstanding request | One |
| Failure details | Fixed error codes; no foreign diagnostic text |

Validated `WorkerLimits` may lower the session ceilings, never raise them. Hello
advertises the actual selected limits; Ready must echo them exactly. Counters check
remaining capacity before body reservation or output growth. A budget too small to
finish a handshake or graceful stop fails closed. These are lifetime wire/frame
caps, not frames per second, elapsed deadlines or exact allocator accounting.

The decoder is sealed to HostMessage/WorkerMessage. One feed decodes at most one
frame and returns its consumed byte count; suffix bytes remain caller-owned. It
retains at most one capped raw body and uses bounded transient typed/canonical
encoding buffers. Returned frames and caller input buffers have separate ownership.
The host Session rejects any supplied suffix after a response rather than carrying
unsolicited bytes across a later request. All decoder errors and EOF are terminal.

## Identity and conversation

The exact identity contains package ID, full SemVer including build metadata,
archive digest, policy epoch and host fingerprint, review/selection revisions,
host instance, generation and launch nonce. These are comparison data. Constructing
or echoing them performs no authorization and attests no binary. The future host
coordinator must obtain current authoritative decisions and mint appropriate fresh
identities; callers cannot use this SDK value as a launch permit.

| Host request | Allowed phase | Required worker response | Next phase |
| --- | --- | --- | --- |
| Hello | New | Ready with protocol version 1, exact identity and exact limits | Ready |
| Ping | Ready | Pong with the exact outstanding bounded sequence | Ready |
| Stop | Ready | Stopped | Stopped, terminal |

Each request moves to a waiting phase before a response is supplied. Nonempty
inbound data in New/Ready is rejected, even a partial prefix or idle Error. Replayed,
duplicate, unexpected, wrong-identity, wrong-limit or wrong-sequence responses fail
terminally. Sequences advance under the existing bounded PolicyRevision type.
Worker Error during an outstanding request, transport failure, exhaustion and
unexpected/truncated EOF are terminal. Only an already Stopped session has graceful
EOF; no later message reopens it. There are no state-changing calls or automatic
replay after a missing response.

Generation is an exact integer on the wire; other implementations must not round
it through a floating-point JSON number. Existing revision/version/digest SDK
validation still applies. Handshake identity stays inline with a narrowly documented
large-enum lint allowance; no separate identity box is introduced. No allocation
or latency parity claim follows from that representation choice.

## Required transport and admission work

The future transport must bound its external buffers, outstanding returned frames
and work/rate admission; reject already buffered unsolicited bytes; bind actual
write/read order; and implement absolute handshake/read/write deadlines, partial
and stalled pipe cancellation, owned process cleanup and reaping. Calling fail()
only changes the pure machine. It closes no pipe and kills no process. OS arrival
ordering and blocked-I/O liveness cannot be proved by an incremental decoder.

Before any independent worker launch, retain all existing policy barriers: explicit
new execution-policy descriptor/re-review (v2 binds Unavailable), exact current
install/review/selection authorization, precommit revocation across policy writers,
verified same-object CAS materialization, finite owned durable attempts with restart
and uncertain-commit reconciliation, bounded host data/scratch, and audited safe
supported-platform process/transport backends. Unsupported platforms remain closed.
Native code sharing the service OS user has that user's authority. A nonce, broker
or process group is not a hostile-code sandbox. Wasm/data execution is not added.

## Validation scope

The ten native tests exercise every split and truncation, hostile prefixes before
allocation, canonical/nested-field rejection, UTF-8/depth limits, full identity and
limit echoes, shared budgets, sequence/EOF/terminal states and split/coalesced
unsolicited replay. All 38 existing API tests and ten native tests pass locally,
alongside strict all-target lint and six actual dependency-profile guards. Fifteen
UI dependency-guard tests reject transitive native backedges. Native is included in
the isolated CI contract job as well as normal workspace coverage. Hosted exact-head
validation remains required. No OS subprocess, pipe, deadline, cleanup, sandbox,
performance or executable-availability result is established by these pure tests.

Separate [controlled Unix process tests](native-process-feasibility.md) exercise
fixed helpers and actual pipes. Their platform-specific evidence and ownership
assumptions do not change the pure production codec or executable availability.
