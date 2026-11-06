# Reference contracts and applications

Collaboration owns Reference/RemoteRef/Found/Scan, lexical find/scan_plain and rendering
ReferenceSet/ReferencedIssue records. Core/service paths reexport the same definitions;
parsing byte ranges, canonical numbers, name validation, closing keywords, ordered
dedup and mention limits retain their existing implementations. The existing pure
repository-name model is reused through a defaults-disabled path dependency. No new
registry package or feature is introduced.

Core retains the existing pulldown Markdown options and event projection as an explicit
temporary bridge. It accumulates the feature-owned Scan with the same buffer flushing,
link/image/code exclusions, emphasis joining and raw-HTML behavior. Existing core
parser tests remain against these public paths. Moving that small adapter to a content
boundary is deferred until its feature graph is proven; this slice does not retire
Markdown parsing, reference suggestions, push enumeration or the collaboration group.

ReferenceResolver owns first-occurrence batched resolution and bounded names. Its
production port still constructs the same issue/meta stores after scanning and uses
one tokio::try_join! for numbers and account names. Remote repository references do
not resolve for rendering. Empty/code/link/remote-only text keeps the zero-query store
fast paths. No intermediate DTO Vec, additional scan or result copy is added.

CrossReferences owns local-target lookup, actor lookup, remote visibility, self/dedup
suppression and link/history effect ordering. It deliberately retains the existing
local full-row rereads, actor lookup after local reads and one remote repository-store
handle even when an active actor has no remote references. Same-repository qualified
references bypass extra grant reads; foreign targets use the unchanged viewer_grants
and repo_access adapter. Missing/inactive actors cannot create remote references.
Archived readable targets retain existing system/reference behavior; this is not a
new user write authorization API. Link insertion commits before history; failure or
cancellation can leave a unique link without a history event, and retry does not repair
it. No outbox, rollback or durability guarantee is introduced.

ReferenceService now owns only Database and ThreadServices, while view adapters borrow
them. Its constructor no longer clones/retains unused config, redb, repository path or
independent event-bus fields through ServiceContext. Existing needed database/thread
clones remain; no per-request owned clone or latency improvement is claimed. Exact
legacy component mappings include ReferenceService, ReferenceView and RemoteView.

Source-prepared acceptance includes independent lexical/scan contracts, first-order
batches, remote privacy, inactive/system actors, dedup/self filtering and the existing
link/history gap. Real-store fixtures retain original workflow ghost/privacy cases,
add unique races and trigger faults, and pin combined rendering at two acquisitions/
statements and empty scans at zero. A local issue event with a new cross reference
retains six acquisitions/seven statements; replay retains five/five. Shared-lane Rust,
strict Clippy, component mode and HTTP/reference/notification/Git tests remain pending.
Source/Node graph guards and read-only existing AST counts pass. There is no measured
runtime/compile-time performance claim from this parity port.
