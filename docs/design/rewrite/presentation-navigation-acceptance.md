# Presentation navigation acceptance

Acceptance evidence for the `presentation.navigation` ledger row ("HTMX/history, preloader
limits, no-JS and safe fragment caching"). The crate and its new edge are explained in the
[repository documents record](repositories-documents-acceptance.md#why-a-new-crate).

## Scope

| Decision | Legacy place | Now |
| --- | --- | --- |
| Layout markers around `<title>`, `<main>` and the repository tabs | `rbit-web/src/partial.rs` constants | `rbit_presentation::navigation`, re-exported from `partial.rs` (templates and `updates.rs` unchanged) |
| The partial cut: marker order, title/tabs from the start and `<main>`'s end from the end, same-repository scope check, the `hx-swap-oob` insertion | `partial::slice`, `between`, `find`, `rfind`, `trim` | `navigation::slice` |
| Header name and its `Vary` spelling; appending to an existing `Vary` | `PARTIAL_HEADER`, `VARY_NAME`, `append_vary` | `navigation::{PARTIAL_HEADER_NAME, VARY_NAME, vary_with_partial}` |

`rbit-web/src/partial.rs` stays the HTTP adapter: which requests may be partial (`GET`/`HEAD`
with `HX-Request`), immutable fragments left alone, only `200` HTML cut, body collection,
`Content-Length` removal and the echoed header. `middleware::html_caching` computes ETags
over what is sent, unchanged. HTMX history, preloader limits and the no-JS fallbacks are
browser assets (`nav.js`, preload) and templates, which the plan leaves in `rbit-web`;
none changed.

## Legacy code removed

`partial::{slice, between, find, rfind, trim, VARY_NAME, SUBNAV_OPEN, SUBNAV_ID}` and the
marker constant definitions; `append_vary` now only converts headers.

## Boundary that fails if legacy paths return

In `scripts/rewrite/presentation-retirement.mjs`: a constant or static in `rbit-web`
initialised with a `<!--partial:` / `<!--/partial:` marker, and a `slice`/`between`
function defined in `rbit-web/src/partial.rs`. Marker strings in test assertions are
allowed. Against the legacy `partial.rs` it reports both rules.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Code moved verbatim (`trim` was `trim_ascii`). The four cut tests moved unchanged to `rbit-presentation/src/navigation/tests.rs`; the layout-marker test and both HTTP tests (own ETag per shape, only successful same-repository htmx reads are partial, pinned fragments vary on neither) stay in `rbit-web` unchanged except for naming `navigation::SUBNAV_OPEN`. New: `vary_with_partial` cases. |
| Security | A partial response is still a subset of the full page plus one attribute; immutable responses are never cut; the scope must match the tabs' `data-repo-scope` exactly. |
| Caching | Same `Vary` value and order (`…, Rbit-Partial`), so cache keys and validators are unchanged. |
| Migration and rollback | None; rollback is the previous binary. |
| Performance | Identical work; one `Option<&str>` passes the existing `Vary`. |

## Results

Verified together with the other rows; see the
[themes and installable app record](presentation-themes-pwa-acceptance.md#results).

## Not covered

- The HTMX history handling, preloader limits and no-JS fallbacks live in browser scripts
  (`nav.js`, preload) and templates, which stay in `rbit-web`. The [rewrite plan](../ground-up-rewrite-plan.md#proposed-crate-responsibilities) makes
  `rbit-web` the owner of "browser assets" and gives `rbit-presentation` "shared Askama
  components"; moving the templates is a template-wide change with its CSP hashes, not
  part of this row.
- HTTP fragment caching (`middleware::html_caching`, ETags, immutable pins) stays in
  `rbit-web` as response negotiation.

## Cost

Shared; see [themes and installable app](presentation-themes-pwa-acceptance.md#cost). This
row's share: `navigation.rs` 101 lines and its 62-line test file, and `partial.rs` shrinking
from 304 to 194 lines.
