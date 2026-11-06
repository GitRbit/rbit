# Filtered thread read ownership

Prepared source, not Rust acceptance or a runtime performance result. This slice
activates collaboration-owned repository search, API list and personal dashboard
applications through the existing public methods. Broad collaboration groups remain
baseline-retained. Deletion, mentions/background ownership and pull/review workflows
are separate obligations.

## Contracts and static composition

`issues::query` owns the original pure language, parser, formatting and modifiers.
Core reexports the exact types and constants. Collaboration also owns SearchFilter,
DashboardRelation/Scope, IssueMetadata, MetadataNeeds, OneIssueMetadata and
IssueSearchResults; storage/service public paths reexport the same concrete types.
No serde, field, ordering or arbitrary public page_count behavior is tightened.

IssueListing owns gate ordering, ordered name resolution, filter selection, pagination,
selected-page IDs/author deduplication and final projections. IssueMetadataReader owns
single/batch/labels projection. Concrete IssueSearchPort and IssueMetadataPort adapters
invoke unchanged SQL and decoders. Borrowed IssueReader retains only &Database, with
an explicit qualified component policy entry. Legacy IssueService methods delegate;
callers may borrow reader() without retaining a ServiceContext in the read component.
The remaining IssueService still owns context for its unported responsibilities.

There are no new registry dependencies, manifest features, tasks, dynamic dispatch,
per-row queries or DTO row buffers. Existing issue/author ID vectors and BTreeSet,
result maps, and raw metadata vectors retain their original allocation structure.
Storage handles are constructed once at their original operation-specific points.
The HTML concurrency adapter still uses tokio::try_join!, preserving first-poll order.
This is source/work parity, not a latency or compilation improvement claim.

## Distinct read paths retained

* Repository visibility is checked before hints, account lookup or store construction.
  Unknown names still become AccountId(0); signed-out @me still adds the English hint.
  Author resolution precedes assignee resolution and its error wins.
* HTML search counts first, clamps the requested page, selects rows, then joins author
  hydration with precisely the requested metadata. Labels-only never reads assignees
  or milestones. All metadata for exactly one ID takes one joined statement; other
  lengths use the three original concurrent batch queries, including their empty guards.
* The API replaces both caller-supplied repository and dashboard scope, normalizes
  page/per_page to at least one, and does not clamp an out-of-range page. Author lookup
  completes before metadata construction/read, retaining its error precedence.
* Dashboard reloads the active user before resolving filters, uses that fresh ID/admin
  state in count/row SQL, and batches readable repositories after page hydration.
  It retains the original temporary repository-ID vector and first-order deduplication.
* Count and row queries remain separate statements, not a new snapshot transaction.
  A deterministic real-SQL fixture changes public visibility to private after counts:
  the total may remain stale, while row SQL filters the now-unreadable issue out.
  No new consistency or authorization capability is inferred from the count result.

## Prepared acceptance

Independent policy fixtures observe lazy construction, denied and unpolled fast paths,
qualifier order/errors, HTML clamping, API scope reset/unclamped paging, fresh dashboard
admin state and metadata mode selection. The original query parser's five unit cases
move with its unchanged production source. Compatibility tests assert type identity.

Actual-store work assertions use the existing task-scoped counter. With two rows by
one author, All uses 6 acquisitions/statements; Labels uses 4. With one row All uses 4.
Empty selected pages use 2, empty metadata uses 0, and multi-ID standalone metadata
uses 3. Denied reads use 0; a suspended dashboard viewer uses 1 before any qualifier
lookup. Stale-admin/private and count/row visibility-change fixtures test actual SQL.

Read-only source comparison against pinned main 39f932a confirms issue_search SQL,
row decoding and its original tests are unchanged (only model definitions move).
Metadata SQL/decoding is unchanged against the immediately preceding checkpoint;
query language production source is unchanged. Source/graph policy and exact AST
scans pass; IssueListing is 191 cumulative lines, IssueMetadataReader 41,
IssueQuery 179, and IssueReader 83. Fourteen focused Node policy/graph cases pass.

Rust tests and strict Clippy are intentionally pending the coordinated compiler lane:

```text
cargo test -p rbit-collaboration --features applications --lib issues::query
cargo test -p rbit-collaboration --features applications --test issue_listing
cargo test -p rbit-services --test main issues::listing_port
cargo test -p rbit-services --test main issues::personal_dashboards_filter_and_revalidate_access
cargo test -p rbit-storage --lib issue_search
cargo clippy -p rbit-collaboration -p rbit-services -p rbit-storage --all-targets -- -D warnings
```

Run the whole issue/reference/timeline and relevant HTTP suites before acceptance;
new fixtures alone do not retire the collaboration feature group.
