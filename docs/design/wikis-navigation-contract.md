# Wiki navigation and presentation contract

W0 contract for the planned `rbit-wikis` web surface; see the
[repository wiki plan](repository-wikis-plan.md). Status: design only. No route,
template, catalog entry, header field or HTMX behavior below is implemented. It binds
the plan's "pages and a platform-native interface" requirements to the existing web
layer so W1-W3 slices add routes without inventing a second navigation model, skin,
renderer or cache policy. Canonical page-path normalization belongs to the domain path
policy; ID formats and derived-cache keys to the
[storage format](wikis-storage-format.md); limits and the `[wikis]` configuration to
the [budgets](wikis-budgets.md); the commit protocol to the
[authority and commit ADR](wikis-authority-commit.md); and required controls to the
[threat model](wikis-threat-model.md). None of them is restated here.

## Existing mechanisms this contract reuses

- Router: `crates/rbit-web/src/routes/mod.rs` `page_routes` layers (error pages,
  locale, session, rate limit, `csrf_origin_check`, `partial_responses`,
  `html_caching`, security headers, timeout, compression, 1 MiB `MAX_FORM_BYTES`).
  Feature areas own a `routes()` table merged there; Actions merges its table only when
  the instance enables Actions, which is the model for an instance-disabled wiki
  (`[wikis] enabled`, budgets).
- Header/tabs: `RepoHeader` in `crates/rbit-web/src/views/repo.rs`, built by
  `RepoPage::header` in `crates/rbit-web/src/routes/repo.rs`, rendered by
  `crates/rbit-web/templates/repo/subnav.html` with `active_tab` and optional
  `actions_url`.
- Partial navigation: `crates/rbit-web/src/partial.rs` cuts title, `<main>` and the
  out-of-band `#repo-subnav` for boosted same-repository GETs carrying `Rbit-Partial`;
  `crates/rbit-web/assets/nav.js` sends it, rotates `Rbit-Nav-Epoch` after unsafe
  requests and moves focus to `<main>`. `templates/base.html` sets `hx-boost` on
  `<body>`, a skip link, and htmx `responseHandling` that swaps every status but 204.
- Caching: `html_caching` in `crates/rbit-web/src/middleware/cache.rs` (private,
  no-cache, weak ETag over catalog key and body, `Vary: HX-Request, Rbit-Nav-Epoch`),
  `cache_pinned` for commit-pinned anonymous-readable responses, `preload_guard` and
  `is_unsafe_to_preload` for download paths.
- Cross-site writes: `csrf_origin_check` in `crates/rbit-web/src/middleware/security.rs`
  rejects unsafe methods whose `Sec-Fetch-Site`/`Origin` is cross-site. There is no
  per-form CSRF token; wiki forms add none. The `request_token` below is an idempotency
  key for lost-response retries, not a CSRF defense.
- Errors: `WebError` in `crates/rbit-web/src/error.rs`; `NotFound`'s message already
  covers "doesn't exist, or you don't have access". `SignedIn` in
  `crates/rbit-web/src/extract.rs` redirects anonymous users to `/login?next=`.
- Form retry: `crates/rbit-web/src/routes/edits.rs` re-renders 422 for validation and
  409 with a `Conflict` panel when the branch moved, and answers success with
  `Redirect::to` (303).
- Rendering: `render_markdown` in `crates/rbit-web/src/render/markdown.rs` (ammonia
  sanitization last, prefixed heading IDs) with `DocumentBase` from
  `crates/rbit-web/src/urls.rs`; `crates/rbit-web/src/render/document_cache.rs`;
  bounded preview in `crates/rbit-web/src/routes/markdown_preview.rs`.
- URLs: `encode_path` and `RepoUrls` in `crates/rbit-web/src/urls.rs`.
- Localization: `RequestLocale` in `crates/rbit-web/src/localization.rs`; typed
  feature catalogs under `crates/rbit-localization/src/catalog/` (for example
  `actions.rs` with dotted IDs such as `actions.run.workflow`).

