# Frontend wave — virtual blocks and deferred slots

Contract for four parallel workstreams that make the heaviest pages (long files, blame,
large commit/compare diffs, large pull requests) scroll without flicker and appear sooner,
and make preloading cheaper on desktop and useful on phones.
House rules from [milestone-1.md](milestone-1.md), [../HANDOFF.md](../HANDOFF.md) and
[CONTRIBUTING.md](../../CONTRIBUTING.md) apply unchanged: thin handlers, no logic in
templates, strict CSP (no inline `style=` or scripts), clippy pedantic clean, atomic
commits with the `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>` trailer,
mobile first (375 px), monochrome design.

## Problem

Pages with a lot of content flicker and jank while scrolling. Causes found in the code:

1. **Per-line containment.** Every `.code-line` (file viewer, blame) is its own
   `content-visibility: auto` element, so a 20,000-line file has 20,000 boundaries for the
   browser to track, and lines scrolled to quickly paint blank before they are laid out.
2. **Guessed heights.** `.diff-file[open]` reserves `auto 24rem` until a file is first
   rendered; real heights then arrive mid-scroll and the content and scrollbar jump.
3. **Tables cannot be skipped.** Diffs are `<table>`s with one `<tr>` per line;
   `content-visibility` has no effect on table rows, so the pull request "Files changed"
   view lays out every line of every file.
4. **Server work done twice or in sequence.** The pull request tab header runs a second
   full compare and diff only to count files; the conversation and commits tabs run one
   for the same counts. Independent calls (diff and statuses, compare and the fork
   network, diff and reviews) run one after another.

`scripts/perf/` reproduces this with a deterministic fixture and measures it
(`scripts/perf/BASELINE.md`).

## Design

### Virtual blocks (client cost)

Long row lists render as runs of **blocks** of at most `BLOCK_ROWS` (128) rows
(`src/views/blocks.rs`: `blocks()`, `Blocker`, `Block`). Each block is one
`content-visibility: auto` container, so a 20,000-line file has 157 boundaries instead of
20,000, and each boundary is a whole screen or two of content.

The placeholder height is **exact**, so nothing moves when a block is laid out:

- every row inside a block is exactly `--row-h` (1.25rem) tall: code lines, diff lines,
  hunk headers and "No newline at end of file" rows alike;
- the block carries `data-rows="{{ block.len() }}"`; app.css's `.vblock` turns it into
  `contain-intrinsic-block-size` with typed `attr()`, and `assets/blocks.js` sets the
  same value through the CSSOM in browsers without it (the CSP allows CSSOM writes, not
  `style=` attributes);
- anything that is not a fixed-height row (a review comment thread, an inline form)
  ends the block before it (`Blocker::end_block`) and renders between blocks, outside
  any `.vblock`.

`content-visibility: auto` keeps skipped content in the DOM: find in page, `#L…` links,
text selection and screen readers keep working, and pages still work without JavaScript.

#### Hybrid containment for diffs

Measured in headless Edge (6× CPU throttling, `scripts/perf/trace.mjs`), blocks inside a
diff's horizontal scroller (`.diff-scroll`) removed every height change but cost more per
frame than the old file-level containment with its guessed height: a containment
boundary inside a scroll container is mapped through the scroller's clip each frame to
decide whether it is near the viewport (`computeIntersections`), and every laid-out
scroller and row clip adds paint property nodes to walk (`PrePaint`) and layerize. So
diffs contain whole files, outside their scrollers, with exact heights:

- Each open changed file (`.diff-file`, commit, compare, new pull request and pull request
  files) is one `content-visibility: auto` element carrying `data-rows`: its whole height
  in rows. Everything in it is a whole number of rows: the header (`FILE_HEADER_ROWS`, 2,
  hairline included), the pull request's "View file" bar (`FILE_BAR_ROWS`, 2), the
  placeholder of a file without a diff ("Binary file not shown", `PLACEHOLDER_ROWS`, 3)
  and every diff row. The box's border is outside the placeholder
  (`contain-intrinsic-block-size` sizes the content box), so nothing is approximated.
