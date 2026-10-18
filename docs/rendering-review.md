# Rendering and virtualization review — 2026-10-17

This document records the initial diff-only follow-up. The same isolated branch
subsequently adds an on-demand branch/tag picker, paged at 50 refs with server-side
search, ordinary navigation without JavaScript, and fresh repository access checks.
Tree, source, history and blame views share it. Counts remain independent of the
rendered links. The fragment endpoint avoids unrelated repository-header queries.
The root page captured from `/realworld/git` had 7,353 elements, of which 3,076 were
inside the closed 1,020-link picker. This is a DOM-work opportunity, not a measured
percentage latency improvement. The initial validation below applies to the diff
stage; subsequent Rust and browser checks cover the picker as well.

## Branches reviewed

The frontend performance changes are absent from the inspected `main` at `7b71630`.
Do not evaluate the current main checkout as if it includes the performance branches.
Local branch names and revisions inspected:

| Branch | Revision | Existing work |
|---|---|---|
| `perf/V` | `bffbcca` | 128-row source and blame blocks |
| `perf/D` | `1213e90` | Fixed-height block diffs instead of tables |
| `perf/S` | `66420ac` | Compact highlighting, highlight cache, deferred counts, concurrent reads |
| `perf/P` | `53e3650` | Partial HTML responses, bounded intent/viewport preloader |
| `perf/fixes` | `676c0d1` | Navigation, fragment and pinned-commit correctness fixes |
| `perf/virtualization`, `perf/H` | `2417ef7` | Combined wave, including server-computed line widths |
| Performance follow-ups | `327354d` | Combined wave plus security/backend performance commits |

Also inspected uncommitted work in `perf/H` (file-level containment, a 512-row
threshold, scrollbar sizing and clip changes) and the performance follow-ups
(preload cancellation, highlight job sharing/cache accounting, shared highlighted
lines, and measurement harness work). These changes remain owned by their existing
worktrees and were not modified, committed or merged by this review.

## Initial diff follow-up

Isolated branch `codex/rendering-followups`, based on `327354d`:

- Cache file containers, sidebar links and collapse controls for each mounted diff.
- Retain binary-search geometry reads for the current file; eliminate selector scans
  from the scroll callback. Do not schedule a diff frame on pages with no diffs.
- Skip geometry work while the sidebar is hidden, including closed mobile sidebars.
- Coalesce native bulk-toggle events into one toolbar update per animation frame.
- Invalidate indexes on htmx swaps, rebuild restored markup even with `data-ready`,
  release references during cleanup, and observe replacement page headers.

No CSS, server rendering, source rows, line anchors, selection or search behavior was
changed. This is an additional patch for the integrated performance branch, not a
replacement for the existing frontend wave or uncommitted follow-ups. Main's newer
uncommitted header/subnav changes need reconciliation when integrating the patch.

## Initial diff validation and evidence

`npm test`: 13 tests pass (eight new diff regressions and five existing service-worker
tests). `npm ci` and `npm run build` pass. JavaScript syntax and `git diff --check` pass.
No Rust code changed; the workspace Rust suite was not rerun, and the combined branch
is not being represented as release-validated.

A separate headless Edge check used the real before/after JS against an identical
synthetic DOM: 150 files, 128 rows per file, 96,612 elements, at 1280 px and 375 px.
Both viewport widths produced the following counts:

| Operation | Before (`327354d`) | Candidate |
|---|---:|---:|
| Selector calls during 60 scroll notifications | 300 | 0 |
| Geometry reads during those notifications | 420 | 420 |
| Selector calls after 150 native file toggles | 450 | 0 |
| Toolbar after collapse | collapsed | collapsed |
| Observed height after replacing header with a 96 px header | 0 px | 96 px |

The candidate additionally refreshes the current-file marker after toggles (seven
geometry reads in this fixture). These are work-count and correctness results, not
application latency, FPS, physical scrolling or Core Web Vitals measurements. The
synthetic page does not exercise the application's virtual-block CSS. The earlier
checked-in performance baseline explicitly says its browser run was invalid; it
must not be used to claim a speedup.

