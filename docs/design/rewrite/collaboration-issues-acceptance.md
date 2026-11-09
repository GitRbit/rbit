# Timeline and issue retirement acceptance

Acceptance evidence for the `collaboration.issues` ledger row ("Issues, labels,
milestones, assignees, search and filtering"), and progress for `collaboration.timeline`
("Shared timeline, comments, edits, mentions, references and closing keywords"), which
stays `baseline-retained`, following the
[identity retirement](identity-retirement.md) pattern. Earlier slices already moved most
of both groups behind collaboration ports; this record covers what remained and lists
the whole group's owners. The [retirement gaps](../collaboration-reference-retirement-gaps.md)
document the earlier state.

## Scope

| Behaviour | Owner after this slice |
| --- | --- |
| Thread and comment permission tables (`IssuePermissions`, `issue_permissions`, `CommentPermissions`, `comment_permissions`) | `rbit_collaboration::threads` (moved from `rbit_core::issue`, which re-exports them) |
| Nothing is offered on an archived repository's threads | `rbit_collaboration::threads::thread_permissions` (new) |
| Open/count threads, text edits, state changes, lock/unlock, system closure, human and task comments, timeline reads | `rbit_collaboration::threads` and `comments`/`timeline` applications (already production) |
| References, cross-references, commit references, closing keywords, merged-pull closures, suggestions | `rbit_collaboration::references` (already production) |
| Which pushed commits are new (previous tip, or every other tip once) | `rbit_collaboration::references::push_exclusions` (new) |
| Users by name (organisations are not users), labels by id, assignees (active and able to push), the assignable list, a comment's thread | `rbit_collaboration::issues::{IssueParticipants, users_named}` (new) over `ParticipantPort`/`AccountNamePort` |
| What the issue event subscriber indexes and references: earlier texts, current thread and comment bodies, no references from rbit Actions comments, submitted reviews only | `rbit_collaboration::issues::ThreadActivity` (new) over `ThreadActivityPort` |
| Issue creation, deletion, label/milestone catalogs and attachments, assignees, search/list/dashboard listing, metadata hydration, query language | `rbit_collaboration::issues`, `labels`, `milestones`, `assignees` applications (already production) |
| Dashboard views and mention index | `collaboration.dashboards` ([acceptance](collaboration-dashboards-acceptance.md)) |

`rbit-services` keeps the composition: `ThreadServices`/`IssueService` build access from
the request, supply throttles and clocks, map errors, record audit entries and publish
events. `issues/ports.rs` adapts the stores, other people's repository grants, the
mention index and the cross-reference recorder to the new ports. The issue event
subscriber's `handle_event` only routes `DomainEvent`s to `ThreadActivity`, the push
scanner and `MergeIssueClosures`. The push scanner keeps the Git walk (mirror skip,
repository lookup, ref snapshot, commit limit) and hands each update's commits to
`CommitReferences`. Webhook payloads (`issues/hooks.rs`) and REST JSON (`issues/json.rs`)
are presentation adapters. HTTP routes, templates and catalog text are unchanged.

## Legacy code removed

1. The permission tables in `rbit-core/src/issue.rs` and the archived rule in the
   timeline adapter.
2. `IssueService::users_named`, `repo_labels`, `validated_assignees`, `assignable`,
   `comment_thread` and `access_of`: their checks, loops and refusal text (they now
   delegate).
3. The subscriber's reads and choices in `handle_event`: thread and comment lookups,
   the pending-review filter, the review comment filter, the Actions-comment rule and
   the direct mention and cross-reference calls.
4. The push scanner's inline exclusion computation.

## Boundaries that fail if legacy paths return

`scripts/rewrite/issues-retirement.mjs`, run by `scripts/rewrite/check.mjs`, over every
authored production Rust file below `crates/*/src`, comments ignored:

- A permission table definition outside `rbit-collaboration/src`.
- `IssuePermissions::default()` in `rbit-services/src/timeline*` (the archived rule).
- In `rbit-services/src/issues*`, except the port adapters `issues/ports.rs` and
  `issues/reader.rs`: account/label/user lookups, `viewer_grants`, organisation
  filtering, assignable or comment-thread store calls, `ReviewState::Pending`, review
  store access, direct mention or cross-reference recording, or an inline ref-tip
  filter.
- The participant refusal text outside `rbit-collaboration/src`.

`scripts/tests/issues-retirement.test.mjs` covers each rule, the allowed adapter, web
view and test cases, and the whole current tree.

## Results

RESULTS_PLACEHOLDER

The existing suites ran unchanged: `rbit-services` `issues` (with `issues/*_port`
fixtures), `timeline` (with `timeline/*_port`), `references`, `pulls` (merge closures),
`access`; `rbit-storage` `issue_meta`, `issue_search`, `issues`; `rbit-web` views and
routes; `rbit-e2e` `issues`; and the `rbit-core` permission matrix tests, which now
exercise the re-exported tables. New: `rbit-collaboration/tests/issue_activity.rs`
(refusal text and stop-at-first order, assignable gate and trim, subscriber call order,
Actions comments, pending reviews, deleted threads) and `push_exclusions` unit tests.

## Parity argument

- Permission tables are moved byte for byte; the archived rule is the same condition.
- Participants make the same calls in the same order: names in one query; labels and
  users one lookup per id, stopping at the first refusal; each assignee's grants are
  read only after the user is found. Store handles are now taken per call instead of
  once per loop (pool handle clones, no extra query).
- The subscriber keeps both match passes and their order: earlier texts first, then the
  current body. Comment events still read the thread and then the comment before
  deciding; review events still read the review, skip pending ones, then index the
  previous body, the body, and (only then) read the review's comments. A comment body
  is copied once into a `String` for the port.
- Push exclusions are the same expression, moved: the previous tip, or all other tips
  skipping one occurrence of the new tip.

## Migration, rollback and performance

No migration, schema, SQL or stored data changed; migration 0073 remains the latest and
no historical migration was edited. Rollback is the previous binary against the same
data. No request path gains a query; the applications are statically dispatched over
borrowed adapters. No release measurement was taken and none is claimed.

## Not covered

- The Markdown event projection (`rbit_core::references::{markdown_options,
  scan_markdown}`, about 50 lines) stays in `rbit-core`. It turns pulldown events into
  the collaboration-owned `Scan`; mention, reference and closing-keyword decisions are
  `rbit_collaboration::references`'. The [retirement gaps](../collaboration-reference-retirement-gaps.md)
  record why it is not moved before a shared safe-content boundary exists. Until it
  moves, `collaboration.timeline` stays `baseline-retained`; every other timeline
  decision listed above runs in `rbit-collaboration`.
- `ThreadWriter::record` remains a direct timeline-event write used by other
  workstreams (labels, pushes); it holds no decision.
- Pull request lifecycle, reviews and review threads keep their own rows.

## Cost

COST_PLACEHOLDER
