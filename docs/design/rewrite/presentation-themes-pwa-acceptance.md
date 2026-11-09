# Presentation themes and installable app acceptance

Acceptance evidence for the `presentation.themes-pwa` ledger row ("Themes, installable PWA,
mobile navigation and offline privacy"). The crate and its new edge are explained in the
[repository documents record](repositories-documents-acceptance.md#why-a-new-crate); this
record also holds the shared results and cost of the three presentation rows.

## Scope

| Decision | Legacy place | Now |
| --- | --- | --- |
| Built-in themes, the generated tinted catalog (498 schemes), slug parsing, names, variants, `data-theme`/`data-scheme`, canvas colours, light/dark toggle, stylesheet paths, cookie names and lifetime | `rbit-web/src/themes/{mod.rs, catalog.rs}` | `rbit_presentation::themes`; `scripts/themes.mjs` now generates `crates/rbit-presentation/src/themes/catalog.rs` |
| Which cookie wins (current, then pre-rename legacy, else System) | `Theme::from_headers` | `Theme::from_cookies`; `rbit-web` reads the jar (`ThemeHttp::from_headers`) |
| Manifest launch colour (URL theme, else the visitor's, else DNA day) | `routes/pwa.rs::launch_color` | `pwa::launch_color`, asking for the cookie theme lazily as before |
| Web manifest document | `routes/pwa.rs::manifest` | `pwa::manifest`; `rbit-web` supplies settings and hashed icon URLs |
| Service worker version (asset build + 12 hex digits of the site identity digest) and placeholder substitution | `routes/pwa.rs::{service_worker, worker_response}` | `pwa::{worker_version, worker_source}` |
| Branding stylesheet text and its cache policy (immutable only for the current colours) | `routes/pwa.rs::branding_styles` | `pwa::branding_sheet` |

`rbit-web` keeps: routes, content types and response headers (`Vary: Cookie`,
`Service-Worker-Allowed`), the `ThemeHttp` trait (cookie jar, `Set-Cookie` attributes,
hashed stylesheet URL), the branding validation (an `rbit-core` rule), the offline page
handler (rendered with no account, so the worker's cached copy never carries anyone's
navigation), templates, `sw.js`, CSS and the inline/deferred stylesheet split in
`assets.rs`/`base.html`, which still inlines `Theme::stylesheet_path()`. Templates call
only the moved inherent methods, so no template changed.

## Legacy code removed

`rbit-web/src/themes/catalog.rs` (moved), the `Theme`, `TintedTheme`, `Variant` and `System`
definitions and every theme method except the three HTTP ones; the JSON manifest, SHA-256
identity digest, `__VERSION__` substitution and branding CSS formatting in `routes/pwa.rs`
(which no longer imports `sha2` or `serde_json`).

## Boundary that fails if legacy paths return

In `scripts/rewrite/presentation-retirement.mjs`: `crates/rbit-web/src/themes/catalog.rs`
existing at all; `struct TintedTheme`, `enum Theme`, `static THEMES` or `fn toggled` in
`rbit-web`; and in `rbit-web` a `"start_url":` manifest key, a `.replace("__VERSION__"`
or a `.btn-primary{` rule. The catalog path also moved in the checker's generated-file
list.

## Gates

| Gate | Evidence |
| --- | --- |
| Behaviour parity | Code moved verbatim. The whole theme suite (`rbit-web/src/themes/tests.rs`: catalog shape, counterparts, toggle, colorblind pairs, contrast, stylesheet per theme, canvas colours against `app.css`, cookie parsing including the legacy and percent-encoded cases, cookie attributes, page attributes) runs unchanged through the re-export. `routes/pwa.rs` tests (launch colours, worker versioning and scope, icon sizes) are unchanged. New `rbit-presentation` tests cover the manifest icons, worker version shape and substitution, and the branding cache decision. |
| Service worker and cache versioning | The version string is built from the same inputs with the same digest and prefix length; `sw.js` is unchanged; the worker is served with the same headers. |
| CSP and inline styles | No template, CSS or asset changed, so the inline-style hashes are unchanged; the branding sheet is still a same-origin stylesheet with the same text. |
| Localisation | No catalog or message changed; theme names were already untranslated identifiers. |
| Migration and rollback | None; stored theme preferences and the cookie format are unchanged. Rollback is the previous binary. |
| Performance | Same work; the catalog is the same static data in another crate. The manifest builds the same `serde_json` value (same features, same key order). |

## Results

Verified on the Linux check (format, Clippy with `-D warnings` on all targets, the full
Linux test suite, the rewrite contracts including the strict AST checker on the new crate,
the front-end build and cargo-deny): Linux check run 204 passed all five jobs on the
slice's final code. Its first run failed one Clippy lint (`assert_is_empty` in the
document tests, now `assert_eq!` against an empty list) and lacked the base type-check fix for
`AccessTokenStore`, which the branch now carries. Locally: `node scripts/rewrite/check.mjs` reports 0 violations and
`node --test scripts/tests/presentation-retirement.test.mjs` passes. No Windows build or
test was run for this slice.

## Not covered

- Askama templates and CSS (mobile navigation, the theme picker, the offline page) stay in
  `rbit-web`. The [rewrite plan](../ground-up-rewrite-plan.md#proposed-crate-responsibilities) gives `rbit-presentation` "shared Askama components"; moving them
  is a template-wide change with its CSP and inline-style hashes, not part of this row.
- Browser scripts (`sw.js`, `theme.js`, `theme-picker.js`) stay in `rbit-web`, which the
  same table makes the owner of "browser assets".
- No release performance measurement was taken.

## Cost

Across the three rows: 5 code commits plus this record; about 45 files touched. New
`rbit-presentation` source is about 1,100 authored lines (of which about 600 moved
verbatim, and the 511-line generated catalog is excluded), new tests about 240 lines;
`rbit-web` lost about 1,000 lines. Checker: a 50-line Node guard, a 60-line test, two
lines in `check.mjs`, one `ADDED_EDGES` entry and one generated-path change.

Tricky parts: the sanitiser's per-thread document had to stay `'static`, so the port lends
an owned copy (`LinkResolver::boxed`) rather than a borrow; `Theme`'s HTTP methods could not
stay inherent on a foreign type, so they became the `ThemeHttp` trait with one import per
call site; and the existing suites were kept in `rbit-web` wherever they need real assets,
cookies or `DocumentBase`, moving only tests of private state or pure cuts.

Estimate for similar rows: a presentation decision with no storage is one focused session
when its legacy file is self-contained; most of the effort is keeping existing suites
running unchanged across the new boundary.
