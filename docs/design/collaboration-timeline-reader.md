# Bounded timeline reading and explicit thread composition

Collaboration owns cursor/EventId contracts, stored pages/source projections and
viewer-shaped pages. Event payloads stay concrete generic parameters; service and
storage paths are exact aliases instantiated with their existing `TimelineEvent`.
No event DTO conversion, dynamic registry, boxed future or extra row buffer exists.
Cursor URL syntax/order and raw pagination are unchanged. `key()` becomes a public
query-key accessor for authoritative storage. Generic page defaults impose no new
event `Default` bound. Unknown stored event kinds still count toward the raw page.

`TimelineReader` collects subject account/source IDs from ONLY the selected page,
using the same two HashSets, two ID Vecs and two result HashMaps. The production
port retains `tokio::try_join!` for account and readable-source SQL batches. There
is no intermediate projection Vec; existing IssueSource rows move directly into the
source map. Empty batches retain store fast paths. Permissions are snapshotted once
after subject reads, then applied per comment. Unreadable cross-reference events
are omitted while the original raw-page next cursor remains unchanged. Account and
source clones happen only when copying subjects into the same visible entry shapes.
Comment lookup short-circuits missing rows and skips author SQL for ghost authors.
No timeline caching or new query/permission authority is introduced.

`ThreadServices` replaces broad ServiceContext ownership with the exact Database,
RepositoryStorage, EventBus and ThrottleService dependencies. The old TimelineService
name is a compatibility alias; Services.timeline and HTTP signatures remain compatible.
Its reader()/writer() methods return narrow concrete borrows. Actual read and write
methods belong to those roles, not forwarding impl blocks on the compatibility name.
Internal row creation, history recording, system closure and pull-opening callers use
explicit roles. Removed crate-private forwarding methods have no external Rust API.
Public legacy methods delegate for staged consumer migration; final alias removal
requires callers to adopt the explicit composition name/roles, not more hidden methods.

Pull reopening still calls the identical validation sequence with explicit borrowed
Database/RepositoryStorage inputs. The old public ServiceContext-based helper remains
a delegating compatibility entry point. No authorization, clock, transaction, SQL,
conditional-winner publication, store lifetime guarantee or outbox claim changes.
IssueStore's implementation and tests after its public-type section remain byte-for-
byte unchanged after newline normalization.

ServiceContext derives Clone over several handles, not one shared Arc. The new owner
omits only the Config Arc and KvStore Arc clone/retention; Database, RepositoryStorage,
EventBus and throttle clone behavior remains. RepositoryStorage still clones its
PathBuf and shared locks. Reader/writer selection adds no request-time owned clone.
A prepared size_of test compares the actual old context+throttle layout with the new
owner; numeric layout and latency measurements await Rust execution. No runtime or
compile-time speedup is claimed from static dispatch or future delegation alone.

The existing exact AST checker was run read-only without Cargo: cumulative authored
ThreadServices is 79 lines, ThreadWriter 204 across seven files, ThreadReader 34 across
two files, feature TimelineReader 70 and TimelineReadPort 22. All collaboration types
pass its strict 400-line scan. The broader legacy services scan still reports other
oversized types/module-resolution issues; this is not a whole-crate completion claim.
Source physical bounds and Node feature guards also pass.

Prepared tests cover cursor/default/wire aliases, selected-page dedup, unreadable
reference omission with preserved continuation, missing/ghost comment lookup,
minimum/maximum page bounds and unpolled reads. Actual-store fixtures pin private
references, joined comment authors, page-scoped work (one page query plus zero/one
account and source batches), explicit roles and owner type identity/layout. Existing
concurrency remains in the production adapter. Rust tests have not run in this source
preparation; acceptance requires model/application collaboration matrices, complete
service timeline/issues/pulls/reference/notification tests, core/storage tests, web
HTTP page/cursor/privacy tests, strict all-target Clippy and current AST checks.
