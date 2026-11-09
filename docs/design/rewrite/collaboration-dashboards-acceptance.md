# Personal dashboards retirement acceptance

Acceptance evidence for the `collaboration.dashboards` ledger row ("Personal Issues/Pull
Requests dashboards and filters"), following the
[identity retirement](identity-retirement.md) pattern. User-facing behaviour is
described in [personal dashboards](../../personal-dashboards.md).

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Cross-repository listing: active-viewer check, kind and relationship scope, readable repositories, counts, page clamp, hydration | `rbit_collaboration::issues::IssueListing::dashboard` (already production before this slice) |
| Which views a page offers (review requests only for pull requests), tab order, absent view = created, unknown view refused | `DashboardRelation::{views, from_view}` in `rbit-collaboration/src/issues/dashboard.rs` (new) |
| Blank or absent search = `is:open`, trimming | `rbit_collaboration::issues::dashboard_query` (new) |
| Mention recording: scanned names, people only (organisations ignored), idempotent insert | `rbit_collaboration::issues::DashboardMentions::record` (new) |
| One-time backfill of existing bodies: completion marker, four sources in bounded batches, restart-safe | `DashboardMentions::index_existing` (new) |
| Mention persistence | `DashboardMentionPort` (new), implemented for `rbit_storage::Database` in `rbit-storage/src/collaboration_mentions.rs` (new); the SQL and migration 0043 are unchanged |
| Relationship SQL and visibility predicates | `rbit-storage` `issue_search` (unchanged) |

`rbit-web/src/routes/dashboards.rs` keeps HTTP parsing, the "unknown dashboard view"
400 response, localized tab labels, URLs and rendering. `rbit-services::IssueService`
supplies the Markdown scanner (`rbit_core::references::scan_markdown`) and delegates
`record_mentions`/`index_existing_mentions` to the application.

## Legacy code removed

1. The route's `view` match (including the pull-request-only review tab), the tab
   filter and the `is:open` default.
2. `IssueService::record_mentions`' account lookup, organisation filter and direct
   `issue_search().record_mentions` write.
3. `IssueService::index_existing_mentions`' marker check, source/batch loop and
   completion write.

## Boundaries that fail if legacy paths return

`scripts/rewrite/dashboards-retirement.mjs`, run by `scripts/rewrite/check.mjs`, over every
authored production Rust file below `crates/*/src`, comments ignored:

- In `rbit-web/src/routes/dashboards.rs`: the view literals (`"created"`, `"assigned"`,
  `"mentioned"`, `"review-requested"`), the `"is:open"` default, or a
  `!= DashboardRelation::ReviewRequested` tab filter.
- A mention persistence call (`issue_search().record_mentions`, `mention_sources`,
  `mentions_indexed`, `finish_mentions_index`) outside `rbit-storage` and
  `rbit-collaboration/src`.

`scripts/tests/dashboards-retirement.test.mjs` covers each rule, the allowed
label/storage/test/comment cases and the whole current tree.

## Results

RESULTS_PLACEHOLDER

The existing suites ran unchanged: `rbit-services` `issues` (dashboard listing,
mentions and the upgrade backfill marker), `issues/listing_port`, `issues/listing_race`,
`access` (lost access removes threads), `rbit-storage` `issue_search` and `upgrade`,
`rbit-web` route and localization tests, and `rbit-e2e` `issues`. New: unit tests for
view selection and the default search in `issues/dashboard.rs`, and
`rbit-collaboration/tests/dashboard_mentions.rs` (people only, no store work without
mentions, batch order across all four sources, completed index not rescanned).

## Parity argument

- `from_view(None)` is `Created`, exactly the route's `unwrap_or("created")`; matching
  is exact and case-sensitive; `review-requested` on the issues page is `None`, which
  the route still turns into the same 400 text. Tabs are produced in the same order
  with the same labels and URLs.
- `dashboard_query` trims, treats blank as absent and returns `is:open` otherwise, the
  same expression the route used.
- Mention recording makes the same calls in the same order: no store work when the
  scan finds no names; otherwise one `accounts_named` query, then one idempotent
  insert of the people among them. The backfill walks sources 0–3, each in batches
  of 100 after the last row id, then sets the completion marker; an indexed database
  only reads the marker.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed; migration 0073 remains the latest and
no historical migration was edited. Rollback is the previous binary against the same
data. No request path gains a query or a store handle; the mention application borrows
the database. No release measurement was taken and none is claimed.

## Not covered

- Which events feed the mention index (opened/edited bodies, comments, the previous body
  of an edit, deleted comment snapshots, submitted reviews and their comments) is chosen
  by the shared issue event subscriber in `rbit-services/src/issues.rs`, which also routes
  cross-references and commit references. That dispatch belongs with the subscriber's
  owners (`collaboration.issues`, `collaboration.timeline`); each branch now calls the
  application.
- Event-bus overflow can still skip activity, as documented for users.

## Cost

COST_PLACEHOLDER