## Routes

`W` is `/{owner}/{repo}/wiki`. Human-readable reads use the page path. Every other page
operation is keyed by the stable page ID under the reserved `W/-/` namespace, because
an Axum catch-all cannot carry a suffix (`W/{*path}/edit`), forms must survive a
concurrent rename, and form actions should not embed arbitrary Unicode. Each form's
GET and POST share one URL so a re-rendered 409/422 that htmx pushes stays reloadable.

| Flow | Method and path | Permission | Success | Slice |
| --- | --- | --- | --- | --- |
| Wiki home | GET `W` | repo read | home page, or empty state | W1 |
| Read page | GET `W/{*path}` | repo read | 200; alias/non-canonical form 301 to canonical | W1 (alias W3) |
| Page index/tree | GET `W/-/pages?parent=&cursor=` | repo read | paginated children; fragment for tree | W1 |
| History | GET `W/-/page/{id}/history?cursor=` | repo read | paginated revisions | W2 |
| Revision | GET `W/-/page/{id}/revisions/{rev}` | repo read | rendered revision + restore form for writers | W2 |
| Compare | GET `W/-/page/{id}/compare?base=&head=` | repo read | diff; ordinary GET picker form | W2 |
| Edit | GET, POST `W/-/page/{id}/edit` | repo write | POST: 303 to read page | W2 |
| Create | GET `W/-/new?path=&parent=`, POST `W/-/new` | repo write | POST: 303 to new page | W2 |
| Preview (no-JS) | POST to edit/create with `intent=preview` | repo write | 200 form with preview region | W2 |
| Preview (HTMX) | POST `W/-/preview` | repo write | fragment, `no-store` | W2 |
| Restore revision | POST `W/-/page/{id}/revisions/{rev}/restore` | repo write | new revision; 303 to read page | W2 |
| Rename | GET, POST `W/-/page/{id}/rename` | repo write | 303 to new path | W3 |
| Delete | GET, POST `W/-/page/{id}/delete` | repo write | tombstone; 303 to history | W3 |
| Undelete | POST `W/-/page/{id}/undelete` | repo write | 303 to read page | W3 |
| Search | GET `W/-/search?q=&cursor=` | repo read | results; Explore-style fragment | W3 |
| Media list/upload | GET, POST `W/-/media` | read / write | POST: 303 to list | W3 |
| Media download | GET `W/-/media/{content_id}/{name}` | repo read | bytes, attachment rules | W3 |

All mutations are POST forms; no GET has a side effect, and no HTMX-only verb exists.
POST bodies never carry a redirect destination; targets are derived server-side, as in
the language settings form. Delete redirects to history, whose tombstone banner and
undelete control make a flash-message mechanism unnecessary. Mutation forms carry the
expected revision (or expected absence for create) as a hidden field; the domain's CAS
decides staleness. Every mutation form (create, edit, restore, rename, delete,
undelete, media upload) also carries the hidden field `request_token`: 32 lowercase hex
characters (128 random bits) generated when the form is rendered, as the ADR's
"Request token" section defines. A browser resubmission of the same form sends the
same token, so a lost response resolves to the already-finalized result. Every
re-rendered 409/422/503 form carries a fresh token. Preview never consumes one.
Edit/preview POSTs need a feature `DefaultBodyLimit` derived from the
domain page limit, as `markdown_preview::routes` does; media upload streams outside
the page router, as `edits::upload_routes` does.

## Status codes and existence

