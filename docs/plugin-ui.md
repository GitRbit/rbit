# Plugin UI extensions

Plugin UI uses compiled Askama templates and the same semantic components, escaping,
accessibility and navigation conventions as the core. Trusted embedded plugins retain
compiled Askama pages. Small contributions use a versioned data contract rendered through
compiled host Askama templates; there is no runtime template compiler or browser template
engine. Language archives never supply templates or executable code.

## Registration lifetime

The trusted composition root supplies `Services::ui_registry()`. During preparation, stage
one validated `UiContributionSet` with the plugin ID, scope and current generation. Staging
is invisible. Publish only after the service is ready; if publication fails, withdraw the
service and close the generation. `UiRegistration::replace` validates an entire replacement
before changing the live snapshot. A failed replacement leaves the previous set intact.

A scope owns its registrations even when plugin code drops a local registration handle.
Quiescing or dropping the scope synchronously removes discovery, before draining tracked
work. Explicit unpublication is idempotent. Old handles cannot remove or republish a newer
registration. Captured UI snapshots contain data, without operation leases: a page can
finish rendering while disable proceeds, and an administrator cannot hold disable open
through the page being rendered. Server authorization remains mandatory on every route.

One page captures one shared immutable `Arc` snapshot. All its slots use that snapshot.
Publication changes invalidate the plugin revision used by HTML, navigation preloads and
history. An HTML GET crossing a revision change refreshes instead of caching a mixed shell.
Registration and localization never add per-component SQL, filesystem access or IPC.

## Slots and components

| Slot | Host context |
| --- | --- |
| Global navigation | Public, signed-in or instance administrator |
| Repository tabs | Repository read access, administrator access or the owner plugin's enabled repository policy |
| Repository settings cards | Repository administrator |
| Repository panels | Checked repository context |
| Account settings | Signed-in viewer |
| Organization settings cards | Organization manager or instance administrator |
| Administrator navigation and panels | Instance administrator |

The first component schema supports links, cards with an optional action, notices and
bounded lists. Navigation slots accept links. Components use fixed host icons and styling;
plugins supply text and internal targets. Forms, arbitrary tables, custom CSS, HTML strings,
remote assets and third-party scripts are outside this declarative schema. Native Askama
feature pages retain their existing forms and host security middleware.

Targets are validated internal route templates. Only whole-segment `{owner}` and `{repo}`
parameters are supported, resolved from an already authorized host context and encoded as
individual URL segments. Query strings, fragments, external URLs, traversal and unknown
parameters are rejected. A visible contribution grants no route or permission.

The schema bounds contributions, aggregate bytes, message lengths and list size. IDs are
namespaced by plugin ownership, and order is deterministic by slot, requested order, owner
and ID. Host messages reuse the request's locale snapshot; unknown plugin message namespaces
render their bounded English fallback. Extending installable catalogs with arbitrary
plugin-owned message schemas remains a later localization contract.

## Built-in adoption

Actions registers its repository tab, repository and organization settings cards, account
settings entry and administrator runner navigation. Languages registers its account
language preferences entry. These entries disappear with their provider and return with a
new activation generation. Existing route authorization, repository Actions policy and
retained feature data are unchanged.

## Trusted browser enhancements

The shell advertises the active owner and generation. Roots name an enhancement from a
compiled host allowlist, together with its content-hashed asset URL. The host checks both
identities before loading code. The initial module is the existing Actions job enhancement;
its behavior remains optional and the server-rendered Askama job page works without it.

An enhancement receives a host-owned scope for its signal, current root, listeners, timers,
observers and disposers. Cleanup is idempotent, cancels pending work and isolates disposer
errors. Late asynchronous imports or mounts cannot reactivate an obsolete root. A matching
same-job polling replacement preserves the viewer's open-step choices while callbacks stop
using detached roots. Navigation, history transitions, page suspension and observed plugin
revision changes retire the old behavior.

Dynamic imports evaluate module top-level code before resolving and cannot be canceled.
Reviewed modules keep that top level side-effect free, and allocate all behavior through
`mount` and the host resource context. Stale imports cannot mount after their scope retires.

Only reviewed, host-shipped modules run in the forge origin. Compiled native plugins are
trusted server code; scoped registration is lifecycle management, not a sandbox. General
installable executable plugins, isolated third-party interactive frames, dynamic route
installation and broader component schemas remain separate stages.

See [plugin core](plugin-core.md), [languages](i18n.md), the
[implementation plan](design/plugin-architecture.md) and
[scoped extension and middleware design](design/plugin-extensions.md).


## Performance checks

The normal request path takes shared immutable snapshots; empty slots allocate no component
vectors. Browser mutation observation is installed only while an approved enhancement is
active. Both native pages and shared declarative components are compiled Askama code.
The platform's transport and message/provider dependencies remain independently gated.

Manual microbenchmarks are reproducible with:

```sh
cargo test --release --locked -p rbit-plugin-runtime benchmark_warm_extension_snapshots -- --ignored --nocapture
cargo test --release --locked -p rbit-web --lib askama_extension_rendering_cost -- --ignored --nocapture
```

The Askama comparison renders identical HTML for 16 links. Its plugin side includes
permission checks, target resolution, message fallback and view-model allocation, while the
core comparison starts with prepared links. It measures that incremental work rather than
whole-page or network latency. Timing results depend on compiler mode and hardware; these
manual benchmarks do not impose machine-dependent CI thresholds.
