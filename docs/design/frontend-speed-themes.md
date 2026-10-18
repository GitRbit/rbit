# Frontend wave — speed, themes, home

Contract for three parallel workstreams. House rules from [milestone-1.md](milestone-1.md)
and [../HANDOFF.md](../HANDOFF.md) §4 apply unchanged: thin handlers, no logic in templates,
strict CSP (no inline `style=` or scripts), clippy pedantic clean, atomic commits with
human attribution only (no AI coauthor trailers), mobile-first (375 px).

## Goals

1. **The fastest Git frontend there is.** Navigation should feel instant: HTMX boost,
   hover/touch preloading, partial swaps where only part of the page changes, no layout
   shift, no flash of the wrong theme, nothing render-blocking that doesn't need to be.
2. **A theme system** built on [tinted-theming](https://github.com/tinted-theming) schemes
   (base16 + base24): hundreds of themes, organised by family and variant, previewable
   live, persisted per user, and costing ~nothing at runtime.
3. **A better front page and auth screens**: a Gitea/Forgejo-style landing page in our
   monochrome language, no duplicated Sign in / Sign up (they live in the header), and
   containers with clear, even borders.

The default look stays **monochrome minimal** (rbit Light / rbit Dark / System). Tinted
themes are an opt-in a user picks; they recolour tokens, never layout or components.

## Workstreams and file ownership

Each workstream runs in its own worktree and commits on its own branch. Edit only the
files you own; anything else goes in your final report as an integration request.

### T — Themes

Owns:
- `scripts/themes.mjs` (new) and a `themes` script line in `package.json` (no new deps).
- `crates/rbit-web/assets/themes/**` (generated, committed — CI must not need network).
- `crates/rbit-web/assets/theme.js`, a new `assets/theme-picker.js` module, and the
  theme section of `assets/app.js` (the `THEME_KEY` / `toggleTheme` code).
- `crates/rbit-web/styles/app.css`: the **Tokens** section, the avatar tones, the
  scheme-dependent visibility section, and a new **Themes** section at the end.
- New theme modules in `crates/rbit-web/src/` (catalog, cookie, routes, views),
  `templates/settings/appearance.html` and any new theme templates.
- `src/views/layout.rs` (`Page` gains the active theme), `src/routes/settings.rs` +
  settings nav (adding the Appearance entry), `src/routes/auth.rs` (only to set the theme
  cookie after sign-in).
- In `templates/base.html`: the `<html>` tag, the theme `<link>`/`theme-color` metas, and
  the header theme control.
- Migration `0009_preferences.sql` and the storage/service code for the stored preference.

### S — Speed

Owns:
- `package.json` dependencies / `package-lock.json`, `scripts/vendor.mjs`,
  `crates/rbit-web/assets/vendor/**` (e.g. the htmx preload extension).
- In `templates/base.html`: the `htmx-config` meta, script tags, `<body>` attributes and the
  progress bar element. A new `assets/nav.js` module (imported from `app.js`) for
  navigation behaviour; append-only edits to `app.js` otherwise.
- `app.css` **htmx progress bar** section and any new navigation/transition section.
- `assets/sw.js`, `src/routes/pwa.rs`, `src/assets.rs`, `src/middleware.rs`,
  `src/routes/mod.rs` layers, `templates/repo/**` (navigation attributes and partial-swap
  targets), and server-side hot paths in `src/routes/{repo,history,blame,compare}.rs`.

### H — Home and auth

Owns:
- `templates/home/**`, `templates/auth/**`, `templates/recovery/**`, `templates/error.html`,
  `templates/offline.html`, the footer in `templates/base.html`.
- `src/views/home.rs`, `src/routes/home.rs`, `src/views/auth.rs`, `src/views/recovery.rs`,
  and read-only counting queries in services/storage needed for instance stats.
- `app.css` **Components** section (container, card and auth-card styles).
- `templates/macros.html`: add new macros only; never change an existing macro signature.

## Contracts between workstreams

- **Theme attribute.** The active theme is `<html data-theme="SLUG">`. `light` and `dark` are
  the built-in rbit monochrome themes; no attribute means System (follows the OS via
  `light-dark()`). Tinted slugs are `base16-<slug>` / `base24-<slug>`, `[a-z0-9-]+` only.
- **Tokens are the API.** Components use only the semantic tokens (`--canvas`, `--surface`,
  `--subtle`, `--hover`, `--line`, `--line-strong`, `--ink*`, `--inverted*`, status and
  `--syntax-*`). T may add tokens; H and S must not hard-code colours. Two levels of
  elevation exist: page `--canvas` and container `--surface`, separated by `--line`, which
  must be clearly visible in every theme.
- **Theme selectors are scoped, not root-only:** `[data-theme="SLUG"] { --canvas: … }`, so a
  preview card can carry `data-theme` and render in that theme without touching `<html>`.
- **No flash, no blocking JS.** The server reads the `rbit-theme` cookie and renders the
  attribute and the single theme stylesheet into the HTML. JS only makes switching instant.
- **Navigation.** S decides boost/preload/swap behaviour globally (on `<body>`), so other
  workstreams write plain `<a href>` links and forms. Links that must not be boosted or
  preloaded (downloads, raw files, `/logout`, anything with side effects) carry
  `hx-boost="false"`; S audits these.
- **History cache.** Pages that show a secret once (new tokens, recovery codes, TOTP setup)
  must never land in htmx's history cache; S owns the mechanism.

## Verification (each workstream)

`cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace`, `npm run build` with a clean `git diff -- crates/rbit-web/assets`
afterwards, then exercise your pages in a browser at 375 px and desktop, light and dark.