| Condition | Response |
| --- | --- |
| Repository missing or unreadable (private to this viewer) | 404, the existing repository not-found body |
| Wikis off for the instance (routes not merged) or the repository | 404 via the router fallback / handler |
| Page path, page ID, revision, media content ID missing or of another repository | 404 |
| Malformed path or ID the domain cannot parse | 404 on reads; 422 on a create/rename field |
| Tombstoned path | 404; writers additionally see an undelete link in the body |
| Anonymous on a write route | `SignedIn` redirect, before any lookup, so identical for any path |
| Signed-in reader on a write route of a readable repository | 403 `WebError::Forbidden` |
| Archived repository mutation | refused after the permission check (see open questions) |
| Field validation; "nothing changed" (as `EditError::NothingChanged` maps today) | 422, same form re-rendered with an error summary |
| Stale expected revision; path clash on create/rename | 409, form re-rendered with conflict panel |
| Busy: writer-lock wait exceeded, intent or blocking-slot bound reached | 503 with `Retry-After` and `no-store` (`WebError::Unavailable` / `unavailable_response`); forms re-rendered with the user's text |
| Page content unavailable, integrity hold or unsupported wiki format | explicit unavailable state with a stable code; status is open question 7 |
| Cross-site unsafe request / oversized body / throttle | existing 403 / 413 / 429 |

IDs are looked up by (repository, ID), never globally, so probing an ID cannot reveal
another repository's page. Lookups, 304 validation, search, history and media all run
only after repository read authorization; `html_caching` computes ETags from the
rendered body, so a 304 can never precede the handler's authorization. A stale form
keeps the user's submitted text, shows current revision metadata with a link to
`compare?base={expected}&head={current}`, and takes the current revision as its base
only on explicit resubmission (the `edits.rs` model). It never auto-merges.

## Repository tab

`RepoHeader` gains `wiki_url: Option<String>` beside `actions_url`, compared in
`subnav.html` as `active_tab == "wiki"`, placed after Releases. `RepoPage::load` reads
the wiki's enabled/has-pages projection concurrently with refs, as it does for
Actions: one bounded catalog query, no Git read. The tab is absent when the instance
or repository has wikis off, or when the wiki is empty and the viewer cannot write
(no dead tab). Unreadable repositories render no header at all. Hiding the tab or any
control is never authorization; every route rechecks. The tab carries
`data-repo-tab="wiki"` and `data-preload="visible"` like its peers, so `partial.rs`
and `nav.js` treat wiki pages as ordinary same-repository navigation.

## Layout regions

All regions sit inside the existing `<main id="main">` below `#repo-subnav`, using
existing tokens and classes (`ink`, `ink-muted`, `line`, `subtle`, `box`, `btn`,
`tab`). No accent color, gradient or wiki-specific skin.

- Page tools row: `Pages` drawer button (narrow only), breadcrumbs as
  `<nav aria-label>` with the current crumb `aria-current="page"`, then Edit/History
  and an overflow `<details data-menu>` with Rename/Delete. Write controls render only
  for writers on non-archived repositories.
- Header: one `<h1>` page title; metadata line with last editor, `<time datetime>`
  and short revision ID in monospace.
- Sidebar: `<nav aria-label>` page tree with a `role="search"` GET form at its top.
  Ancestors of the current page are server-expanded; each level is bounded with a
  "show all" link to `W/-/pages?parent=`.
- Table of contents: server-rendered `<nav aria-label>` from rendered headings,
  linking the prefixed IDs that `render_markdown` emits. Unlike the client-built
  outline in `templates/repo/documents.html`, it must exist without JavaScript.
- Document canvas: `<article>` with readable measure; `pre`, tables and diffs scroll
  in their own `overflow-x:auto` container, never the page. Long titles and crumbs wrap
  or truncate with the full text available via `title` and the accessible name.
- Footer: last-edited summary, Edit and History links.
- Empty states: readers see an explanatory sentence; writers also get a create button.
  Search no-results and empty history have their own messages.

Desktop at `xl`: tree, article, TOC columns. At `lg`: tree and article; TOC becomes
a `<details>` above the article. Below `lg`: single column; the tree moves into a
drawer built on `<details>` (works without JavaScript). Enhancement adds Escape to
close, focus return to the trigger and close-on-navigation. Layout uses logical
properties (`ps-`, `pe-`, `start-`, `end-`) so RTL mirrors without overrides.

## No-JavaScript baseline and HTMX enhancement

