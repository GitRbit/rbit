# Representative rendering correctness probe

This module is compiled only with `cfg(test)`. It adds no production route,
feature toggle, template parser, dependency or alternative production renderer.
The original `ExplorePage` and `LoginPage` remain the inherited references.

Three composition paths render the real view models:

- A calls the existing inherited full-page derive.
- B renders fixed shared shell segments and independently compiled feature bodies.
- C renders the same shell with content-block derives from the original templates.

The shell function is not generic over bodies. Its private sealed slot accepts only
four explicitly approved HTML template contexts, and every part writes into the
same caller-owned buffer. Ordinary strings never implement the slot or HTML safety.
The copied shell retains the existing reviewed embedded `InlineCss` use exactly;
no new `safe` expression or escaping override is introduced. All templates are
compiled by Askama; no request parses or rewrites their source.

Probe-only template snapshots make the composition reviewable. Exact source guards
reconstruct the original base and both child sources. Comment sentinels preserve
boundary whitespace, including Askama0.16.1's removal of one terminal LF per source.
All files use the repository's canonical LF policy. A changed production template
must fail the guard until its probe snapshot is deliberately reviewed and updated.
The probe currently covers default theme-script/navigation/subnav blocks only;
repository tabs and error/offline overrides remain later representative cases.

Tests demand byte equality with A outside request timing. They cover real `Page`,
`NavUser`, typed Explore catalogs/results, login form errors/provider buttons,
anonymous/member/admin navigation, classic/DNA/tinted themes, diagnostic Explore
locales, empty results and current direct results fragments. Malicious text/attribute
values must match Askama's normal escaping exactly once. Login stays English.

A test router wraps each renderer with the real security headers, task-local head
styles, partial-response, HTML caching and timing middleware. It checks hashes of
actual emitted CSS against CSP, omitted inline styles for HTMX, private cache policy,
Vary, explicit validator presence, real conditional GETs returning empty 304 bodies
with retained CSP/validators, full fallback for non-repository partial requests and the
real timing span. Only that already-dynamic span is excluded in timed HTTP byte
comparisons. The view fixtures choose authenticated navigation data directly; this
router does not authenticate cookies, resolve account preferences, query storage or
replace the existing production HTTP authorization/locale acceptance tests.
Additional selected-stack fixtures require no validators for explicit `no-store`,
`Set-Cookie`, non-200 and POST responses, even with wildcard `If-None-Match`.
These use artificial response policies at `/probe`; they do not exercise production
login/session routes, preload protection, CSRF/rate/error middleware, HSTS/security
configuration variants or account-derived theme/locale resolution. Repository-partial
success/history and browser no-JS/HTMX behavior remain separate acceptance gates.

There is no visual browser review, runtime performance measurement or isolated
compiler comparison in this preparation. Compiling all candidates into one lib-test
binary cannot establish a compiler-memory improvement or a production winner.
Future helper-controlled measurements must isolate variants, record real compiler
identity, and compare representative real-web compilation and rendering under
`docs/design/rendering-compilation.md`. The earlier synthetic72-sample report is
screening evidence only; its proxy-based memory flags remain unaccepted.

After assets are built, validate through the shared helper when its lane is free:

```sh
node scripts/workspace.mjs test --package rbit-web --lib views::render_probe:: --locked
```

The integrated probe passes seven shared-lane tests, including source reconstruction,
byte equivalence, escaping, real CSS/CSP behavior and conditional 304 responses.
Strict candidate-wide Clippy and isolated real-web performance comparisons remain
separate gates; these correctness results do not select a production renderer.
