# Baseline (partial) — 2026-10-17

Debug build (`target/perf-build`) of the working tree at `d0ad7bb`, including the user's
uncommitted template, CSS and JS changes. Fixture: `main` 9f169c2, `big-change` f1cf015
(152 files, +15,000 −10,219). Server timings are debug-build and only relative to each
other; the machine was saturated by other `cargo` builds (Windows reported an exhausted
paging file), so treat every millisecond below as noisy.

## Static page size (curl, signed out)

| Page | HTML | Elements (tags in HTML) | Lines / rows | Server render (min of 3) | Caps hit |
|---|---|---|---|---|---|
| blob `src/huge.rs` | 17.3 MiB | 356,040 | 20,000 `.code-line` | 2.5 s idle, 5.6-18 s under load | none (507 KB < 512 KiB highlight limit) |
| blame `src/huge.rs` | 17.6 MiB | 358,581 | 20,000 `.code-line`, 319 blame runs | 2.8 s idle, 6.7-27.9 s under load | none (< 512 KiB blame limit) |
| compare `main...big-change` | 5.4 MiB | 106,062 | 20,180 `<tr>` in 152 `.diff-file` | 0.30 s idle, 0.6-1.1 s under load | `max_total_lines` 20,000: 33 files "Diff too large to display" + "This change is very large" notice |
| PR #1 files | 6.1 MiB | 102,233 | 20,180 `<tr>` in 152 `.diff-file` | 0.53 s idle, 1.2-2.3 s under load | same as compare |

Elements on a highlighted line: ~17 (the blob page is 89% syntax-highlight spans).

## Scroll probe

Not recorded validly. In the Claude desktop Browser pane the stylesheets failed to load
(`net::ERR_FAILED` for `app.css` and the theme CSS; scripts and images loaded), so the
pages were unstyled — no sticky header, no backdrop blur, no `content-visibility` — and the
pane was hidden part of the time, which stops requestAnimationFrame. One run on the
unstyled blob page, for the record only:

| Page | DOM elements | FCP | frames > 16.7 ms | > 33 ms | worst | blank samples | CLS |
|---|---|---|---|---|---|---|---|
| blob (unstyled, invalid) | 356,095 | 9.3 s | 86 / 87 | 86 / 87 | 283 ms | 0 / 264 sweep, 0 / 1,728 jumps | 0 |

Every sweep frame took 130-170 ms, all of it inside the probe's own `scrollTop = …` call
(long-animation-frame: script 127 ms, forced layout 0, unchanged with `content-visibility`,
scroll anchoring or backdrop-filter switched off). This looks like the cost of a
programmatic scroll on a 356k-element document in that environment, not what a wheel scroll
costs; verify with the Performance panel before relying on the sweep.

To record the real baseline: an idle machine, a normal Chrome or Edge window at a fixed
size, the steps in README.md, both `noBackdrop` settings per page.
