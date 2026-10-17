# Phase 2 — Organisations, administration, insights, settings

This is the working contract for the phase-2 build. It is split into four workstreams that
are built in parallel; each owns a disjoint set of files. Shared registration points
(service fields, store accessors, route merges, view modules) are pre-wired with stubs so no
two workstreams edit the same file.

## Product scope

| Area | What ships |
|---|---|
| **Languages** | Forgejo-style language breakdown: segmented bar + legend per repository, computed from the default branch, excluding vendored/generated/documentation files, honouring `.gitattributes` `linguist-*` overrides. Cached in redb by commit id (content-addressed, never stale). |
| **Last commit per entry** | Tree view shows each entry's latest commit message and age, computed by a bounded history walk and cached in redb. |
| **Organisations** | Create orgs, org profile (repositories, members), member management with roles (`owner`, `member`), org settings (display name, description, delete), repositories owned by orgs, owner picker on "New repository", orgs on the dashboard, explore search + sort. |
| **Administration** | `/admin` panel for site admins: dashboard (counts, versions, disk usage, uptime), users (search, promote/demote, suspend/unsuspend, delete), organisations, repositories (delete), **instance settings editable at runtime** (site name, registration open/closed, who may create organisations, default repository visibility). |
| **User settings** | Profile (display name, bio, website, location), account (change email, change password). Settings shell already exists (`/settings/tokens`). |
| **Repository settings** | `/{owner}/{repo}/settings`: description, visibility, default branch, rename, delete (type the name to confirm). |
| **Stars** | Star / unstar (htmx toggle), star count in repository header and cards, stargazers list. |

## Runtime instance settings

The TOML/env config (`rbit_core::Config`) supplies *defaults*. Admins override them at
runtime; overrides live in SQLite (`instance_settings` key/value table) and are cached in
memory by `InstanceService` (`Arc<RwLock<InstanceSettings>>`), so reads are synchronous and
lock-cheap and every page can use them.

```rust
pub struct InstanceSettings {
    pub site_name: String,
    pub registration: RegistrationMode,          // Open | Closed
    pub org_creation: OrgCreationPolicy,         // AnyUser | AdminsOnly
    pub default_visibility: Visibility,
}
impl InstanceService {
    pub fn current(&self) -> InstanceSettings;                          // sync snapshot
    pub async fn update(&self, actor: &User, new: InstanceSettings) -> Result<(), ServiceError>; // admins only
}
```
The first account can always be created (bootstrap rule) regardless of `registration`.

## Permissions

| Actor | Personal repo | Org repo |
|---|---|---|
| Site admin | Admin | Admin |
| Owner (user) | Admin | — |
| Org owner | — | Admin |
| Org member | — | Write |
| Anyone else | Read if public | Read if public |

Suspended users cannot sign in, use sessions, use access tokens, or use Git.

## Migrations (numbers are reserved per workstream)

| File | Workstream | Contents |
|---|---|---|
| `0003_organizations.sql` | Orgs | org metadata (`org_profiles`: description, website), indexes |
| `0004_administration.sql` | Admin | `instance_settings(key, value)`, `user_logins.suspended_at` |
| `0005_profiles_and_stars.sql` | Settings | `account_profiles(account_id, website, location)`, `stars(account_id, repo_id, created_at)` |
| `0006_insights.sql` | Languages | only if relational data is needed (cache lives in redb) |

## Workstream file ownership

Pre-wired stubs (created by the integrator) are marked ★. A workstream may edit only its own
files; anything else is requested from the integrator.

**A — Insights (languages, last commit per entry)**
`rbit-git/src/lib.rs`, `rbit-git/src/languages/**`, `rbit-git/src/last_commit.rs`,
`rbit-storage/src/kv/**`, `rbit-services/src/insights.rs`★,
`rbit-web/src/routes/repo.rs`, `rbit-web/src/views/repo.rs`, `rbit-web/src/views/insights.rs`★,
`templates/repo/tree.html`, `templates/repo/latest_commit.html`.

**B — Organisations**
`rbit-core/src/{names.rs,account.rs,repository.rs}`, `rbit-auth/src/policy.rs`,
`rbit-storage/src/sqlite/{orgs.rs★,repositories.rs}`, migration 0003,
`rbit-services/src/{orgs.rs★,repositories.rs}`,
`rbit-web/src/routes/{orgs.rs★,account.rs,home.rs,repo_create.rs★}`,
`rbit-web/src/views/{org.rs★,account.rs,home.rs,components.rs}`,
`templates/org/**`, `templates/account/**`, `templates/home/**`, `templates/repo/new.html`, `templates/macros.html`.

**C — Administration & instance settings**
`rbit-core/src/config.rs`, `rbit-core/src/instance.rs`★,
`rbit-storage/src/sqlite/{instance.rs★,accounts.rs,admin.rs★}`, migration 0004,
`rbit-services/src/{instance.rs★,admin.rs★,accounts.rs,tokens.rs}`,
`rbit-web/src/{extract.rs,middleware.rs}`, `rbit-web/src/views/{layout.rs,admin.rs★}`,
`rbit-web/src/routes/{admin.rs★,auth.rs,git_http.rs}`, `templates/admin/**`, `templates/auth/**`.

**D — User settings, repository settings, stars**
`rbit-git/src/storage.rs`, `rbit-storage/src/sqlite/{profiles.rs★,repo_settings.rs★,stars.rs★}`, migration 0005,
`rbit-services/src/{profiles.rs★,repo_settings.rs★,stars.rs★}`,
`rbit-web/src/routes/{settings.rs,repo_settings.rs★,stars.rs★}`,
`rbit-web/src/views/{settings.rs,repo_settings.rs★}`,
`templates/settings/**`, `templates/repo/{settings*.html,subnav.html,nav_context.html}`.

**Integrator (not a workstream):** `Cargo.toml`s, `rbit-services/src/lib.rs`,
`rbit-storage/src/{lib.rs,sqlite/mod.rs,sqlite/rows.rs}`, `rbit-web/src/routes/mod.rs`,
`rbit-web/src/views/mod.rs`, `templates/base.html`, `styles/app.css`, `docs/ARCHITECTURE.md`.
Workstreams put new row types in their own store files, not `rows.rs`. If a workstream
needs a new CSS component it describes it in its report; the integrator adds it.

## Engineering rules (all workstreams)

- Match the existing code: `//!` module docs, self-describing names, small modules, validated
  domain types, thin handlers → services → stores.
- Clippy **pedantic** clean; `unsafe` forbidden; no `unwrap` outside tests.
- sqlx 0.9: SQL must be `'static` (literals or `concat!` macros).
- Askama 0.16: `{% import "macros.html" as ui %}` in every template using macros; pass
  literals to `ui::icon`; enum matching uses `{% when Enum::Variant(x) %}`.
- Strict CSP: no inline `style=` or scripts. For proportional widths use SVG attributes.
- Design: monochrome minimal — `.box`, `.row`, `.btn`, `.btn-primary`, `.tab`, `.badge`,
  hairline borders, 13–14px type, colour only for status (and language colours in the
  language bar, which follow GitHub Linguist so they are recognisable).
- Forms: validation errors re-render with 422; success redirects 303. Private things are 404.
- Tests: unit tests for logic, integration tests against a temp SQLite DB / temp Git repo.
