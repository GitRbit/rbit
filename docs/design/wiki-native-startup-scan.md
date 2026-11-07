# Native Wiki startup scan

This W2 component validates real loose files, not a current/durable head. It is
inactive until the SQL scratch/edge/agreement and platform protocols are closed.
It neither accepts user commands nor updates refs or SQL visible pointers.

An actual catalog-bound object owner starts a scan for one repository/WikiId. Its
opaque, noncloneable continuation retains the real catalog/root owner and per-wiki
gate, plus the actual directory iterators. It cannot be recreated from serialized
OID/filename cursors. Normal preparation for the same wiki is Busy until the scan
is consumed or destroyed. A dropped awaiter leaves its queued/running blocking
worker holding the real guards; restarting a lost scan requires a new scratch pass.

Each call has an explicit 1..256 step/record budget and 1..16MiB decoded-byte budget.
Directory advancement, object starts and edge emission each consume one step.
One maximum tree is reserved before decoding another file. Empty progress batches
are valid; they do not mean end-of-scan. At most one raw 1MiB tree and one bounded
record vector are retained across calls. A loose read temporarily holds at most
2MiB compressed input and 1MiB+64 decoded backing, independently of history size.
Limits are format/resource bounds, not real-time deadline or allocator measurements.

Each lowercase SHA-1 loose filename is visited once. The decoder requires an actual
complete zlib stream/checksum, exact EOF, canonical kind/decimal-size header, exact
declared payload size and recomputed Git OID agreement. Blobs are bounded UTF-8
Markdown without NUL. Flat tree entries are emitted incrementally with strict
plain-blob mode, canonical positive page-ID filenames and unique byte ordering;
there is no second decoded tree vector. Commits have at most one parent, normal
Wiki message/signatures, no encoding or extra headers, and carry actor/time facts
for later exact SQL agreement. Legacy account labels are distinguished from
incarnation-bound labels, never silently upgraded to the latter.

Object facts precede their ordered edges. Complete is emitted only after all
semantic checks for that object. A partial/failed scan may leave incomplete scratch
rows, never a capability. The native directory cursor cannot establish that a
caller committed the preceding records: a private closed coordinator must persist
them before advancing and bind that ordering to the actual owner/epoch. Public
record enums remain data, not trusted head receipts or authentication evidence.

Unknown names, alternates, nonempty info/pack directories, foreign object kinds,
invalid names/objects and oversized formats refuse this intermediate scanner.
Pack/restore/backup support remains a required later compatibility gate; refusal
is not a claim that a packed Wiki is corrupt. Native-only orphan namespaces still
need root enumeration rather than silently following SQL alone. The deployment
assumption remains cooperating writers and static topology outside held resources,
not hostile same-user filesystem replacement protection.

Prepared fixtures exercise real three-commit shared history with seven unique
objects, one-record resumptions, strict owner/gate lifetime and unknown layouts,
plus malformed zlib/header/hash/size/tree cases. They are Rust/native UNRUN. Root
acceptance includes `-p rbit-git --test integration wiki_startup` and the unit module
`wikis::managed_writer::startup`, strict Clippy/AST and supported platform CI.

The next scratch pass stores unique verified objects and ordered edges under explicit
scope/object/edge/logical-byte ceilings. Indexed edge-kind joins, historical SQL
parent/generation/page agreement, exact accepted selected-blob bytes, platform
synchronization and actual current-ref agreement must all finish before any private
head capability exists. Shared blobs must not be rehashed once per historical page
or revision. None of those closed-bundle guarantees is inferred from scan completion.
