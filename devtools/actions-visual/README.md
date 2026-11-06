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
object and verifies its canonical SHA-256 before use. The upstream digest is
`8e3abd795acf8d96f90d4f2103f2b9665c21ab645df244d4eedcc5c95ceac3a2`.
The browser policy appends exactly one unconditional `chroot` allow rule; all
upstream rules, conditions and default action remain identical. Its canonical
digest is `106b9b2b44d775c324f57bd196bf6b6a46add952e8955804cfc6e2e97b75813c`.
Both evidence files record this digest and the `namespace-chroot-only-v1` delta.
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

## Avoiding repeated heavy captures

GitHub pull-request path filters compare the whole PR's three-dot diff, so an
older prototype change can trigger a workflow on later unrelated rewrite pushes.
The lightweight relevance job additionally compares the synchronize event's exact
validated `before` and `after` commits. Renames expose both added and removed paths.
Only a proved irrelevant event delta skips the browser/image job. Initial/reopened
PRs, first workflow additions, manual/main events, unavailable commits, malformed
identity and Git errors conservatively capture. A failed lightweight job also
causes the heavy job to run rather than silently accepting a missing decision.

There is no workflow-wide cancellation group. Only the heavy capture job has a
per-ref group, with cancellation disabled, so a new irrelevant skipped run cannot
cancel an older real capture. GitHub may still replace a pending job with a newer
pending relevant job; running captures are preserved. Cleanup belongs to the heavy
job and runs even during its cancellation, using its private ownership evidence.
## Launch evidence and preflight

Before launching, the runner creates its fixed ephemeral HOME with mode 0700,
checks ownership and lack of links, and performs an exclusive write/read/delete
probe. It records only readiness booleans, numeric UID and expected executable
existence/executability/bundled-root booleans, never paths. Known container-visible
kernel observations are NoNewPrivs, Seccomp/filter count, whether effective,
permitted, bounding and ambient capabilities are zero, maximum user namespaces,
unprivileged-userns setting and
AppArmor userns restriction. Missing/unreadable/invalid fields are null; these
observations alone do not prove Chromium's sandbox and do not identify an LSM
profile. Actual browser proof is still required before any capture.

Failed launch evidence accepts only the known Playwright process-exit framing,
Unix exit range and a fixed signal set. Chromium stderr yields only enumerated
source categories and reason codes from bounded in-memory text. Matched DBus or
other stderr categories are observations, not proof of the fatal cause. No raw
stderr, full command, environment, unknown source filename or private value is
retained. Unknown evidence remains unknown and the run stays invalid.

The exact upstream Playwright 1.65 policy has no clone3 rule. Its default ERRNO
therefore uses EPERM under the [OCI definition](https://github.com/opencontainers/runtime-spec/blob/main/config-linux.md#seccomp).
The [glibc 2.39 wrapper](https://github.com/bminor/glibc/blob/glibc-2.39/sysdeps/unix/sysv/linux/clone-internal.c)
only falls back from clone3 on ENOSYS. The current [Moby policy](https://github.com/moby/profiles/blob/main/seccomp/default.json)
explicitly denies clone3 with errno 38 without CAP_SYS_ADMIN. This is a plausible
compatibility difference, not an established cause of the failed CI launches.
The chroot compatibility delta leaves clone3 unchanged. Any later narrowly justified
errno compatibility adjustment needs separate review and exact-delta tests; permitting
clone3, disabling sandbox controls or broadening host privileges is not a fallback.

The latest retained failure reached browser launch with a nonroot UID, private
writable HOME, bundled executable, NNP/seccomp and zero effective capabilities,
then aborted in the zygote host before any browser sandbox proof. The observed
AppArmor userns-restriction setting alone does not prove an LSM denial. Zygote
boot/hello/premature-exit categories and bounded numeric known-source locations
now distinguish these failure sites; a known child exit may identify namespace
versus setuid mode. Numeric source lines correspond to the pinned
[Chromium zygote host](https://github.com/chromium/chromium/blob/158.0.8127.18/content/browser/zygote_host/zygote_host_impl_linux.cc),
not copied private paths. No setuid or sandbox-disabling fallback is enabled.

## Namespace filesystem compatibility

The observed hello failure precedes any accepted browser sandbox proof. Source
inspection identifies a concrete policy incompatibility: the upstream `chroot`
allow depends on configured `CAP_SYS_CHROOT`, excluded by `--cap-drop ALL` even
after Chromium creates its own user namespace. Chromium sends its boot message
before entering the [layer-one sandbox](https://github.com/chromium/chromium/blob/158.0.8127.18/content/zygote/zygote_main_linux.cc),
whose [filesystem isolation](https://github.com/chromium/chromium/blob/158.0.8127.18/sandbox/linux/services/credentials.cc)
uses chroot before the hello message. This source incompatibility motivates the
single-rule correction; it does not establish that no other launch blocker exists.

Allowing this syscall is an explicit outer seccomp surface expansion. Classic
seccomp cannot constrain the pointed-to pathname. Kernel namespace capability
checks still apply: no host or container capability is granted, and network,
mounts, read-only root, NNP and every other seccomp rule remain unchanged.
This is not a claim of zero added attack surface or syscall-trace attribution.

Before either probe or Chromium, nonroot identity, NNP, seccomp filtering and all
four zero capability sets are mandatory. A fixed chroot probe against a fresh
private empty directory must fail with exit125 and EPERM in the initial namespace.
EPERM alone cannot distinguish a kernel capability check from seccomp or an LSM.
A separate fixed `unshare --user --map-root-user --root` child must successfully chroot,
then fail to execute its deliberately absent command with the specific exit127
and ENOENT diagnostic. This proves namespace-local operation, not host privilege.
The [util-linux implementation](https://github.com/util-linux/util-linux/blob/v2.39/sys-utils/unshare.c)
chroots before exec in that same process, avoiding an intervening exec that could
discard its namespace capabilities under the zero bounding set. No capability
retention flag or additional helper executable runs inside the new root.
The probe does not fork: timeout kills the owned unshare process directly.
It needs no pidfd syscall allowance or detached-worker cleanup.
Probe tools, unknown results or timeout failures prevent browser launch. No raw
probe stderr or paths are retained. The child and its scratch directory are owned
and cleaned within the disposable container. Actual Chromium namespace/PID/network
and seccomp proof remains mandatory before screenshots; successful CI capture and
human visual acceptance are still pending.
