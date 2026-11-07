# Default-off real-page shell experiment

The `rbit-web/shared-shell-experiment` feature selects a production-build candidate
for the actual `ExplorePage` and `LoginPage`. No feature is enabled by default.
Their routes, constructors, view data, `WebTemplate` responses, fragments and
middleware stay the same. An enabled non-test build replaces those two inherited
derives with one shared compiled prefix and suffix and two compiled borrowed bodies.
Other pages still use their original renderers. This is an experiment, not adoption.

All three pieces write into one output String; no body HTML String, dynamic body
registry, parsing, escaping override, new dependency or template engine is introduced.
The manual `Template` implementation preserves writer, `Values`, Display,
`FastWritable` and I/O entry points. Its allocation policy matches pinned Askama's
default `render_with_values`. Askama 0.16.1's own generated renderer accepts
`&mut dyn fmt::Write` for every template, including the baseline. The concrete outer
buffer does not establish per-literal static dispatch. Compiler devirtualization is
a measurement hypothesis; this prototype adds no dynamic body-dispatch layer.

The existing reviewed `render_probe` snapshots are now also the experimental source.
Their sentinel comments preserve Askama's terminal-newline behavior. Source guards
must reject drift from `base.html` and the original content blocks. Only these two
pages' default theme-script/navigation/subnav blocks are supported; no repository,
error or offline block override is migrated.

With the feature selected, the existing real-page protocol probes add the actual
production candidate as a fourth renderer and retain independent inherited template
references in tests only. They cover malicious text/attributes, locales, anonymous/
member/admin navigation, themes, HTMX style omission, CSP, validators, conditional
304, history-compatible non-repository fallback, invalid forms and no-store policies.
The added entry-point test checks all public render paths and writer failure.
No request timing is fabricated: the shared suffix evaluates the original footer
at its original template position, once per render. Timed byte comparisons exclude
only that existing variable span. The direct Explore fragment is unchanged.

## Required validation matrix

Preparation ran Node source guards and formatting only. Rust compilation, tests,
runtime measurements and browser review are pending the shared compiler lane.
Run both variants at the same integrated revision, toolchain, assets and profile:

```sh
node --test scripts/tests/shared-shell-experiment.test.mjs
node scripts/workspace.mjs test --package rbit-web --lib views::render_probe:: --locked
node scripts/workspace.mjs test --package rbit-web --lib views::render_probe:: --features shared-shell-experiment --locked
node scripts/workspace.mjs test --package rbit-web --locked
node scripts/workspace.mjs test --package rbit-web --features shared-shell-experiment --locked
node scripts/workspace.mjs clippy --package rbit-web --all-targets --features shared-shell-experiment --locked -- -D warnings
node scripts/workspace.mjs build --package rbit-server --release --locked
node scripts/workspace.mjs build --package rbit-server --release --features rbit-web/shared-shell-experiment --locked
```

Keep exact immutable helper-produced binaries for baseline and candidate, then run
the existing isolated performance fixture against both. Measure actual Explore
English/diagnostic locales and login, full/HTMX/history navigation, empty/nonempty
results, errors/providers and authenticated navigation. Include ordinary forms with
JavaScript disabled. Compare exact HTML outside the footer interval and all relevant
headers, fresh/304 footer updates, CSP and escaping before throughput acceptance.
Existing integration route tests must run in both variants; probe routers alone do
not establish authentication, CSRF or account preference behavior.

For compiler comparison, record real rustc identity, features, target kind, cache
state and profile; keep dependencies matched, measure repeated cold web compilations
and warm edits separately, and report peak process memory and wall time without
including the rustup proxy as rustc. A two-page change may be too small to resolve
against whole-web variance. Do not infer a win from compiling all candidates into
one test binary. Record binary/cache footprint and matched runtime latency/throughput
before considering expansion. Keep inherited rendering as default until accepted.
