# Rendering compilation: evidence and experiment contract

Status: synthetic probes are implemented and screened; no production rendering migration.
The [runtime observations](rewrite/measurements/render-runtime-count8.md) and
[72-sample compiler screening](rewrite/measurements/render-check-counts.md) record the
executed experiments. Compiler timing passed its validity checks; the initial memory
observations are rejected because crate arguments cannot distinguish a rustup proxy
from the actual compiler. Real-page prototypes and real-web acceptance remain open.

The original source review below did not execute compiler/expansion measurements. Its inspected
source is the rewrite-localization branch through `6c44582`, based on GitHub main
`39f932a00b7aa35b324e06a9ff2343a57d4bed76`. Record the integrated revision when executing.

## The question to answer

The integrator reports roughly 9–11 GiB for a rustc process compiling the web library
or its tests, and roughly 7 GiB during an all-target check. These observations motivate
investigation; this review did not independently sample them or establish their cause.
Preserve the exact compiler command, target kind, features, profile, source revision,
measurement field and process timeline before using those numbers as a baseline.
Working set, private bytes and simultaneous process-tree totals are different metrics.

Hypothesis: repeated expansion of the common Askama shell and frequently called
macros contributes substantially to compiler syntax/type-checking/code-generation
work. The competing explanations include the overall web source/test size, large
route types, embedded assets, dependencies, debug information, linking and other
proc macros. A successful experiment must isolate template expansion before assigning
all observed memory to Askama or claiming that splitting crates fixes it.

## Source evidence

An inventory of `crates/rbit-web` at the inspected revision found:

| Item | Count or size |
| --- | --- |
| HTML template files | 179 |
| Physical template lines, including comments/blanks | 8,438 |
| Files containing an inheritance directive | 111 |
| Direct `base.html` inheritance directives | 86 |
| Other inheritance targets | admin/layout: 13; settings/layout: 9; webhooks/layout: 3 |
| Syntactically matched Template derive + template attribute pairs | 125, including two test-only base fixtures |
| Imports of `macros.html` | 143 |
| `templates/base.html` | 175 physical lines |
| `templates/macros.html` | 256 physical lines |

The admin/settings/webhook layouts themselves extend the base. These are source
counts, not generated-code size or a count of independently compiled full documents.
An import alone does not mean every macro body is emitted. Macro calls, branch
specialization and compiler optimization must be examined in actual expanded output.

`base.html` contains the common head/assets, theme/style decisions, navigation/user
menus, footer/update notice and main markers. Customization points include
`theme_script`, `nav_context`, `subnav` and `content`. Error/offline templates override
theme-script loading; repository pages supply repository navigation/tabs. A prototype
limited to content would miss real shell behavior.

The lockfile and installed source both identify Askama and its derive/parser/macros
packages as 0.16.1. Exact installed source inspection found:

- `askama_derive/src/generator/node.rs::impl_template_inner` traverses inherited root
  nodes in the derived renderer; `handle_include` feeds child nodes into its output
  buffer. Macro invocation generation likewise processes the macro body's nodes.
- `askama_derive/src/generator.rs::impl_template` emits a render implementation for
  each context. `integration.rs::impl_everything` adds Display and FastWritable.
- Template-file dependency markers are emitted, so editing a shared template can
  invalidate many dependent derives even when runtime output barely changes.

