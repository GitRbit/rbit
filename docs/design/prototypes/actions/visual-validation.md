# Proposed isolated visual gate

Design proposal only. No browser dependency, image, package manifest, lockfile or
workflow is added by this prototype. Visual acceptance remains pending. Review
the footprint and trust boundary before implementing this separately from the
production renderer or Actions engine.

## Recommended first gate

Use a disposable Linux CI worker and a fresh headless Chromium context, without
personal browser profiles, sessions, storage, extensions or native UI automation.
The sole test origin is fixed `http://127.0.0.1:4187`; the test starts its own
allowlisted prototype server with `reuseExistingServer: false` and fails if the
port is occupied. No caller-supplied target URL is accepted.

Keep an exact test dependency and lockfile in an isolated development directory,
for example `devtools/actions-visual/`, outside production and root asset tooling.
Use `npm ci`, then its locally installed binary; never install an unpinned `npx`
package. No browser assets or Node runtime ship with rbit.

Verified candidate on 2026-11-05:

| Component | Exact candidate | Registry metadata |
| --- | --- | --- |
| Microsoft `@playwright/test` | `1.65.0` | Apache-2.0; Node >=20; 28,544 unpacked bytes |
| Its `playwright` dependency | `1.65.0` | 5,063,827 unpacked bytes |
| Its `playwright-core` dependency | `1.65.0` | 13,969,978 unpacked bytes; no further runtime dependencies reported |

These three package payloads total 19,062,349 bytes before browsers, installation
metadata and output artifacts. Registry SHA-512 integrity values to preserve in
the future lockfile:

```text
@playwright/test sha512-iGURLGuKDLakSEl7Pgfv8h7f+XLT+V861Wfhz0zlEl5nyFuKUopdkFBPKqTllTUp06aOwPuH8iuUEb3yp3MpbQ==
playwright      sha512-F3AMWcR44qMBt4kq+6Kb6mX7bjGXshktKFOPULd7reED6ZJWevjbNRqysP7q+AATE9kdPEZBibex1BYsvpVoeA==
playwright-core sha512-vNSKgH6YqtN7cdT/L334zo0a+YwkrpicyrHJlWWPIm/Q+Nn2SWInil01oT0mbu96U4GgoRtCYQD9CzJQgju+lA==
```

