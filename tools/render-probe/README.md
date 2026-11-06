# Askama rendering compilation probe

This development-only crate implements the synthetic screening stage in
[`rendering-compilation.md`](../../docs/design/rendering-compilation.md). It is not
used by the forge, its production routes or release binaries. No application feature,
template, CSP or authorization policy changes when this package is added to the workspace.

It uses exact Askama 0.16.1 with derive/std requested and the already available
serde_json dependency for strict JSON reports. Record actual resolved dependency
features for the invocation: workspace feature unification can add Askama features if
other packages are compiled together. Do not treat a workspace-wide run as an isolated
probe-package compile measurement.

## Variants and generated fixture scope

Select one composition feature for compilation measurements:

- `inheritance` (A): every concrete context derives an inherited full document.
- `segments` (B): every concrete body derives only its body; two non-generic shared
  shell templates render before/after it into the same caller-owned buffer. Only
  explicitly approved compiled HTML body types implement the crate-private sealed
  component interface. No generic Template/Display/string implementation exists.
- `blocks` (C): every concrete context derives only `block = "content"` from the
  inherited template, then uses the same shell composition as B. This retains the
  inheritance parser path while measuring block-only generated rendering.

Select exactly one of `count-1`, `count-8`, `count-32` or `count-96`. Defaults are A
and eight contexts. Select all three composition features for output/escaping tests,
not isolated compilation comparisons. Build-script checks reject empty mode selection
and zero/multiple count selections. Always use `--no-default-features` for explicit
mode/count builds, so the default does not silently add A or count-8.

`build.rs` writes distinct concrete Template structs and their dispatch modules into
Cargo's OUT_DIR. These are explicitly benchmark-only synthetic fixtures, not generated
application business logic or a way to evade authored source limits. Each generated
type/file stays below 400 lines even at 96 contexts. A and C read the same inherited
template recipe; B uses the same source body without inheritance. All variants share
authored shell/body/macro sources. Writes preserve unchanged generated file contents
so a no-op build-script run does not deliberately touch all fixtures.

Recipe version 2 accounts for Askama 0.16.1 removing exactly one final LF when
loading each standalone template. Inherited assembly uses each segment's visible
source bytes rather than turning stripped final LFs into internal output. A final
non-output comment prevents stripping another LF from the assembled closing segment.
Copied standalone templates retain their original bytes, including CRLF or multiple
trailing newlines; the equality tests still require exact output across A/B/C.

There is no runtime template parsing or file reading. Rendering appends to the supplied
String; the CLI clears/reuses one buffer for all contexts. No pre-rendered body string,
`|safe`, blanket HtmlSafe marker or escaping-disabled template is used. Ordinary
untrusted fields are escaped by the concrete HTML-derived template that owns them.

The synthetic shell has a theme branch, site/viewer navigation, title, body, footer,
icons and a macro-rendered list, including empty results. It deliberately omits the
real application's assets, domain/database behavior, HTMX protocol, localization,
full theme/CSP handling and route graph. Passing its tests is not real-app parity or
evidence that the web compiler memory issue has been fixed.

## Commands through the shared build lane

Do not run bare Cargo in a worktree or create a per-worktree target directory. Pass
the following inner Cargo commands through the repository's shared build-lane helper
and let that helper set both target and intermediate storage. These examples describe
the child invocation, not a replacement for the helper. The root workspace must
register `tools/render-probe`; it is not a default member or production dependency.

```text
cargo test -p rbit-render-probe --no-default-features --features inheritance,segments,blocks,count-8
cargo clippy -p rbit-render-probe --all-targets --no-default-features --features inheritance,segments,blocks,count-8 -- -D warnings
cargo check -p rbit-render-probe --no-default-features --features inheritance,count-32
cargo build -p rbit-render-probe --release --no-default-features --features segments,count-32
```

The semantic suite compares exact HTML for every compiled case across A/B/C, including
empty rows, Unicode and malicious text/attribute values. It compares escaping with
ordinary Askama interpolation rather than assuming a particular HTML entity spelling.
Separate checks cover caller buffer reuse, append semantics, invalid case/mode errors,
CLI input rejection and strict JSON encoding. Buffer pointer/capacity checks describe
the output buffer; they are not a global allocator or intermediate-allocation profiler.

For execution, use the exact binary path handed off by the shared helper/Cargo JSON
artifact record for the selected source/features. A later build may overwrite a shared
path; copy approved experiment artifacts into bounded sample storage with source/input
metadata or hold the lane while sampling. Do not guess a target/debug binary path.

Example executable arguments:

```text
--mode segments --count 32 --iterations 1000 --warmup 10 --label "source-sha/profile/flags recorded externally" --emit
```

`--count` verifies the compiled feature; it cannot change how many derives were built.
An iteration renders every compiled case once, so rendered_documents is count times
iterations. Warmup rounds are excluded. Timing includes the render loop, buffer clearing,
byte bookkeeping and black-box observation. It excludes CLI parsing, JSON serialization
and optional sample rendering. `--emit` adds case-zero HTML inside the same JSON record,
never a second raw-HTML stdout stream. Errors are JSON on stderr with a failure status.

The report contains mode/count/iterations, bytes, elapsed nanoseconds, buffer capacity,
label, package/Askama versions, requested Askama features, compiled modes, target,
Cargo profile/optimization/debug settings and target features. It deliberately leaves
compiler memory and compile wall time null: an external process sampler must supply
those measurements. Record source/lockfile hashes, rustc -Vv, resolved dependency
features, build helper/lane settings, rustflags and executable hash separately; the
label alone is not proof of provenance. JSON is valid for labels containing quotes,
newlines or other text. Do not publish private paths/secrets in experiment metadata.

## Measurement protocol and adoption gate

Follow the design document's alternating serial A/B/C runs at counts 1, 8, 32 and 96.
Separate warm-dependency cold-package checks, normal builds, test compilation and
release builds. Record body-only, common-shell and macro edit rebuilds as separate
experiments. Do not use concurrent compilers or printing of generated expansions in
timed runs. External monitoring should distinguish per-process working set/private
bytes, simultaneous process-tree peak, linker memory and wall time. Preserve raw
samples, full child arguments and exit codes. Do not add non-simultaneous memory peaks.

The source-generation recipe is intentionally simple enough to inspect in OUT_DIR;
count concrete derives when checking that scaling is real. Both peak memory and any
static-layout duplication need observed data; neither is inferred from file counts.
Run runtime comparisons after builds stop, on the same machine with enough rounds to
report spread. This CLI emits elapsed totals, not p95/p99 or production latency claims.

Only proceed to a small real-page prototype if the synthetic result supports the
hypothesis. Full real-app acceptance still requires the document's no-JS, HTMX/history,
private-cache/authorization, theme/CSP/asset, locale/accessibility and runtime gates.
No variant becomes the default production renderer on the strength of this package.

For an external compiler experiment with shared storage, cold probe-package work,
balanced serial ordering and sampled owned-process memory, see
[the development compiler measurement runner](../../scripts/render-compile/README.md).