- The diff's own horizontal scrollbar is the one thing CSS cannot know: it takes space
  only where the platform's scrollbars do (not on phones or macOS) and only when the diff
  is wider than the file. `blocks.js` measures the scrollbar once and, for files whose
  diff's minimum width (from the widest line, `cols-N`) exceeds the file's width, sets
  the placeholder through the CSSOM with the scrollbar added. It recomputes when the
  column of files changes width; a file rendered before the change keeps the size
  `auto` remembered, which can be off by the scrollbar until it is rendered again.
  Without JavaScript the placeholder of such a file is short by the scrollbar until its
  first render.
- Blocks only where they pay: diffs of more than `BLOCKED_ABOVE_ROWS` (512) rows also
  keep their rows in `.vblock`s inside the file. Laying out a file costs about 0.4 ms
  per row at 6× (a 4,000-row file is one 1.5 s frame there, ~250 ms unthrottled); at 512
  the worst first render stays near two 60 Hz frames on a desktop CPU. Between 256 and
  ∞ the threshold made no measurable difference to scrolling the fixture pages, whose
  diffs are either under 500 or over 1,100 rows.
- Pull request files with comment threads or the inline form have no exact height, so
  they carry no `data-rows` and are not contained as a whole: their rows keep exact
  blocks (`Blocker` ends one before each thread) and the threads render between them.
- Diff rows no longer clip their overflow (`overflow-y: clip` per row): one clip node
  per row made pre-paint and layerize about 13 % of the main thread's time scrolling
  the 106-file commit. Rows stay exactly one row tall; only an unusually tall glyph can
  draw past its row.

The file viewer and blame keep blocks inside their single scroller: one scroller, not
hundreds, and against the old per-line containment they cut main-thread time in a sweep
by more than half (see **Measurements** below).


#### Measurements

Headless Edge, 6× CPU throttling, 5 viewports/s, 1280×1000, pages visited in turn
(`scripts/perf/trace.mjs` and `sweep.mjs`); median of 7 rounds (5 for the file and
blame of the 20,000-line `src/huge.rs`). "Old" is the code before this wave (per-line
containment, diff tables, files with a guessed `auto 24rem`), "blocks" the wave's blocks
inside each diff's scroller, "hybrid" this design. Main-thread ms per 6 s sweep:

| Page | RunTask old / blocks / hybrid | PrePaint | computeIntersections | Frames > 33 ms | Height changes |
|---|---|---|---|---|---|
| 106-file commit | 4967 / 5314 / 4970 | 391 / 778 / 434 | 149 / 471 / 260 | 40 / 40 / 30 | 19 / 0 / 0 |
| Compare, 152 files | 5060 / 5541 / 4789 | 374 / 979 / 392 | 216 / 629 / 347 | 40 / 47 / 45 (29 / – / 21 rerun) | 9 / 0 / 0 |
| Pull request files | 2824 / 3507 / 3225 (2549 / – / 2429 rerun) | 179 / 252 / 215 | 330 / 1038 / 619 | 10 / 4 / 2 | 10 / 0 / 0 |
| File, 20,000 lines | 6769 / 2674 / 2471 | 790 / 235 / 205 | 2592 / 474 / 438 | 7 / 6 / 11 (old: 8 frames in all) | 1 / 0 / 0 |
| Blame, same file | 6739 / 5379 / 5495 | 787 / 212 / 226 | 3795 / 416 / 402 | 11 / 15 / 15 (old: 13 frames) | 4 / 0 / 0 |

Scroll corrections are 0 and every placeholder is within 2 px of its rendered height on
every page with blocks or the hybrid; without throttling every page holds 60 fps. The
three-way runs of the pull request and compare pages were noisy (wide interquartile
ranges); the two-way reruns are the steadier numbers. The hybrid draws more frames in
the same sweep, so per frame it is cheaper than old on every diff page except for
`computeIntersections`.