The public npm bulk advisory query returned an empty map for these versions on
that date. This is a time-bound package advisory observation, not proof of safe
browser, OS or container dependencies. Recheck the exact lockfile when implemented.
Microsoft's [release](https://github.com/microsoft/playwright/releases/tag/v1.65.0)
and [browser revision manifest](https://github.com/microsoft/playwright/blob/v1.65.0/packages/playwright-core/browsers.json)
identify Chromium 158.0.8127.18 / revision 1254. Record and verify the actual browser
version at runtime rather than assuming the tag guarantees what launched.

## Container versus installation tradeoff

For repeatable OS/fonts/browser bytes, the preferred baseline environment is the
official image with both version and digest pinned:

```text
mcr.microsoft.com/playwright:v1.65.0-noble@sha256:6e3fb709361521eb27161606e6da6d2674e5e89fa16516cb37d6fdd91372273d
linux/amd64 manifest: sha256:bc4fbb329a25287ed23e9216c97063646abdd1bf2da669a9895fa3e2078b2302
```

Only registry metadata was read: no image was pulled. The amd64 manifest reports
seven layers totalling 1,002,814,156 compressed bytes. Unpacked disk, pull time,
setup time and test RSS are unmeasured. This image also includes browser families
the first gate will not use. That is a significant ephemeral CI cost, so keep the
initial job manual/main-only and measure its setup cost before broad PR adoption.

The smaller alternative is an ordinary hosted Linux worker installing only the
pinned Chromium/headless shell. It avoids pulling unused Firefox/WebKit images,
but system libraries/fonts and runner updates are less reproducible; browser
download plus OS dependency installation cost must still be measured. Do not
claim pixel-identical baselines across these environments or introduce browser
caches without measured benefit and an explicit trust/eviction policy.

Official images default to root, which disables Chromium sandboxing. The proposed
job should use a non-root browser user and explicitly enabled Chromium sandbox
with the supported seccomp setup. Verify that launch in the hosted runner before
approving the matrix; do not silently fall back to `--no-sandbox` if it fails.
The [official Docker guidance](https://playwright.dev/docs/docker) explains this
boundary and the requirement to match image and package versions.

## Runtime containment and provenance

- Read-only workflow permissions; no secrets, publication tokens, saved browser
  state, production server, credentials, personal paths or runtime databases.
- Pinned checkout/setup-node/artifact Actions, `persist-credentials: false`; obtain
  only `app-css` from the compatible same-revision asset job, verify its digest.
- Allowlist only localhost GET/HEAD page/asset paths in browser request routing;
  abort every other origin/method, unexpected popup and service worker. The Node
  preview also rejects mutation methods and serves no checkout/runtime directory.
  These application/browser restrictions are not a claim of OS-wide egress isolation.
- One worker, no Rust build, no new target directories. Keep installation and
  screenshots inside the disposable CI job, separate from developer shared caches.
- Record revision, package/lock integrity, image/architecture/browser version,
  CSS/source digest, fixture ID, viewport, pixel ratio, locale, timezone and timings.
  Do not serialize arbitrary environment values, browser state or command lines.

## Bounded capture and interaction matrix

Start with the three default screens at 1440×1000 and 360×800, DNA light and DNA dark
(12 images), then the same screens at 360×800 in RTL/long-label diagnostic mode in
both themes (six images). Use fixed locale `en-US`, timezone UTC, device scale 1,
reduced motion and explicit colour scheme. Wait for local styles/fonts and a
stable render; fixed fixtures contain no wall-clock or actual runner updates.

Add focused images for fork approval writer/read-only, environment review reviewer/
writer, loading/empty/error/denied and disconnected/expired/truncated log states.
Keep a declared maximum 40 images and 64 MiB total artifact budget; unexpected growth
fails visibly. Capture content viewport/full-page only, with fixed names and no
personal browser chrome. Retain a manifest and failure diagnostics for three days.

Browser assertions must cover:

1. No document-level horizontal overflow at 360px, RTL or long labels. Horizontal
   scrolling is confined to repository tabs and the labelled log region.
2. Tab order, visible focus and skip link; Enter/Space on native step/mobile-job
   disclosures. Hidden permission controls must not appear in the accessibility tree.
3. No-JS overview filters/dispatch ref, step disclosures, refresh, earlier lines,
   anchors and raw download. Proposed copy/follow controls are hidden without JS.
4. Reader/writer/reviewer capability fixtures, while every attempted action stays
   a local non-mutating preview intent. This does not replace production service tests.
5. Log search/match navigation and copy feedback. While follow is paused, synthetic
   output must not replace the visible window, move focused controls or steal scroll.
   Following resumes at the tail; DOM and pending buffers stay bounded.
6. No unexpected page errors, external requests or CSP violations. Inspect ARIA
   snapshots/landmarks and ensure no per-line live announcements. These checks do
   not replace manual screen-reader review or measured theme contrast.

The first run produces candidate images for human inspection, not an automatic
"looks good" verdict. Only after that review should approved Linux-specific golden
images be committed and compared. Never automatically approve updated snapshots.
Playwright documents [environment-dependent screenshots](https://playwright.dev/docs/test-snapshots)
and recommends [one CI worker for reproducibility](https://playwright.dev/docs/ci).

No production renderer/Actions port follows solely from this gate. Actual
Axum/Askama/HTMX routes still need authorization, CSP/cache, no-JS/history, live
update/focus/scroll, accessibility and measured performance acceptance.