## Remaining opportunities, in order

1. **Integrate and validate the work that already exists.** The largest wins are on
   the performance branches and in pending work. Preserve current security fixes and
   newer repository navigation when bringing the frontend wave together. Measure the
   resulting application before changing more containment thresholds.
2. **Bound large metadata lists at their source.** Branch/tag pages call
   `reader.ref_details()` and render every result. Organization/user repository lists
   use `list_for_owner()` without page arguments. Directory pages turn every tree
   entry into a row. Add server search/paging for scale and then consider block
   containment for each page. Slicing only the rendered result still leaves full ref
   enumeration and commit summaries on the server; cache/index that work separately.
3. **Make inline review forms small responses.** The pull files template requests
   `row.comment_url` and uses `hx-select` to keep one file. A dedicated fragment route
   should avoid rendering the entire files page to open a line-comment form. Keep the
   same authorization and pinned-commit rules.
4. **Finish containment lifecycle checks.** The pending `perf/H` `blocks.js` observes
   each diff column without a corresponding cleanup/unobserve path. Test repeated
   navigation and release removed targets. Check horizontal scrollbar appearance,
   resized columns, theme/font changes, browser zoom, print, line links, copy and find
   in Firefox, Safari and Chromium before treating placeholder sizes as proven exact.
5. **Reduce initial work on small pages where measurements justify it.** All pages
   currently import diff, block, passkey and theme-picker modules. Consider lazy
   loading feature-specific code, preserving the first click, htmx navigation and
   keyboard behavior. The theme picker also schedules one animation callback per
   scroll event without a pending-frame guard. Precompressed assets are another
   server CPU opportunity; compare compressed bytes before adding a minifier.
6. **Use true DOM windowing only if memory/parse measurements justify it.** Current
   virtual blocks retain every row. They reduce style/layout/paint, but not all HTML
   parsing or retained DOM memory. Extremely large source/diff pages may eventually
   need paged content or a bounded DOM with explicit full-file search/copy/anchor
   support. This is a larger behavior change than CSS containment.

## Page-family assessment

| Page family | Existing bound or optimization | What remains |
|---|---|---|
| Landing | Eight recent repositories | Validate actual asset/LCP budget |
| Dashboard | Repository list capped at 100 | Profile filtering; avoid unnecessary row virtualization |
| Explore | 30 repositories per page | Retain paging and test navigation |
| Issues, pull lists, notifications, releases, commit history, admin lists | Existing pagination paths | Measure query/render cost before adding a virtual list |
| Issue/pull conversations | Cursor-paged timeline, service limit of 200 | Check retained DOM after repeated older-page loads and rich content |
| Branches/tags, owner repositories, large directories | Full collections in view paths | Highest-priority remaining list-size work |
| Source/blame | 128-row blocks, compact highlighting/cache on performance branch | Validate pending shared-line/cache work and browser behavior |
| Commit/compare/new pull/pull files | Fixed-height block diffs; bounded Git diff work | Integrate pending `perf/H`, verify comment-form fragments and scrolling |
| Themes | Content visibility; dialog loaded on demand | Coalesce index updates and measure filtering |
| Settings/auth/forms | Generally small documents | Avoid feature initialization unrelated to the page |

This is a source-level survey and targeted browser regression check, not proof that
every route is optimal. The next application benchmark should cover these families
on identical release builds and fixtures, cold/warm loads, boosted navigation,
back/forward, desktop/mobile widths and an idle machine. Record transfer and decoded
bytes, DOM nodes, server timing, long frames, layout shifts and interaction latency.

Browser behavior references: [content-visibility](https://web.dev/articles/content-visibility)
keeps content in the DOM and skips rendering work; descendant geometry reads can
defeat that skipping. [htmx events](https://htmx.org/events/) define the swap, settle,
history and cleanup lifecycle used by the patch.