`computeIntersections` stays about 1.5–1.8× old's on diff pages (≈ 0.4–0.8 ms per frame
at 6×). It is the relevance check of the file-level containment itself: making the files
`content-visibility: visible` removes it, while making the inner blocks visible, removing
the page's own IntersectionObservers or ResizeObserver, the scroller, its container
query, the sticky headers, or giving files old's `auto 24rem` placeholder changes it by
10 % or less. With the same 152 targets and file bodies hidden it is still about 1.3× old's
per call; the rest of the difference is inside Chromium and was not attributed further.
It is outweighed by lower style, layout and layerize time.

### Deferred slots (server cost)

Secondary data that is expensive and not needed for the first paint renders as an empty,
fixed-size **slot** and arrives with one batched htmx request of `hx-swap-oob` fragments
— the pattern of `repo/tree.html` → `entry_commits` (`src/routes/repo.rs`), generalised.
Main content (code, diffs) is never deferred: showing it twice (plain, then highlighted)
would itself flicker. It gets faster through concurrency and caching instead.

### Server hot paths

Compute once, in parallel, and cache what is immutable: results keyed by commit or blob
ids never change.

## Workstreams and file ownership

Each workstream works in its own worktree, on its own branch cut from
`perf/virtualization`, and commits there. Edit only the files you own; anything else goes
in your final report as an integration request. Paths are under `crates/rbit-web/`
unless they say otherwise.

### V — Viewer blocks (file viewer and blame)

Owns:
- `templates/repo/blob.html`, `templates/repo/blame.html`.
- The view models in `src/views/repo.rs` (`BlobContent` and what feeds it) and
  `src/views/blame.rs`, and their tests. Build blocks in the view layer: handlers keep
  passing what they pass today.
- app.css **File viewer lines** section.
- e2e assertions about these two pages.

Delivers:
- The file viewer renders its lines in blocks: `<div class="vblock" data-rows="…">` of
  `.code-line`s; per-line `content-visibility` goes; `.code-line` is exactly one row tall.
  Line ids (`L{n}`), the CSS-drawn line numbers, `:target` highlighting and nav.js's
  line-number click keep working unchanged.
- Blame: a 20,000-line file with thousands of small hunks must not lay out every hunk.
  Design how hunks group into blocks (for example consecutive hunks packed into one
  `.vblock` up to `BLOCK_ROWS` rows, with a hunk header whose height is a whole number of
  rows and counted in `data-rows`); document the choice in the module comment.

### D — Diff blocks (commit, compare, new pull request, pull request files)

Owns:
- `templates/repo/file_diffs.html`, `templates/pulls/files.html`, the `diff-view` markup
  in `templates/pulls/review_comment.html` if it changes.
- The diff view models in `src/views/commit.rs` and `src/views/compare.rs`, the
  files-tab view models in `src/views/pulls.rs` (`DiffRowView`, `DiffHunkView`,
  `PullDiffBody`, `PullFileView`, `PullFilesPage`, `hunk_view`) and their tests.
- app.css **Diffs** section; `assets/diff.js`; `assets/nav.js` (`HEAVY_CONTENT` and any
  selector that follows diff markup).
- e2e assertions and helpers about diff markup (`crates/rbit-e2e/tests/e2e/pulls.rs`
  `line_anchor`, `repositories.rs`, `edge.rs`).

Delivers:
- Diffs drop `<table>` for rows of fixed height grouped in `.vblock`s: per row the old
  and new numbers, the sign and the code, with the colours, spacing and horizontal
  scrolling of today (backgrounds span the full scroll width; numbers and signs stay out
  of copied text). Number columns are as wide as the file's largest line number needs,
  chosen with classes (no `style=`).
- File-level `content-visibility` with its guessed 24rem goes; blocks do the skipping.
- Pull request rows keep their ids (`{file_anchor}-L{n}` / `-R{n}`), the comment links
  and their `hx-get`/`hx-select`/`hx-target` attributes, `aria-label`s and `tabindex`.
  Comment threads and the inline form render between blocks.
