# Isolated Actions visual review

Development-only synthetic screenshot runner, separate from root asset dependencies
and production Rust. Visual acceptance is pending the first successful isolated CI
run and human image review. No production renderer or Actions engine is changed.

The optional `actions-visual.yml` workflow runs on relevant pull-request paths,
matching main pushes, and manual dispatch after the workflow reaches the default
branch. It leaves the seven required CI jobs unchanged. No browser is launched
by the ordinary Node contract tests. Do not use this harness to control personal
browser sessions or visit arbitrary websites.

## Exact environment and containment

`npm ci --ignore-scripts --prefix devtools/actions-visual` installs only the isolated
locked Playwright 1.65.0 graph. Root dependencies and versions are untouched.
The Docker image is pinned in `policy.mjs` to Microsoft's 1.65.0 Noble digest;
Chromium must report 158.0.8127.18. The root asset toolchain builds `app.css` fresh
from the checked-out revision before capture. Its SHA-256 and the prototype tree
hash are recorded with the exact revision and isolated lock hash.

The CI-only launcher requires nonroot Linux. It mounts only the prototype tree,
exact public CSS/logo files and isolated harness read-only, plus one writable
artifact directory. It does not mount the repository, Git directory, credentials,
data, Docker socket or user profile. The container uses no network, drops all
capabilities, has a read-only root, private bounded tmpfs/shared memory, a 3 GiB
memory limit, two CPUs and 512 PIDs. It starts its own allowlisted server at the
fixed loopback origin; an occupied port fails rather than reusing a server.

Bundled full Chromium (channel `chromium`, new headless mode) launches with
`chromiumSandbox: true` in a fresh ephemeral profile.
Before any screenshots, the runner checks nonroot identity, absence of sandbox
bypass flags, exact browser version, and positive Layer 1 Sandbox = Namespace, PID/Network namespaces = Yes and
Seccomp-BPF = Yes rows from Chromium's internal `chrome://sandbox` diagnostic. That internal page
is not a network target or screenshot fixture. Labels match the
[pinned Chromium source](https://github.com/chromium/chromium/blob/158.0.8127.18/chrome/browser/resources/sandbox_internals/sandbox_internals.ts).
Unknown/unsupported proof fails
closed; no disabling flags or elevated-capability fallback is used. Browser
fixture requests additionally allow only fixed loopback GET/HEAD paths; unexpected
requests, WebSockets, popups, script errors and CSP violations fail their fixture.

## Capture and limits

There are 18 baseline cases: overview/run/job, desktop/mobile, DNA light/dark,
plus mobile RTL/long-label diagnostics in both themes. Eleven additional cases
cover approvals/permissions, loading/empty/error/denied and disconnected/expired/
truncated logs. These 29 images are candidates, not approved golden snapshots.
Assertions cover document overflow, skip-link/focus, native disclosures, hidden
permission controls, no-JS GET forms/earlier lines, copy/matches, and bounded log
follow without replacing a paused reader's window. RTL is a diagnostic, not a
translation. Manual screen-reader/contrast review and actual production middleware,
backend authorization, live stream and performance gates remain outstanding.

A separate pre-upload audit permits at most 40 PNGs, two JSON evidence files and
64 MiB total, with no links or nested paths. Artifacts expire after three days.
The job is bounded to 20 minutes and each container run to five minutes. No browser
trace, console contents, URLs from unexpected requests, arbitrary environment or
command line is retained. Failures have fixed category codes and fixture IDs.

Image metadata reported 1,002,814,156 compressed bytes across seven amd64 layers.
No image has been pulled locally. The first CI run records actual pull/test wall
time; network transfer bytes, unpacked disk and peak RSS are still unmeasured.
Measure these costs before broadening the gate or adding caches.

## Seccomp provenance

The `seccomp/` JSON fragments are a readability-only split of Microsoft's
[Playwright 1.65.0 policy](https://github.com/microsoft/playwright/blob/v1.65.0/utils/docker/seccomp_profile.json),
Copyright Microsoft Corporation, Apache-2.0 (see the repository's
[Apache license](../../LICENSE-APACHE)). `seccomp.mjs` reconstructs the original
object and verifies its canonical SHA-256 before use; no syscall rule is changed.
The official [Docker guidance](https://playwright.dev/docs/docker) describes the
nonroot namespace/seccomp requirement. Host support is deliberately not assumed.

After dependencies are installed on the disposable CI worker, invoke
`node devtools/actions-visual/container.mjs` with the runner-provided `GITHUB_SHA`.
There is no arbitrary URL, browser executable, Docker argument or origin option.

The host creates private ownership evidence and a random Docker label before
container creation. Cleanup verifies a full CID and that label, never a guessed
name. SIGINT/SIGTERM stops and awaits owned Docker clients before cleanup; an
`always()` workflow step is the cancellation backstop. Forced host termination
or runner loss can prevent both cleanup paths; the disposable worker is the final
lifetime boundary. No global Docker prune or unrelated container removal occurs.

Launch failures are classified in memory into fixed namespace/sandbox, missing
browser/library, read-only path, display, timeout or early-exit categories. The
raw Playwright message, browser arguments, paths and environment are not retained.
A namespace-denied result requires a disposable runner host that permits nonroot
user namespaces under this exact seccomp/capability policy. Do not disable the
sandbox, AppArmor, seccomp, no-new-privileges or capability restrictions to pass.
A dedicated ephemeral supported Linux runner can be evaluated with the same
positive proof before accepting screenshots; host support remains unproven until
that launch succeeds. The present failure does not establish its cause by itself.
