# Actions native browser routes

Actions owns its browser documents and mutations through the same compiled Askama
registries available to native plugins. `prepare_plugin_routes(&state).await?` binds
weak host factories before HTTP startup. `Services::bind_actions_routes` attaches the
blueprints to the actual `rbit.actions` scope and generation. Disable withdraws all
pages and forms before lifecycle work waits; re-enable publishes fresh versions and
invalidates old form proofs. Core-only and i18n-only distributions publish none of
these routes. `/admin/plugins` remains the host recovery and lifecycle surface.

## Canonical pages

Repository paths below start with `/{owner}/{repo}/-/plugins/rbit.actions/`.
Global paths start with `/-/plugins/rbit.actions/`.

| Scope | GET path | Declared resource/query fields |
|---|---|---|
| Repository reader | `runs` | branch, actor, event, status, before, ref, workflow |
| Repository reader | `workflow` | workflow plus run filters and ref |
| Repository reader | `run` | run number, jobs_page and artifacts_page |
| Repository reader | `job` | run number and job ID |
| Repository reader | `logs` | run number, job ID and step number |
| Repository reader | `summary` | run number, job ID and optional step number |
| Repository administrator | `settings` | done notice, edit, after, secret_edit, secret_after |
| Repository administrator | `settings/environments` | none |
| Repository administrator | `settings/environment` | environment name, done notice, edit, after, secret_edit, secret_after, secret_edit, secret_after |
| Repository administrator | `settings/runners` | none |
| Signed-in account | global `settings`, `settings/runners` | done, edit/after and secret_edit/secret_after on settings |
| Organization manager | global `org/settings`, `org/runners` | owner; done, edit/after and secret_edit/secret_after on settings |
| Site administrator | global `admin/runners` | none |

These are fixed registrations, not a growing table of routes per run or resource.
Queries are projected through an explicit descriptor schema, bounded to 16 fields,
2 KiB per value and 8 KiB total. Duplicate, unknown and malformed fields fail before
native code runs. Domain services resolve identifiers and enforce resource ownership;
a job/run mismatch or an inaccessible repository does not reveal the resource.

## Forms and authorization

Repository mutations use fixed `dispatch`, `switch`, `cancel`, `rerun`, `rerun-failed`,
`rerun-job`, `approve` and `review` actions, plus configuration actions under `settings`.
Global configuration and runner actions stay under the same Actions namespace.
Resource identifiers are bounded typed form fields. They are not permission grants.
The host verifies session, exact action, repository, boot, generation, version, expiry
and Origin before invoking a callback, then reloads the session and repository grants.
The callback reloads the relevant run, environment, runner or organization through
its existing authorization service before changing it.

Run controls require repository write permission. Deployment review has a narrower
signed-in reader floor so a designated reviewer does not need write permission;
`review_deployments` still verifies the configured reviewer and self-review policy.
Repository configuration requires administration. Personal forms operate on the
signed-in account. Organization forms independently check management of the submitted
owner, including on rejected-form rendering. Instance runner forms require site admin.

Each native form contains an explicit host proof and works with JavaScript disabled.
After authorization, sibling proofs are reissued only for the same owner, scope and
generation and only when their own permission floors pass. Rejected forms render
compiled 422 documents. Runner-token issuance renders a compiled 200 document once;
the secret is never placed in a redirect, session flash or history cache. Successful
changes otherwise redirect within the same plugin/scope using bounded query values.

Ordinary forms retain 4 KiB field / 32 KiB body limits. Dispatch explicitly permits
a 256 KiB encoded body so all 25 allowed 1 KiB workflow inputs survive percent encoding.
Configuration explicitly opts
into 64 KiB text fields and a 256 KiB form body; domain services retain their existing
64 KiB secret and 48 KiB variable limits. Repeated repository selections accept at most 128 entries per submission as
explicit bounded lists; larger submissions fail validation without changing settings. Dispatch inputs use a declared bounded prefix and the workflow's own schema.
Other duplicate fields, including proofs, remain invalid.

## Output, lifetime and performance

All documents render through compiled Askama models and the bounded native writer;
there is no runtime template interpreter or extra browser framework. Locale helpers
render against the captured immutable catalog even in independently owned mutations.
Responses retain plugin admission and rendering capacity through body consumption or
cancellation, use `no-store`, decline preloads and keep the normal host security stack.
Native documents disable boosted form/history storage; explicit job polling continues.
Host-approved appearance metadata preserves the selected theme and unread indicator
across ordinary/no-form GET pages and owned success/validation responses, without
passing cookies or headers into native callbacks.

The ordinary response budget remains 256 KiB. Configuration documents explicitly
allow at most 2 MiB to preserve complete editable variable values. Each registry
admits only eight such larger operations in addition to its normal shared admission
limit, with permits retained through the final response. This is a bound, not a
promise that arbitrarily many large stored rows fit a single document. Variable and secret
lists page through 25 names at a time; variable values use short previews. An explicit Edit link
loads one complete editable value, preserving it without truncation and keeping
multiple maximum-size values from exhausting the document budget. Secret
replacement and repository-selection editors load for one selected row, with
repository choices allocated once for the new form and only for the active editor. Native document
authorization also avoids loading repository header data and Git refs that the
compiled Actions factory will load for its own chrome.

Job previews have an aggregate 16 KiB / 128-line budget and show at most 32 steps,
retaining important running/failed steps and reporting omissions. Standalone step
previews use the same byte/line budget. Raw-log downloads retain their complete bytes.
Large escaped log tests cover the compiled output cap and raw-download preservation.
Run metadata is paged in groups of 32 jobs and 32 artifacts, independently, while
controls still inspect the complete run state. Run summary previews load metadata
and at most four bounded prefixes totaling 8 KiB, each capped at 24 KiB of rendered
HTML. Every summarized job links to its full per-step page. That page reads one
current-task summary plus indexed previous/next step numbers, renders at most
512 KiB of sanitized HTML, and falls back to its complete escaped Markdown source
when expansion exceeds the rendering budget. No old-attempt body is exposed after
a rerun. Ordinary documents do not add browser-cookie parsing for non-locale forms.
The existing owned native commit rules apply: a timed-out or aborted HTTP waiter
cannot undo an already-started write; do not automatically retry an unknown outcome.

## Compatibility and transport boundaries

Legacy browser GET bookmarks redirect to the corresponding native page while Actions
is active. Legacy browser POST handlers are removed; redirects never replay writes.
Navigation links use canonical native destinations where the UI target supports them;
the organization card retains a compatibility GET because its owner is contextual.

Badges, raw logs and artifact downloads retain specialized host transport adapters:
SVG caching, gzip negotiation and streaming archives have different contracts from
bounded HTML documents. REST endpoints, runner registration/task transport and the
independent runner executable also remain on their existing routes. Their Actions
availability and generation guards still apply. This phase adds no executable package
installer, native-code sandbox, database migration or machine-code unloading.