Without JavaScript every flow is a link or an ordinary form with a full response:
tree levels via `W/-/pages`, compare via a GET picker, preview via
`intent=preview` re-rendering the form with the user's text and a preview region,
search via a GET form, confirmations as their own GET pages, and POST-redirect-GET on
success. Drawer and TOC are `<details>`.

Enhancements are bounded and optional:

- Navigation between wiki pages uses the existing boosted partial swap of `<main>` and
  tabs; `nav.js` already pushes history, moves focus to `<main>` and keeps
  `historyCacheSize` at 3. No narrower canvas-only swap in W1 unless measured; tree
  state is server-derived from the current path, so a whole-`<main>` swap loses none.
- Hash links scroll to the prefixed heading ID after swap; history restore keeps the
  browser's scroll restoration. TOC links do not push extra history entries.
- Tree expansion: `hx-get` `W/-/pages?parent={id}` into that level's list on first
  `toggle`, no push; the no-JS "show all" link remains.
- Preview: the editor's preview control posts the form to `W/-/preview` targeting
  `#wiki-preview` with no push, `aria-busy` while pending and a `role="status"`
  announcement on completion. It uses the same `render_markdown`/sanitizer path and
  `DocumentBase` as saved pages; there is no client-side renderer.
- Search: Explore's pattern (see [localization contracts](localization-contracts.md)):
  GET form with `hx-get`, a single `HX-Target` of `wiki-search-results` returns that
  component plus its title; boosted/history-restore or ambiguous headers get the full
  page; `hx-push-url`, `hx-sync` replace, debounced input and a `role="status"` count.
- Mutations: boosted POSTs follow the 303; 409/422 bodies swap because htmx is
  configured to swap them; focus moves to the error summary (`role="alert"`,
  `tabindex="-1"`). The epoch rotation in `nav.js` invalidates earlier preloads.
- Loading: the existing delayed progress bar; no spinners that convey state by
  motion alone.

## Accessibility acceptance

- Landmarks: one `main`; named `nav` regions for tabs, breadcrumbs, tree and TOC
  with distinct localized names; `article` for content; `role="search"` form.
- Headings: page title is the sole `h1`; rendered content headings are demoted or
  offset so authored `#` does not create a second `h1` (domain/render decision).
- Focus: visible focus on every control; skip link reaches `#main`; drawer traps no
  focus without JS and returns focus when closed with it; error summary focused.
- Names: icon-only buttons have localized `aria-label`; current tree item and crumb
  use `aria-current="page"`; expandable tree items expose expanded state.
- Non-color cues: tombstoned, conflict, archived and diff add/remove states carry
  text or an icon plus text, and diffs keep `+`/`-` markers.
- Motion: drawer and TOC transitions disabled under `prefers-reduced-motion`.
- Keyboard-only acceptance for every row of the route table, at 360px width and
  desktop, light/dark, `en`, `en-XA` and `ar-XB`.

## Localization

Chrome, validation, confirmations, accessible names, empty states and conflict text
come from a new typed `wiki` catalog module following the Actions catalog pattern,
with dotted stable IDs: `wiki.tab`, `wiki.page.*`, `wiki.tree.*`, `wiki.history.*`,
`wiki.compare.*`, `wiki.edit.*`, `wiki.conflict.*`, `wiki.validation.*`,
`wiki.delete.*`, `wiki.search.*`, `wiki.media.*`, `wiki.a11y.*`. Domain errors map to
messages by stable code; templates contain no English literals and no `safe` filter.

- Plurals: counts are typed unsigned arguments. Only messages whose Spanish noun forms
  are reviewed may use the existing singular/plural rule; no general plural engine.
- Dates: `<time datetime>` carries a locale-independent UTC timestamp. Displayed
  dates and relative times need the not-yet-written date presentation contract;
  until then any English date text is an explicit `lang="en"` boundary.
- User content: titles, paths, snippets and articles are never translated. Interpolated
  titles are FSI/PDI isolated in messages; the article and title use `dir="auto"` and
  do not claim the UI language.
