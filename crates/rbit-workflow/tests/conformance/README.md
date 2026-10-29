# rbit-workflow conformance eval

Data-driven cases for the workflow crate. Each case states an input and the outcome GitHub
Actions gives for it. `tests/conformance.rs` loads every `*.json` file here and grades the
crate against each case programmatically; no model judges a result.

## Files

- `expressions.json`, `matrix_plan.json`, `concurrency_run_name.json`, `triggers.json`,
  `dispatch_schedule.json`, `parse_load.json`: the cases, by area.
- `known_failures.txt`: case ids that fail today. The runner reports them as `KNOWN` and
  does not fail on them. Remove an id once its case passes; a listed id that now passes is
  reported as `NOW PASSES`.

Run it:

```bash
cargo test -p rbit-workflow --test conformance -- --nocapture
```

## Case schema

```json
{
  "id": "push-branch-match",
  "area": "triggers",
  "tags": ["easy"],
  "why_hard": "...",
  "source": "GitHub Docs, ... the behaviour the expected value relies on",
  "split": "train",
  "kind": "trigger",
  "input": {},
  "expect": {}
}
```

- `tags`: one or more of `easy`, `hard`, `edge`. A `hard` case must say `why_hard`.
- `source`: the GitHub docs behaviour the expectation relies on. Every expectation is one two
  Actions experts would agree on; a case whose behaviour is ambiguous, or that
  `docs/actions.md` lists as a known gap (reusable workflows, `jobs.<id>.uses`,
  `secrets: inherit`, custom shells, `container.options`, `hashFiles` outside steps), is
  dropped, not kept.
- `split`: `train` or `test`. Assigned by a seeded draw: `random.Random(20261027)`, one
  `rng.random() < 0.3` per case, in file order (files sorted by name, cases in file order).
  Test cases are held out of hill-climbing; a fix that only passes train cases has not
  generalised.
- `yaml` inputs are an array of lines (joined with newlines) or one string.
- Expectations: `{"error": true}` matches any error. `{"ok": true}` means the workflow
  parses. `{"value": X}` must equal `X` (numbers compare as numbers). A `plan` expectation's
  `jobs` list matches planned jobs in any order, each by subset of the listed fields.

## Kinds

| kind | input | expect |
|---|---|---|
| `expr` | `template`, `contexts`, optional `status`, `hash_files` | `value` or `error` |
| `condition` | `condition`, `contexts`, `status` | `value` (bool) or `error` |
| `render` | `template`, `contexts` | `value` (text) or `error` |
| `needs_status` | `results` (job results) | `value` (`success`, `failure`, `cancelled`, `skipped`) |
| `parse` | `yaml` | `ok` or `error` |
| `plan` | `yaml`, optional `run` (`github`, `inputs`, `vars`) | `jobs` or `error` |
| `run_name` | `yaml`, `run` | `value` (text or null) |
| `concurrency` | `yaml`, `run` | `value` (`{group, cancel_in_progress}` or null) |
| `trigger` | `yaml`, `event`, optional `changed_paths` | `value` (bool) |
| `dispatch` | `yaml`, `given` | `value` (input name to text) or `error` |
| `pattern` | `patterns`, `inputs`, `mode` (`include` or `ignore`) | `value` (passes: bool) |
| `skip_marker` | `text` | `value` (bool) |
| `cron` | `expression`, optional `after` (unix seconds) | `value` (`{too_frequent, next}`) or `error` |

`event` is `{"type": "push", "ref", "deleted"}`, `{"type": "pull_request", "action", "base"}`,
`{"type": "dispatch"}` or `{"type": "schedule", "cron"}`.

## Harness rules

- Each case runs under `catch_unwind`; a panic is that case's failure, and the rest still run.
- Every failing case is reported with its id, input, expected, actual and the GitHub rule.
- A malformed case (missing field, unknown kind or tag, hard case without `why_hard`, a split
  other than `train`/`test`, a duplicate id) fails the run.