- Fix: diff.js's capture `toggle` listener calls `syncCollapseAll(null)` on the pull
  request files page, which has no `[data-diff-layout]`, and throws on every toggle.

### S — Server hot paths and slots

Owns:
- `src/routes/pulls.rs`, `src/routes/history.rs`, `src/routes/compare.rs`,
  `src/routes/repo.rs`, `src/routes/blame.rs`, `src/render/highlight.rs`,
  `src/middleware.rs`, `src/routes/mod.rs`, `src/state.rs` (or wherever shared state
  lives), service code in `crates/rbit-services` it needs.
- Compact highlighting (see **Data efficiency**) and app.css **Syntax highlighting**.
- `templates/pulls/header.html`, `PullHeaderView` and what builds it in
  `src/views/pulls.rs` (D owns the rest of that file), new slot fragment templates.
- A new app.css **Deferred slots** section placed just before **Themes**, if slots need
  styles.

Delivers:
- The files tab computes the pull request's changes once (the header takes its counts
  from them). The conversation and commits tabs stop computing a full diff for counts:
  counts come from a cache keyed by the base and head commit ids when present, otherwise
  from a deferred slot.
- Independent awaits run concurrently: commit diff and statuses (`history.rs`), compare
  and the fork network (`compare.rs`), changes and reviews (`pulls.rs`).
- A byte-bounded in-memory cache of highlighted lines keyed by blob id (or LFS object
  id), file name and a highlighter version, used by the file viewer and blame.
- Commit-pinned fragments (`tree-commits`, any new slot route) are cacheable: `private,
  max-age=31536000, immutable` when the repository is readable anonymously; private
  repositories keep revalidation, so a shared browser never serves one account's private
  fragment to the next.

### P — Preloading

Today `nav.js` marks links `preload="mouseover always"` for the vendored htmx preload
extension: every hover past 100 ms sends another request, a preload older than
`PRELOAD_MAX_AGE_SECONDS` (10 s) is revalidated at the cost of a full render, a pointer
sweeping across a list preloads every link it rests on, and on touch screens every scroll
gesture that starts on a link fetches it while phones gain only ~100 ms on a real tap.

Owns:
- The **preloading** and **partial swaps** sections of `assets/nav.js` (`NO_PRELOAD`,
  `isPreloadable` and its callers, the `htmx:beforeSwap` partial-swap handler; D owns the
  rest of the file) — or a new `assets/preload.js` module imported
  from `app.js` (register it in `src/assets.rs`'s module test and `base.html`'s
  `modulepreload` list).
- `PRELOAD_HEADER`, `PRELOAD_MAX_AGE_SECONDS`, `is_unsafe_to_preload` and
  `preload_guard` in `src/middleware.rs` and their tests (S owns the rest of the file);
  partial responses (see **Data efficiency**): a new module plus the markers in
  `templates/base.html` and one layer line in `src/routes/mod.rs`.
- Removing the vendored extension: `package.json`/`package-lock.json`,
  `scripts/vendor.mjs`, `assets/vendor/htmx-preload.min.js`, its `<script>` tag and the
  `hx-ext="preload"` attribute in `templates/base.html`, `THIRD-PARTY.md` if it lists it.
- `data-preload="visible"` on the few links worth preloading ahead of any intent (the
  repository tabs in `templates/repo/subnav.html`, "next page" links).

Delivers a purpose-built preloader:
- Each URL is preloaded at most once per freshness window (an in-memory map cleared when
  the navigation epoch changes), never for the current page, at most two at a time.
- Desktop intent: hover held ~80 ms (cancelled on leave) or `mousedown`.
- Touch: start on `touchstart`, cancel when `touchmove` or a scroll follows, so scrolling
  a list fetches nothing.
- Viewport preloading for `data-preload="visible"` links: `IntersectionObserver`, when
  idle, with a per-page budget; off under Save-Data or a 2g connection.
