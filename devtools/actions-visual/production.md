# Actual production Actions run fixture

This development-only contained mode uses the existing compiled `rbit-web`
integration test binary, not a second release/LTO build or a mock HTML renderer.
One ignored test seeds synthetic users, one public repository and one fork run
awaiting approval through existing fixture services/stores. It listens only at
`http://127.0.0.1:4189`, keeps SQLite/redb under a new container temp directory and
never opens application configuration or user data. Ordinary tests do not start it.

The Node owner executes only that exact ignored test. Its private process pipe
transfers a new synthetic session cookie into a class-private field; stdout/stderr,
cookie values, database files and browser storage are never logged or artifacted.
A private fixed Unix socket accepts only revoke-writer and stop commands. Writer
revocation uses the real collaborator service while the browser keeps GET/HEAD-only
network access. Shutdown joins the HTTP server, closes fixture stores and confirms
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

Node fake-process/file/DOM-contract tests are not browser proof. Rust compilation,
real isolated Linux execution, actual asset compatibility and human image review
remain required. Overview pages, deployment text drafts, full screen-reader review
and release performance remain outside this first bounded fixture.
