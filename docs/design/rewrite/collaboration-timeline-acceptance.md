# Collaboration timeline acceptance

Acceptance evidence for the `collaboration.timeline` ledger row ("Shared timeline,
comments, edits, mentions, references and closing keywords"). The
[timeline and issue record](collaboration-issues-acceptance.md) moved every timeline
decision into `rbit-collaboration` except one, so the row stayed `baseline-retained`. This
slice moves that last piece, the Markdown event bridge, following the
[identity retirement](identity-retirement.md) pattern.

## Scope

| Behaviour | Owner |
| --- | --- |
| Thread and comment permissions, the archived rule, comments, edits, state, timeline reads | `rbit_collaboration::threads`, `comments`, `timeline` (earlier slices) |
| Reference lexing, closing keywords, mentions, cross-references, push exclusions, merge closures | `rbit_collaboration::references` (earlier slices) |
| **Which Markdown text is scanned**: the options, joined text runs, skipped code spans, code blocks, link and image text, counted raw-HTML text | **`rbit_collaboration::references::{markdown_options, scan_markdown}`** (`references/markdown.rs`, moved from `rbit_core::references`) |

## Where it lives, and why

`rbit-presentation` would be a natural home for Markdown parsing, but it isn't in this
base (`88d16e99`). More importantly, the scan's result is collaboration's `Scan`, and the
architecture policy gives `rbit-presentation` only `rbit-contracts` and
`rbit-localization`. A presentation-side visitor would need a third crate to compose it
into a `Scan`, plus a new edge into `rbit-services`.

Collaboration owns `Scan` and every decision made on it, so the bridge sits next to them:

- **Feature gating:** it is compiled only with the existing `applications` feature, which
  now enables an optional `pulldown-cmark` (the same workspace version and features core
  used). The default model graph stays engine-free, as the
  [gaps record](../collaboration-reference-retirement-gaps.md#markdown-content-boundary-resolved)
  required.
- **Core's public API:** the earlier objection, that core would have to turn on
  application capabilities to keep its public `scan_markdown`, is avoided by removing that
  API. Every caller is in `rbit-services`, which already enables `applications`.
- **Edges:** no internal crate edge is added. `rbit-core` drops pulldown-cmark.

## Legacy code removed

- `rbit_core::references::{markdown_options, scan_markdown}` and core's pulldown-cmark
  dependency. Its Markdown scan test moved, unchanged, to the new module. Core keeps the
  re-exports of the lexer and scan types (`find`, `scan_plain`, `Scan`, …) and their tests.
- The five calls in `rbit-services` (`issues.rs` mentions and merge closures,
  `references.rs` resolution and cross-references, `notifications/fanout.rs` mentions) now
  name `rbit_collaboration::references::scan_markdown`.

## Boundary that fails if legacy paths return

`scripts/rewrite/collaboration-timeline-retirement.mjs`, run by `scripts/rewrite/check.mjs`
(comments stripped), rejects:

- a `scan_markdown` or `markdown_options` function outside `rbit-collaboration`;
- a `rbit_core::…references::scan_markdown`/`markdown_options` path;
- any `pulldown_cmark` use in `rbit-core`;
- a missing collaboration scanner, or a scanner whose `Options::ENABLE_*` set differs from
  safe rendering's (`rbit-presentation/src/markdown.rs` when present, else
  `rbit-web/src/render/markdown.rs`). Both currently name TABLES, FOOTNOTES, STRIKETHROUGH,
  TASKLISTS, HEADING_ATTRIBUTES and GFM.

`scripts/rewrite/collaboration-features.mjs` now also rejects pulldown-cmark in the
collaboration model graph (no features), while allowing it with `applications`.

Run against the legacy files at `88d16e99`, the new checker reports 5 violations; against
this tree, none. `scripts/tests/collaboration-timeline-retirement.test.mjs` and a new case
in `collaboration-features.test.mjs` pin the rules, the allowed cases and the tree.

## Gates

| Gate | Evidence |
| --- | --- |
| Byte-identical detection | The function bodies are unchanged: the same options, the same event match (buffer joining, break handling, emphasis/strong/strikethrough pass-through, the hidden counter for links, images and code blocks, flushing on every other event, the final flush), the same `Scan::add_text` calls in the same order. Same pulldown-cmark version and features (workspace dependency). The existing `markdown_scans_skip_code_and_link_text` test runs unchanged in its new module. The service suites for references, timeline, notifications and issues exercise the scan through the same calls. |
| Privacy and authorization | Unchanged: the scan only lists candidates. Resolution, cross-reference recording and mention fan-out keep their access checks in `rbit_collaboration::references` and `rbit-notifications`. |
| Feature graph | The model graph (no features) has no Markdown engine (new graph rule); `rbit-core` and `rbit-notifications` still take collaboration without features (existing rule). |
| Migration and rollback | No schema, migration or stored data. Rollback is the previous binary. |
| Performance | Same code, statically dispatched, still passed as a `fn` pointer where it was before. Compile graph: pulldown-cmark moves from `rbit-core` to `rbit-collaboration[applications]`; it was already in the build. No measurement claimed. |

## Results

Linux quick check run 234 on the slice's final code passed all five jobs: rewrite contracts
(including the collaboration feature-graph rules and this boundary check), workspace-wide
format and Clippy with `-D warnings` on all targets, cargo-deny, the front-end build, and
nextest for the changed packages (`rbit-collaboration`, `rbit-core`, `rbit-services`):
1,011 tests passed. Locally, `node scripts/rewrite/check.mjs` reports 0 violations, and the
new and updated Node tests pass (8 of 8). The full suite runs with the integrated batch. No
Windows build or test was run for this slice.

## Not covered

- Safe rendering keeps its own option set. The two copies are pinned equal by the rewrite
  check rather than shared. A shared contract would need the scan and the renderer in one
  crate graph, which the presentation edges don't allow today.
- `ThreadWriter::record` remains a direct timeline-event write used by other workstreams
  (labels, pushes); it holds no decision, as the
  [timeline and issue record](collaboration-issues-acceptance.md) notes.
- Pull requests, reviews and review threads keep their own rows.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 2 (move, boundary check) plus this record |
| Files | 9 code files, 5 checker files |
| Lines | about +200 / −65 across both, mostly moved code and the checker |

The work was mostly deciding where the bridge belongs. Once core's public path was dropped
rather than preserved, the move needed no new edge.