- One freshness value shared by the server's preload `max-age` and the client window,
  raised to ~30 s; the epoch keeps data from going stale after the viewer's own changes.
- Unchanged safety: same-origin, `NO_PRELOAD` mirrored by the server's guard, never
  downloads or side-effecting paths; preloads still send `HX-Preloaded` so the server can
  tell them apart.
- Measured with the network panel before and after: requests for repeated hovers, for a
  sweep across a file list, and while scrolling a list at 375 px.

### Data efficiency (shared by P and S)

The site has few layouts and many pages of data; responses should carry the data and as
little else as possible, while staying hypermedia (server-rendered HTML that htmx swaps).
Measured on the fixture (gzip): chrome (header, menus, footer) is 59–79 % of a typical
page, whose `<main>` is 0.9–2.3 KB; a 3,000-line highlighted file is 1.9 MB with 34,500
`<span>`s, most of them unstyled wrapper scopes (`hl-source`, `hl-meta …`).

- **Partial responses (P).** nav.js already swaps only `<main>` and `#repo-subnav` between
  pages of the same repository, but the server sends the whole page. A boosted request
  that can be swapped partially says so in a request header (e.g. `Rbit-Partial`, carrying
  the repository scope); the server answers with only `<title>`, `#main` and the
  out-of-band `#repo-subnav`, generically (a new module, e.g. `src/partial.rs`, cutting the
  rendered page at markers the layout emits — the way `timing.rs` marks the render time —
  or rendering just the block), and adds the header to `Vary`. Preloads use the same
  header so the click reuses them. The full page stays the answer to every other request.
- **Compact highlighting (S).** Highlighting emits one span per token, flattened (no
  nesting), with one short class per *styled* category (comment, string, number,
  keyword, function, type, tag, attribute, heading, bold, italic, invalid), resolved in
  Rust from the scope stack the way the cascade resolves today's classes, adjacent
  tokens with the same style merged. Visual output must be identical (test it against
  today's rules); app.css **Syntax highlighting** moves to the new classes and keeps the
  `--syntax-*` tokens, so tinted themes keep working.

Rejected: shipping JSON plus client-side templates (duplicates rendering in JavaScript,
adds an injection surface, breaks the no-JS fallback, and gains little: repeated markup
compresses well, and the data part of a typical page is already ~1–2 KB). Later, if
measurements call for them: `ETag`s from inputs (so a revalidation never renders) and
Compression Dictionary Transport (a site dictionary of the templates' markup, so only the
data travels; Chromium only).

## Contracts between workstreams

- **Blocks** (`src/views/blocks.rs`, app.css **Virtual blocks**, `assets/blocks.js`) are
  shared and owned by the coordinator; request changes in your report.
- **Row height.** Inside a `.vblock`, every child is exactly `var(--row-h)` tall. A
  component with taller chrome inside a block (a blame hunk header) makes it a whole
  number of rows and counts those rows in `data-rows`.
- **Markup kept stable:** line ids `L{n}`; pull request row ids and comment URLs; the
  `id="{anchor}-path"` span the copy button reads; every `data-diff-*` hook diff.js uses.
- **Slots** have a meaningful no-JS state (empty or a link), a fixed size so filling them
  shifts nothing, and ids from one Rust function shared by the page and its fragment.
- **No new dependencies** (crates or npm) without a note in your report explaining why.

## Verification (each workstream)

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, `npm run build` (output stays untracked). Then load your pages
from the `scripts/perf/` fixture, at 375 px and desktop, light and dark, and run
`scripts/perf/scroll-probe.js` before and after: report DOM size, frames over 16.7 ms
and 33 ms, the worst frame, blank samples and layout shift.

## Not in this wave

- Streaming one response into many slots (one request instead of two) and evicting
  far-off blocks from the DOM: only if measurements after this wave still call for them.
- `ETag`s computed from inputs (commit ids, viewer) so a revalidation skips rendering.
- A fragment route for the pull request line-comment form (today the whole page renders
  and `hx-select` keeps one file).