This supports the duplication hypothesis at generated-source level, not a measured
memory percentage. Inspect the exact [derive source](https://docs.rs/crate/askama_derive/0.16.1/source/src/generator/node.rs)
when reproducing it. Askama offers block-specific renderers and generated-code printing;
these are tools to measure, not assurances that inheritance is compiled once.
[Template creation options](https://askama.rs/en/stable/creating_templates.html).

The installed runtime exposes `Template`, `FastWritable` and the object-safe
`DynTemplate`. A derived HTML template is not automatically marked HtmlSafe by the
inspected integration generator. Consequently, interpolating a nested template as
an ordinary value can escape its generated tags; a blanket safe wrapper would create
a new trust boundary. `Template` also has an associated size constant and is not the
trait-object interface. Use the documented rendering APIs and measure dispatch costs.
[Template 0.16.1](https://docs.rs/askama/0.16.1/askama/trait.Template.html),
[DynTemplate 0.16.1](https://docs.rs/askama/0.16.1/askama/trait.DynTemplate.html).

## Candidates to compare

| Variant | Composition | Expected benefit and risk to test |
| --- | --- | --- |
| A | Current inheritance/includes/macros | Control; retain identical data, features and profiles |
| B | Fixed non-generic compiled shell segments plus approved typed components rendered into the same buffer | Potentially removes repeated shell expansion; adds orchestration/slot calls and maintenance obligations |
| C | Askama `block = "content"`/dedicated component templates, with shared shell segments | Tests whether existing block extraction avoids duplicating content definitions; extra block-generated implementations may themselves cost compilation |
| D, optional | One compiled fixed shell with tightly sealed typed slots | May simplify layout ownership, but only after proving safe composition without an arbitrary-HTML escape hatch |

B is the first prototype because it can avoid introducing any new HTML-safe value.
Compile shell segments once as concrete, non-generic Askama template structs. Render
their output, approved navigation/subnav/body templates and the closing shell directly
into one `String`/writer in the correct order. Static literal tags are authored in
Askama; every user value is still escaped inside its owning HTML template. HTML
segments may span element boundaries; final-output parser/DOM tests validate nesting.

For the initial Explore case, use opening head/navigation/main and closing main/footer
segments around its compiled body. Then add the real nav-context/theme/subnav slots
before assessing repository or offline/error parity. Do not fork a second hardcoded
copy of the shell: maintain an explicit segment manifest and compare its reconstruction
against the control shell. A prototype generator may mechanically split the control
fixture; production would need a reviewed single ownership scheme.

The component slot interface should be private/sealed and explicitly implemented only
for approved HTML-derived component types. The host takes a small borrowed trait
object or a closed enum, not `String`, arbitrary Display, a public raw-HTML constructor
or a blanket implementation for every Template (which would also admit text templates).
Its method calls the concrete component's Askama render-into API with the same writer
and values. It does not interpolate an already-rendered string into a template.

Sketch of the boundary, not compiled production code:

```rust
// Only this module can implement ApprovedHtml; bodies retain ordinary HTML escaping.
trait ApprovedHtml: sealed::ApprovedHtml {
    fn render_into(&self, out: &mut dyn std::fmt::Write,
                   values: &dyn askama::Values) -> askama::Result<()>;
}

fn render_document(out: &mut dyn std::fmt::Write, page: &Page,
                   body: &dyn ApprovedHtml) -> askama::Result<()> {
    ShellOpen { page }.render_into(out)?;
    body.render_into(out, askama::NO_VALUES)?;
    ShellClose { page }.render_into(out)
}
```

The final prototype must handle navigation/theme slots and error propagation explicitly;
the sketch omits them for clarity. Reserve the final output buffer using measured shell
and body size hints. Avoid intermediate body buffers and String concatenation in B.
Keep the shell type non-generic over every body, so moving it merely into a generic
`Layout<T>` does not reproduce the same specialization problem. LTO may inline across
boundaries later: inspect generated code and release binary size instead of assuming
that the separation survives every compiler phase.

Do not use `|safe`, `escape = "none"`, new HtmlSafe implementations or generic
`Html(String)` wrappers to make D work. Existing reviewed embedded `InlineCss` and
sanitized-document boundaries remain narrow existing mechanisms; this experiment
must not broaden them or mark LocalizedText as trusted markup. If a one-template shell
cannot satisfy this restriction simply, drop D and measure B/C instead.

## A small reproducible prototype

1. Pin the integrated source/lockfile, rustc/Cargo version, target, features, asset
   hashes, profiles, job counts and environment. Use the nested worktree/shared-build
   helper. Keep the production tree unchanged; serialize compilation in a bounded
   disposable experiment lane while other measurements are stopped. Never create a
   target tree per worktree or clean the shared production lane.
2. Start with an independent probe depending on exact Askama 0.16.1, using its current
   feature set and the same simplified Page model in A/B/C. Use fixed shell data and
   deterministic bodies; no database, embedded asset derive, Axum route graph or other
   proc macros yet. This isolates the proposed mechanism.
3. Generate 1, 8, 32 and 96 distinct small body context types/files from one recorded
   fixture recipe. A extends the same realistic shell each time; B compiles fixed
   shell segments once and each body separately; C extracts named body blocks. The
   recipe changes composition only, not escaped values, loops, labels or output.
   Include enough calls to the actual icon/row macros to expose caller expansion.
   Keep each authored/generated probe file under the 400-line rule; these are
   experiment fixtures, never generated production business logic.
4. Capture expansion for selected types with Askama `print = "code"` in a separate
   diagnostic run. Save generated bytes/tokens and shell-body occurrence counts.
   Do not include verbose expansion I/O in timed/memory runs. Capture the entire probe
   with compiler expansion tooling only if available and recorded; do not upgrade the
   project's toolchain merely to obtain an expansion command.
5. Compare cold probe-package checks with dependency artifacts already warm, normal
   dev builds, test compilation (`--no-run`) and release builds. Treat check/typecheck,
   codegen, lib-test compilation and linking as distinct phases. Fix build jobs to one
   for isolation, then repeat the winning candidate at supported job limits. Record
   incremental clean/no-op, body-only edit, common-shell edit and macro edit rebuilds.
6. After screening, port only the prototype forms of Explore, login, repository tree,
   one Actions page, one admin/settings page and error/offline. Reuse real Page/assets
   and current data fixtures. Keep each original route available in the probe for
   output and runtime comparison; do not migrate the platform to obtain first evidence.
7. Repeat the promising variant in an isolated real-web build with identical dependency
   graph before any rollout. Synthetic scaling alone cannot explain the real 9–11 GiB
   observation. Include lib, lib-test and integration-target compiler invocations.

Run variants serially in alternating order, at least three cold-package repetitions,
with no unrelated compiler activity. Use only helper-managed experiment cleanup after
checking the resolved lane path. Preserve dependency warmth consistently; report cold
full-dependency builds separately. One shared compatible dependency cache must not make
the later variant appear artificially faster. Record wall/user/system time, per-rustc
peak resident/private bytes, simultaneous process-tree peak, page faults, link peak,
output/intermediate bytes and invalidated artifacts. Save raw timestamped process
samples and exit codes; totals from non-simultaneous peaks must not be added together.

For runtime, prebuild all variants, stop compilation and alternate render samples on
the same fixture. Measure output-buffer allocations/capacity growth, CPU/render,
bytes, warm/cold routes and p50/p95/p99 at stated offered load. Use enough sustained
samples for tails, not a handful of serial requests. Compare same-writer static calls
and sealed dynamic dispatch separately; the official docs flag dynamic-rendering
tradeoffs, but do not predict this application's measured cost.

## Correctness and acceptance

Require the same accessible DOM, title, classes, attributes, URL/query values and
escaping on all fixture pages, allowing only documented insignificant whitespace.
Test untrusted `<>&"'`, marker-shaped text, Unicode/bidi and long identifiers in text
and attributes; ensure each value is escaped exactly once and cannot become a slot.
Keep selected attribute values typed/validated. Test template/writer errors: finish
rendering into the buffer before publishing a 200 response so a late failure cannot
leak an incomplete successful document. No new streaming HTTP lifecycle is implied.

Preserve full-page GET/no-JS forms, HTMX search fragments, boosted navigation,
history-restoration full responses, response Vary/privacy/validators, preload side-effect
guards and authorization before private output/304s. Body composition never fetches
domain data or caches permissions. Preserve repo-tab out-of-band behavior and existing
partial markers until that protocol is deliberately migrated.

Run system/light/dark/custom themes, anonymous and signed-in/admin navigation,
notifications/update notices, mobile/desktop layouts and opt-in expansion/RTL Explore.
Test inline CSS bytes and exact CSP hashes with `assets::with_head_styles` both true
and false; preserve hashed asset URLs and task-local style scope across every component.
Keep offline/error synchronous theme-script behavior and anonymous service-worker
caching. Reuse `views/layout.rs` and middleware security regressions rather than
substituting a visually similar unauthenticated page.

Provisional go/no-go: a representative real-web build should reduce peak rustc memory
or common-shell edit rebuild time by at least 20% across repeated runs, without a
statistically meaningful (>5% beyond measured noise) runtime CPU/latency regression,
additional per-body buffering, output growth or correctness/security loss. Record
absolute results too. If only synthetic expansion improves, report it and stop before
claiming the production memory problem is fixed. If B/C fail runtime or maintenance
gates, retain current rendering and investigate other compiler hot spots.

Crate boundaries may improve invalidation and permit lower-memory compilation units,
but parallel units can also increase total memory. Evaluate them after measuring the
template mechanism. A crate split is not evidence of less generated code, lower
monomorphization cost or a smaller final binary.