- The subnav region is currently an explicit `lang="en" dir="ltr"` boundary in
  `base.html`; the Wiki tab label follows whatever that region does (see open
  questions).
- Long-translation (`en-XA`) and RTL (`ar-XB`) diagnostics must show no clipped
  controls, no page-level horizontal overflow and correctly mirrored tree, crumbs and
  drawer.
- Locale-independent: URLs, page/revision/media IDs, query keys and values (`intent`,
  `q`, `cursor`, `base`, `head`), form field names (including `request_token`), API
  error codes and event names.

## Page paths in URLs

The router accepts `W/{*path}` as Axum's percent-decoded catch-all, bounds its byte
length before domain work, and hands it to the domain path parser; parse failure is a
404 on reads. A first segment of `-` is reserved for `W/-/` and must be rejected by
the domain as a page name. Generated links encode each segment with `encode_path`, so
`/` separates segments and `%`, `?`, `#` and spaces never survive raw. Unicode
normalization, case folding, reserved names, trailing slashes and relative-link
resolution are domain rules; the router only redirects (301, uncacheable) to the
canonical form the domain returns. Page IDs and revision IDs in `W/-/` routes are
bounded ASCII tokens parsed by the domain; `{content_id}` is the storage format's
64-character SHA-256 hex ID, looked up only within the page's wiki. Repository
renames keep working through the existing `moved_permanently` redirect in
`middleware/errors.rs`, which preserves the path after `/{owner}/{repo}`.

## Cache dimensions

| Route group | HTTP cache | Server-side derived cache key |
| --- | --- | --- |
| Home, read, revision, history, compare, index | `html_caching` defaults; Vary adds `Rbit-Partial` and language dimensions from the shared pipeline | rendered body: wiki ID, body OID, render-policy version (storage format); navigation: wiki ID, head OID, navigation policy, parent |
| Tree/search fragments | as above plus the response-shape headers Explore varies on | none beyond bounded query results |
| Edit, create, rename, delete forms | defaults; preload allowed (no side effects) | none |
| Preview, POST responses | `private, no-store` | none |
| Media download | own headers: `nosniff`, attachment unless an allowlisted raster type (budgets), `cache_pinned` only for anonymous-readable repositories, else revalidated | none; bytes are read from `wiki-media/<repo_id>/<wiki_id>/` by content ID |

Locale and viewer do not enter rendered-content keys because stored content is not
localized and every reader of one repository shares its ACL; both remain HTTP
dimensions through the catalog key, `private` and per-viewer controls in the body.
Authorization precedes every derived-cache lookup. Link resolution depends on other
pages, so cached bodies hold no resolved links; links resolve per request after
authorization (storage format). Media downloads must join the `is_unsafe_to_preload`
repository areas so hover never fetches them.

## Open questions

1. Archived-repository mutation status: `ServiceError::Archived` currently becomes 400
   through `WebError::BadRequest`; adopt that or define 403/409 centrally.
2. Router precedence of the static `W/-/...` routes beside the `W/{*path}` catch-all
   must be proven by a router test before W1.
3. A user-facing Git remote (W4) cannot use `{repo}.wiki.git`: `repo.wiki` is a valid
   repository name today, so that path collides with a real repository.
4. Subnav language: localize the whole tab row as one unit, or keep the Wiki label
   inside the existing English boundary.
5. Whether delete belongs in W2 with restore or W3 with rename; this contract uses W3.
6. Tab placement and the empty-wiki visibility rule need product confirmation.
7. HTTP status for the unavailable states (ADR "page content unavailable", integrity
   hold, unsupported format): `503` without `Retry-After`, or a `200` page that states
   the condition. Busy stays `503` with `Retry-After` either way.
8. The per-repository wiki switch named above has no column in the storage format;
   decide whether it exists, and where it is stored.
9. W1 read routes have no write path to create pages (create is W2), so W1 acceptance
   can only exercise empty wikis unless creation moves into W1.
