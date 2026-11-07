# Actions overview presentation slice

The production overview now uses the prototype's information hierarchy with real
existing data: workflow navigation, a clear selected/all-runs heading, visible
native filter labels and readable responsive run rows. The page count describes
only the rows on this page, not a fabricated total or aggregate success metric.
No dependency graph, log-search endpoint, scheduler state or backend feature is
introduced. Existing theme tokens and Tailwind utilities supply all styling.

## Responsibility and behavior

`views/actions/overview.rs` owns existing rows, workflow links, filter choices and
polling policy; `views/actions/dispatch.rs` owns native dispatch input models.
`overview_text.rs` prepares typed compiled presentation labels. The used overview
GET adapters in `routes/actions/runs/overview_page.rs` negotiate locale and wrap
existing document responses without adding service calls. The initial extraction's
workflow/control/result includes expand exactly to the old template source.

Native filters retain branch/actor/event/status names, values, action and GET
semantics. Pagination retains the complete selected filter query and existing
before cursor. Dispatch reference selection remains GET, dispatch and workflow
switching remain POST with their existing fields, redirects and permission checks.
Submitted invalid dispatch inputs are rendered with the same negotiated document
locale and 422 status. Successful dispatch redirects remain unchanged.

Only `#runs` refreshes. Filter, dispatch and toggle forms remain outside that
region, so typing or choosing inputs cannot be erased by list polling. Existing
three-second active, ten-second newest-idle and older-page polling decisions are
unchanged. No additional JavaScript or polling listener is introduced.

Overview labels and empty states use compiled English and Spanish catalogs. The
production account preference/browser negotiation is active; expanded/RTL pseudo
catalogs remain trusted diagnostic opt-ins for selected feature routes. Machine query values are never translated. Full documents and the
selected `#runs` root carry the negotiated language/direction; Content-Language and
Vary retain locale/cache dimensions alongside existing privacy and CSP middleware.
Dynamic names, titles, reference text and errors remain escaped. Successful dispatch
redirects and machine/protocol responses retain their existing contracts. Generic
HTML errors use the shared shell/catalog pipeline; diagnostic requests fall back to
the real document locale for those errors rather than enabling pseudo error text.
Logical filter padding and isolated metadata support RTL. Existing event/status option labels,
relative times and dispatch explanatory text retain their English representations;
these remaining labels are not covered by the compiled Spanish presentation catalog.
This is partial translation coverage, not a claim that all Actions text is translated.

## Acceptance

Real-router fixtures cover 26-run pagination/filter preservation, native form
isolation from the poll target, actual dispatch/toggle permission boundaries,
private-repository 404 behavior, and full/HTMX pseudo-locale roots with CSP/private
cache headers and unchanged pooled SQL work across locale representations. Existing
Actions filter, workflow dispatch, disabled-workflow and control parity tests remain
gates. Typed message tests cover the page-count meaning and English plural rules.

The original preparation was source-only; publication of this implementation is not
by itself an acceptance record for every subsequent revision. Selected integration,
localization and strict Clippy results must be tied to their actual source revision.
Fresh matching-revision CSS and isolated visual/keyboard/no-JavaScript review at
360px, light/dark and RTL remain required. No screenshot acceptance or rendering
speed improvement is claimed by this presentation-only slice.


## Current coverage record

This contract was checked against public `fb9f21dd`. Account locale, full/selected-root
language attributes and compiled Spanish presentation are current behavior. See
[account activation](account-locale-activation.md) for preference precedence and
[the localization contract](localization-contracts.md) for the exact raw/error and
queued theme-picker/Explore boundaries. Historical prototype images and earlier
footer samples establish only their recorded revisions, not current production
visual, accessibility or speed acceptance.
