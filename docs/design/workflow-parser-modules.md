# Workflow parser module boundary

This is a responsibility extraction of the existing parser, not a language or
execution-policy change. The public `parse` entry point remains in `parse.rs`,
with its original byte/UTF-8/YAML/deadline order. Existing module paths for both
limits, `call_input_value`, `call_target`, `parse_env`, `parse_steps` and
`valid_job_id` remain available with their original visibility.

| Module | Responsibility |
| --- | --- |
| `parse.rs` | Public composition and existing reusable-target syntax entry |
| `syntax` | Located scalar shapes, optional text and unknown-key diagnostics |
| `expressions` | Context/function scopes and expression-bearing scalar validation |
| `environment` | Environment maps, defaults, shell and registry credential fields |
| `triggers` | Event forms, activity/ref/path filters and schedules |
| `dispatch` | Bounded manual input definitions and defaults |
| `permissions` | Permissions and scoped concurrency |
| `jobs` | Job identity, job/deadline bounds and ordinary job composition |
| `calls` | Reusable workflow calls and called-workflow inputs/outputs/secrets |
| `runtime` | Runner selection, deployment environments, containers and services |
| `strategy` | Matrix shape and strategy-scoped expressions |
| `steps` | Step limits, builtin/action syntax and runner-scoped expressions |
| `dependencies` | Dependency existence and cycle validation |

Sibling modules explicitly import the functions/types they use. No context owner,
helper registry, trait dispatch, dependency, expression evaluation or allocation
policy is introduced. Function ordering within a source file does not select the
parser's evaluation order; all original call bodies and iteration orders remain.

The inline tests move into bounded workflow, diagnostics, steps, expressions and
property-test modules, with explicit shared fixture imports. The original 12 unit
cases and two 512-case property tests remain. Their assertions still cover located
errors, unknown/unsupported keys, deadline/resource limits, builtin/third-party
steps, scalar amounts, expression contexts and functions, dependency cycles,
complete typed fields and job-ID boundaries.

## Source preservation and execution gates

Preparation compared all 71 production items against `66f9cc77`: 55 function
bodies/signatures, 14 constants, the scope enum and its implementation. It also
compared all 21 test/helper functions and fixture constants, including property
functions. There were no changed semantic token streams or string/fixture literal
bytes. The comparison ignored comments, inter-token whitespace, helper visibility
and rustfmt's trailing-comma normalization; imports/module declarations moved.
Private helper visibility is scoped to the parent parser; existing API visibility
is preserved. No runtime-performance change is claimed.

Rust/native validation is pending. Through the owned shared build helper, run the
whole `rbit-workflow` unit suite and its `conformance` integration target, then
strict all-target Clippy. Keep the existing Actions evaluation, reusable-workflow,
planning and runner acceptance gates: they exercise downstream parse API consumers.
The extracted expression `Scope` is registered in the cumulative component policy;
its enum and implementation share one 400-line logical-type limit. The cumulative
AST and physical source bounds must pass before acceptance. No
Cargo or metadata command was used to prepare this extraction.
