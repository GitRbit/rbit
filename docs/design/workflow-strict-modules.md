# Workflow module boundaries and strict source checks

`rbit-workflow` is selected as a complete strict crate by the shared rewrite policy.
Its historical file-size ceilings no longer apply. The native structural checker
receives the entire crate rather than a list of selected types, so definitions and
all implementations of each logical type must total at most 400 physical lines.
The physical source check independently applies the same limit to every authored
file in the crate. This changes the enforcement boundary; it does not certify
runtime correctness without the Rust gates below.

## Responsibilities

| Boundary | Responsibility |
| --- | --- |
| `parse/` | Workflow syntax, field expression scopes, triggers, jobs, calls, steps, matrix and dependency validation |
| `plan/` | Planning contexts, matrix expansion, reusable calls, deferred inputs and resulting job templates |
| `expr/` | Syntax and parsing, context lookup, value coercion, evaluation, builtins and expression templates |
| `model/` | Workflow records, event filters, reusable-call declarations and job declarations |
| `protocol/` | Runner/task envelopes, endpoint rules, limits, validation and refusal contracts |
| `spec/` | Stored job templates, executable task specifications, permission rules and input validation |
| `tests/conformance/` | Case loading, observed behavior, verdict comparison and reporting |

Pattern, builtin, trigger and YAML acceptance fixtures live beside their respective
modules. Integration and property tests retain their original targets. Public
module paths and reexports stay compatible; internal imports name the required
responsibility rather than sharing an implicit helper registry. No dependencies,
execution queues, runtime indirection or additional allocations are introduced.

## Preservation evidence

The extraction was compared against original UTF-8 Git objects. Existing function,
constant and type bodies were compared as Rust tokens, ignoring only moved module
visibility, comments, whitespace and formatting-only trailing commas. Expression,
model, protocol and specification extractions preserve respectively 43, 38, 90
and 54 production items. The conformance split preserves all 24 original items
and their literal spellings and multiplicities. Four explicit module path
attributes point the integration test at its new child files.

Golden fixture files are unchanged. Two moved specification-test includes use a
new relative path resolving to the same files; planner fixture resolution was
checked separately against the same original bytes. Conformance case discovery
still accepts only JSON files, so adjacent Rust helper modules are not cases.
Limits, evaluation order, diagnostic strings, serialization fields, redaction,
known-failure handling and error precedence remain the existing contracts.

At this source checkpoint all 100 Rust files are at most 384 lines. The source
ratchet passes, and the focused policy fixtures prove that a 401-line former
workflow exception is rejected while unrelated legacy ceilings remain intact.
These counts describe source checks, not a latency or memory improvement.

## Required native gates

Rust compilation and cumulative structural checking remain pending for this
checkpoint. Under the owned shared compiler lane, run the normal structural
checker wrapper, the complete `rbit-workflow` test suite (including conformance,
golden, reusable, deferred, matrix and property targets), and strict all-target
Clippy. Retain every original test and compare diagnostics on failure. No golden
blessing or conformance known-failure edits are part of this change.
