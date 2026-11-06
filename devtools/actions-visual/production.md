# Actual production Actions run fixture

This development-only contained mode uses the existing compiled `rbit-web`
integration test binary, not a second release/LTO build or a mock HTML renderer.
One ignored test seeds synthetic users, one public repository and 26 bounded runs
through existing fixture services/stores. Fork runs await approval; one valid
job template is claimed by a synthetic registered runner through existing services.
It listens only at
`http://127.0.0.1:4189`, keeps SQLite/redb under a new container temp directory and
never opens application configuration or user data. Ordinary tests do not start it.

The Node owner executes only that exact ignored test. Its private process pipe
transfers a new synthetic session cookie into a class-private field; stdout/stderr,
cookie values, database files and browser storage are never logged or artifacted.
A private fixed Unix socket accepts only revoke-writer, finish-log-job and stop commands. Writer
revocation uses the real collaborator service. Actions navigation remains GET/HEAD;
the only browser mutation is the bounded synthetic language form described below. Shutdown joins the HTTP server, closes fixture stores and confirms
the actual process exit before acceptance. Unexpected exits/cleanup failures fail.

## Binary and public asset handoff

After the existing Linux workspace test build, the exporter uses nextest's
[binary-only JSON](https://nexte.st/docs/machine-readable/list/) and Cargo package IDs
to select exactly the web integration target. It neither searches by basename nor
builds a new release variant. Revision, binary hash, lock hash and every copied
public asset hash are recorded. Bounds are one 512 MiB binary, 2048 asset files /
64 MiB and a 1 MiB input manifest. Links, private/runtime file types, unexpected
top-level files, untracked assets and copy/hash drift are refused. `app.css` is the
only explicitly permitted untracked asset, built before the test binary.

Debug rust-embed reads runtime files at its compiled manifest directory. The
exporter therefore refuses any build root other than the fixed hosted CI checkout.
The container mounts only the verified binary/manifest and public assets read-only,
with those assets at `/home/runner/work/rbit/rbit/crates/rbit-web/assets`. It mounts
no source checkout, Git directory, database, profile or host socket. This preserves
the actual same-revision debug CSS/JS behavior without another web compilation.
This is a functional debug fixture, not release performance evidence.

The existing Linux test job exports this input only for relevant source events.
The optional consuming job downloads only that named artifact from its own workflow
run and verifies revision, ELF identity and all hashes before restoring execute mode
on the exact single-link binary. Artifact upload/download does not preserve its
executable mode. No raw Cargo output directory is mounted or searched by basename.

The consuming job waits for **both** existing test platforms. It does not change
the seven branch-protection aggregate dependencies; its own failure still fails
the overall workflow. It inherits the CI workflow's PR supersession cancellation.
On synchronize events, both producer and consumer compare exact validated
before/after commits; unknown comparisons run conservatively. All crate changes
are relevant because real shared middleware and persistence can affect the page.
Documentation-only deltas skip export and the image pull. Initial/manual/main runs
remain conservative. This is a bounded reuse gate, not a latency optimization claim.

```text
node devtools/actions-visual/export-production.mjs
node devtools/actions-visual/container.mjs --production
node devtools/actions-visual/cleanup.mjs --production
node devtools/actions-visual/artifacts.mjs actions-production-artifacts
```

Export is refused outside the fixed Linux GitHub CI context. There is no arbitrary
binary, command, URL, port, mount destination, live-data or browser-profile option.
The input directory is separate from the screenshot artifact directory and must
never be uploaded as visual evidence. Only the bounded public build-input artifact
contains the binary/public assets; it contains no seeded runtime state or cookie.

## Browser gates

All existing image/version, nonroot, zero capability sets, NNP, seccomp and namespace
proof checks remain mandatory before starting the fixture. The browser still uses
`chromiumSandbox: true`; no disabling flag, capability grant or security fallback
is added. Fixed loopback read routes cover the run and actual repository navigation
preloads; all other origins, mutations and WebSockets are blocked. Service workers
are blocked in fresh contexts. Screenshot budgets remain 40 images / 64 MiB total.

The first four production candidates cover desktop light, native no-JavaScript
mobile controls, mobile RTL/dark and writer-permission removal. JavaScript checks
wait for actual three-second HTMX node replacement and require the edited approval
checkbox and focus to survive. Removing the writer grant through private fixture
control must remove the freshly rendered approval form while keeping public run
read access. No old form is reconstructed. CSP headers and layout/console/network
checks remain active; JavaScript-disabled pages cannot run the CSP event observer.
The revocation check observes a natural same-run HTMX GET started after the private
revocation acknowledgment, then its matching successful selected-root swap within
the existing ten-second bound. A response already in flight before revocation
cannot falsely count as that proof. No manual polling or update cancellation is used.

Three additional Overview candidates exercise the real 25-row newest page, its
one-row older page, empty filters, a dirty focused branch input across a natural
`#runs` replacement, mobile RTL/dark and actual native GET submission with JavaScript
disabled. Filter/dispatch controls remain outside the refreshed result region.
Dispatch execution and workflow-file editing are not exercised by these fixtures.

Four Job candidates cover live desktop, mobile RTL/dark, native no-JavaScript details
and a finished failure. The service-driven synthetic runner reports two steps and
appends 1207 lines through the real masking/index/durability path. One runtime task
token is deliberately appended privately; expected browser bytes contain only
`task token ***`. The browser requires the exact masked raw digest before capturing
any Job image. Only byte/line counts, SHA-256 and compression flags enter evidence;
raw response bodies, runner credentials and task tokens are never artifacted.
Live raw responses also traverse the page transport compressor, so their HTTP
`Content-Encoding` may be Brotli, gzip or deflate. Evidence records this bounded
transport category separately from the known live/final fixture storage stage;
browser-decoded bytes must still match exactly. Final storage requires the actual
gzip header plus the independent Rust fixture's final-state/raw-file proof.

The job checks require the existing 1000-line preview, original 206–1205 line numbers,
the larger 1205-line step page, and a keyboard-opened step surviving a natural poll.
They close that long step for compact screenshots after proving its contents and
open-state preservation. The private fixed completion command uses `task_state` to
finish the known job and finalize its real gzip file; browser-decoded gzip must
match the earlier live raw digest and byte count. Browser requests remain GET/HEAD
only. No runner shell command, workflow execution or additional build profile runs.
These eleven Actions candidates remain under the unchanged forty-image / 64 MiB ceiling.

Node fake-process/file/DOM-contract tests are not browser proof. Rust compilation,
real isolated Linux execution, actual asset compatibility and human image review
remain required. Deployment text drafts, workflow dispatch, full screen-reader review
and release performance remain outside this bounded fixture. Job polling retains
the existing step-open contract; this does not claim to preserve summary focus across
a replaced Job subtree. Run approval and Overview filter focus are explicitly checked.

## Native account language acceptance

Three additional candidates cover real Spanish account language settings and Explore
navigation, bringing the total to fourteen. They reuse the same test binary, users,
public assets and hardened browser/container; no new seed protocol or runtime export
is added. Language checks run after the Actions permission-removal checks, so their
account preferences cannot change those earlier diagnostic fixtures.

The network policy now permits one bounded mutation family: same-origin native
`POST /settings/language`, with no query, exact `locale=en`, `locale=es` or empty
`locale=` form body, URL-encoded content type and the fixed loopback Origin. The
interceptor reads only that body and the two validation headers, never cookie or
authorization inventories. Every other mutation remains denied. Only exact language
settings GETs (optionally `saved=1`) and the unfiltered Explore GET join the existing
read allowlist. Production CSRF/authentication middleware still handles the real form.

An English-browser session saves Spanish through the rendered nonboosted form, then
requires Spanish document/catalog metadata and a real HTMX Explore response without
replacing the document. Back navigation must recover the Spanish language form. A
360px Spanish-browser session with JavaScript disabled resets the preference through
the native form and follows browser Spanish. The earlier Spanish document, whose
browser prefers English, must then receive an actual cross-catalog refresh on its
next HTMX navigation; a subsequent English HTMX navigation must stay in-document.
Cookie values, storage, posted bodies, database files and raw responses are excluded
from artifacts. Captures contain synthetic page content only.

Policy matrices and independent fake-browser transitions exercise denial boundaries,
wrong-save/no-JS failures and stale swaps. They do not establish actual Playwright,
HTMX history, sandbox, linguistic or visual acceptance. Those hosted gates remain
pending until this exact revision runs in the isolated Linux job.

### Native redirect transport boundary

For an admitted language POST, the fixture fetches the actual response with
`maxRedirects: 0`, `maxRetries: 0` and a ten-second timeout. Only status 303 with one
exact relative Location `/settings/language?saved=1` is forwarded, unchanged, to the
browser. A 307/308 replay response, another status, duplicate Location, another
destination or transport error is refused before forwarding. The real response's
cookie headers are retained; there is no synthetic 200, manual navigation or secret
header artifact. The API response is disposed after fulfillment. Actual hosted
cookie/redirect compatibility must still pass this exact pinned Playwright revision.

[Playwright's redirect behavior](https://playwright.dev/docs/network#redirects) invokes
a route handler only for the initial request, including when a 3xx is fulfilled.
Consequently this is initial-request gating and prevention of an unverified POST
redirect/replay, not preemptive per-hop GET filtering. An observer requires the
browser's real 303 response followed by the exact fixed GET and a final 200; it faults
unexpected subsequent requests or redirect responses. Positive completion counts
are required before the browser context closes. These observers detect issued GET
hops; they cannot revoke a request already issued. The unchanged network-none
container namespace remains the actual boundary against host/external egress.

The source fixtures cover refused 307/308 responses, hostile/duplicate destinations,
failed fetches, request replay and unexpected subsequent GET chains. They do not
replace actual isolated browser execution or establish general per-hop containment.


The Spanish theme picker candidate uses the same synthetic account/browser and exact
read-only `/themes/picker` endpoint. It verifies translated accessible/search labels,
Colorblind filtering and empty/results counts without submitting a theme change.
Native no-script links still target Appearance settings. Preview example code remains
explicitly English; catalog theme names and machine filter values are unchanged.
This candidate requires actual hosted browser/Rust acceptance; Node fixtures alone
are not browser proof or a claim that all settings content has been translated.
