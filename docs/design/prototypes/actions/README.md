# Actions screen prototype

Review-only, server-rendered HTML fixtures for the delivery plan's Actions UI
prototype gate. No production template, route, view model, runner, workflow engine
or generated asset is changed. Node is the existing development toolchain, not a
new platform runtime. There are no added dependencies.

The prototype consumes an already-built **public** `app.css`, retaining the
platform font stack, Classic/DNA themes, semantic colour tokens and existing
button/input/box patterns. Its separate CSS supplies proposed screen layout with
logical spacing, mobile navigation and bounded log scrolling. It never rebuilds
or overwrites shared assets.

```sh
node docs/design/prototypes/actions/server.mjs --port 4187 --asset-root /path/to/current-checkout/crates/rbit-web/assets
```

The asset directory must already contain the current build's `app.css`. Without
that argument the preview uses this checkout's public asset directory and fails
if it has no built CSS. Port 0 chooses a free loopback port. The printed URL is the
review entry point. Stop the owned Node process after review.

The preview binds only `127.0.0.1`. It serves an exact allowlist of its screen
routes, CSS/JS, one public logo, the built stylesheet and a synthetic text log.
It never serves the checkout, `.git`, source directories or runtime data. Only
GET/HEAD are accepted; POST is 405. CSP blocks outbound connections and inline
scripts/styles. Forms use local GET preview intents and clearly state that no
repository/runner request was sent. `data-production-method="post"` identifies
controls that remain server-authorized POST forms in production; this is not a
drop-in replacement for their real endpoints or authorization.

## Review screens

| Screen | Default fixture | Proposed presentation |
| --- | --- | --- |
| `/overview` | Mixed run states, workflow/event/actor/branch metadata | Quiet workflow navigation, compact readable rows, filters and dispatch panel |
| `/run` | Failed Windows matrix job, successful siblings, skipped dependency | Clear failure destination, dependency stages plus accessible job list, summary/artifact sections |
| `/job` | Windows test failure in a 50,000-line synthetic log | Mobile job navigation, step details, bounded 80-line windows, anchors, earlier/raw output and search |

Open the **Design preview** disclosure to choose deterministic fixture variants.
Screens also accept these query parameters:

- `theme=dna-day|dna-dark|light|dark|system`
- `dir=ltr|rtl` and `long=0|1` (layout/length diagnostics; content remains English)
- `viewer=writer|reader|reviewer` (synthetic capabilities, never authentication)
- `state=failed|success|cancelled|running|approval|deployment|loading|empty|error|disabled|denied|disconnected|truncated|expired|queued`
- `workflow=0|1|2|3`, `branch`, `actor`, `event`, `status`, `ref`, `q`, `earlier=1`

Not every state applies to every screen. Log-specific states apply to `/job`;
disabled applies to a selected workflow; fork approval and environment review are
separate `/run` fixtures. Other-job links show smaller completed/skipped fixtures;
the Windows job is the focused long-log workspace. All examples are synthetic,
including names, commits, failures, durations and artifacts.

## Existing controls retained

| Current capability | Prototype representation / integration boundary |
| --- | --- |
| Workflow links, branch/actor/event/status GET filters, clear, older pages | Native links/forms; workflow and search selection stays outside any future polling region |
| Enable/disable | Writer-only preview intent; actual `disabled` POST and service checks remain unchanged |
| Dispatch ref change, typed inputs | GET ref form plus string/number/choice/boolean examples, hidden false boolean; production must resolve authorized workflow at the chosen immutable revision |
| Cancel/re-run all/re-run failed/per-job re-run | Writer-only controls; per-job shortcuts move to job detail on narrow layouts |
| Fork approval, optional trust-author checkbox | Writer-only fork fixture; retains explicit read-only token/no-secrets explanation |
| Environment review, comment, approve/reject | Reviewer-only deployment fixture; repository write access alone does not imply environment-review authority |
| Artifact ZIP/tar.gz, size/expiry; sanitized summaries | Review intents and synthetic summary; actual safe archive routes and Markdown sanitization are untouched |
| Raw log, step details, earlier window, expired/truncated notices | Native links/details and full synthetic raw download; retention loss is distinguished from omitted earlier lines |
| Current JS step-open preservation | Production `actions-log.js` stays unchanged; integrating live replacement must retain this behavior |

Log search, match navigation, copy, follow/pause and reconnect presentation are
**proposed**, not claims of current engine support. Find is a GET search of the
visible window. JS-only controls enhance that window; no-JS users retain details,
search, refresh, earlier lines, anchors and raw download. The running fixture has
an explicit **Simulate new output** control: it appends synthetic lines without
replacing focused controls and keeps the DOM capped at 80 rows. Follow pauses when
the reader scrolls away; output is not placed in a per-line live region. There is
no SSE/WebSocket/polling connection and no real reconnection implementation here.

## Validation status

Source/HTTP fixtures verify escaping, read-only route/asset isolation, permission
presentation, bounded DOM/log data and RTL diagnostic labelling:

```sh
node --test scripts/tests/rewrite-actions-prototype.test.mjs
node scripts/rewrite/check.mjs
```

On November 5, 2026, Tailwind 4.3.3 produced byte-identical CSS with and without
these prototype sources and their new markup fixtures: 144,701 bytes, SHA-256
`4a134b074d858b16720edb61f4d92059c1f6518d0cfafd4f82514db70d7a55ad`.
No production source-scan exclusion was justified by that measurement.

**Visual acceptance is pending.** No screenshot, accessibility, overflow or
browser-performance pass is claimed. The separate
[isolated Linux CI browser proposal](visual-validation.md) records exact dependency
and image candidates, footprint, restrictions and acceptance cases. Package/image
changes require review before implementation.

Before production integration, review desktop and 360 px screenshots in light/dark,
RTL and long-label fixtures. Verify keyboard-only navigation/focus, no-JS paths,
screen-reader landmarks/status announcements, contrast, permission combinations,
log-window/scroll/focus preservation and disconnected/empty/error states. Then
exercise the actual Axum/Askama/HTMX routes, authorization, CSP/cache/security
middleware, live updates and measured rendering/memory/DOM budgets. A static
prototype pass cannot replace those application gates or authorize an engine port.
