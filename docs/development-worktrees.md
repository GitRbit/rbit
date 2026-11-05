# Worktrees and shared developer builds

Use Node 24 or newer. The repository helper creates new checkouts inside the primary
checkout's `worktrees/` directory and shares compiler output across compatible branches.
It discovers the primary checkout using Git's common directory, including when invoked
from a nested worktree. Existing checkouts are not moved or cleaned.

```sh
git fetch github main
node scripts/workspace.mjs new my-feature github/main
node scripts/workspace.mjs list
```

`new` resolves the supplied revision to an immutable commit and creates
`feature/my-feature`. The default revision is the locally fetched `github/main`;
creation does not silently fetch or select the current PR. Names accept lowercase
letters, digits and hyphens. Checkouts must remain below `worktrees/`.

## Builds, checks and tests

Run these from the checkout whose source you want to build:

```sh
node scripts/workspace.mjs check --workspace --locked
node scripts/workspace.mjs clippy --workspace --all-targets --locked -- -D warnings
node scripts/workspace.mjs test --workspace --locked
node scripts/workspace.mjs nextest run --workspace --locked
node scripts/workspace.mjs build --package rbit-server --package rbit-runner --locked
node scripts/workspace.mjs run --release -- serve
npm run dev
```

Both `CARGO_TARGET_DIR` and `CARGO_BUILD_BUILD_DIR` point into
`.build-cache/<compiler-and-flags-key>/shared/`. The second override matters because
the checked-in Cargo configuration otherwise puts intermediates in each checkout's
`.cargo-build`. SQLite's `SQLITE_ENABLE_QPSG` flag is retained. Cargo still fingerprints
features, dependencies, targets and profiles; sharing a directory is not a promise that
every worktree path will reuse every object. Compiler/host/explicit compiler flag changes
select a different compatibility namespace, not a per-branch directory.

One repository-wide lease serializes builds, checks and tests through completion. This
is deliberately one lane, rather than an unlimited target directory per worktree.
Checks that need independent parallel capacity must use an explicitly budgeted future
lane policy. Commands accept normal Cargo arguments except manifest, output-directory,
message-format and arbitrary config overrides, which could escape shared storage.
`--locked` is caller controlled: use it for verification; omit it only when intentionally
updating the lockfile.

When adding local workspace members, run a normal `check` or `test` once without
`--locked`, review the minimal lockfile change, then return to locked verification.
`node scripts/workspace.mjs generate-lockfile` is available for intentional complete
lock regeneration; it can refresh the entire dependency resolution and should not be
used merely to register new local crates.

A successful build reads executable paths from Cargo JSON, copies those exact outputs
while holding the lease, and reports generation-specific paths under
`.worktree-state/<checkout-id>/bin/<generation>/`. A failed build never returns a prior
binary. Run copies keep Windows compiler output relinkable while another server runs.
Runtime markers preserve running generations; cleanup retains the three most recent
inactive copies and live handoffs. The CLI prints only intentional build metadata,
never the inherited environment, which may contain credentials.

## Development runtime isolation

`run` and `npm run dev` use independent `.worktree-state/<checkout-id>/data`, stable
localhost HTTP ports allocated from 3100–3999, and log-mode mail by default. They do not
copy the current or production database. The chosen port is stored in
`.worktree-state/ports.json`. `dev` builds with the same lease, launches the private
generation, and retains the previous server if compilation fails. CSS/JS watchers and
debug assets stay in the source checkout.

Explicit `RBIT_STORAGE__DATA_DIR`, `RBIT_SERVER__BIND`,
`RBIT_SERVER__PUBLIC_URL` and `RBIT_MAIL__TRANSPORT` override these defaults.
When overriding the bind address, set the matching public URL too. External programs
can still claim a port between allocation and bind; a server bind error remains visible.
Runtime state and data are intentionally retained after checkout removal. Port registry
entries are currently retained as well; operators can remove an unused entry after
confirming its runtime is stopped.

## IDEs and direct Cargo commands

Do not point direct Cargo invocations at the helper's shared lane: Cargo's internal
lock does not cover the helper's executable-copy interval or the full test run. All
writers to that lane must use the helper. Existing direct commands still follow Cargo's
local defaults and can create local output; use the supported helper commands instead.

For rust-analyzer, configure both compilation entry points to enter the same lease.
Replace the script path below with the path in the checkout being edited:

```json
{
  "rust-analyzer.check.overrideCommand": [
    "node", "/absolute/checkout/scripts/workspace.mjs", "ide-check",
    "--workspace", "--all-targets"
  ],
  "rust-analyzer.cargo.buildScripts.overrideCommand": [
    "node", "/absolute/checkout/scripts/workspace.mjs", "ide-check",
    "--workspace", "--all-targets"
  ]
}
```

`ide-check` forwards Cargo's JSON diagnostics and build-script/artifact records while
holding the lease. Metadata-only Cargo commands do not produce compiler trees. Verify
your IDE's logs after changing settings; do not silently add a separate target per IDE
window. Nested worktrees inherit parent Cargo configuration; both directory environment
overrides are always set by the helper, while other project configuration remains intact.

## Cache reports and safe removal

```sh
node scripts/workspace.mjs cache-report
node scripts/workspace.mjs cache-prune 21474836480
node scripts/workspace.mjs remove my-feature
```

`cache-prune` takes a maximum total byte count and removes least-recently-used whole
compatibility namespaces until the cache is within that high-water mark. It refuses
an active lease, linked paths or symlink/reparse descendants. It never deletes source
checkouts, runtime data, shared Cargo downloads or unknown entries. Reports sum logical
file bytes, not filesystem allocation or external tool caches. Pruning is explicit;
schedule it for your disk budget. One namespace may grow beyond the selected budget
until the next maintenance run; no automatic eviction interrupts a build.

`remove` refuses the current checkout, dirty/untracked files, registered locks and active
runtime markers. Git performs the actual removal without `--force`; branches, private
runtime data and shared cache remain. The helper does not remove legacy checkouts above
the repository or under old target directories. Coordinate their migration separately.

Never use a direct `cargo clean` against a shared lane. Cache maintenance must enter the
same repository lease. Normal command failures release the lease. A hard crash leaves a
stale lease and intentionally blocks new builds: inspect the recorded host/PID and any
orphan Cargo/rustc descendants before manually removing only `.build-cache/.lease`.
Do not steal a lease just because its parent died; orphaned compilers may still write.
PID reuse/unknown hosts fail closed. No automatic stale-lock deletion is performed.

## Verification and remaining integrations

```sh
node --test scripts/tests/workspace-build.test.mjs scripts/tests/workspace-lifecycle.test.mjs
```

The tests use disposable directories and temporary Git repositories. They cover
independent-process exclusion, failed build recovery, dual directories and exact
handoff, containment/junction protection, dirty/active worktree guards, LRU cache pruning,
port isolation and live generation retention. They do not compile the full Rust workspace.

The performance launcher accepts exact executable paths or helper build records and
retains the fixture paths, binary hash and source metadata. It never compiles during a
measurement. Save build commands/logs and explicitly retain binaries needed for later
reproduction; inactive runtime generations are disposable. CI may continue using isolated runner storage;
do not blindly share a developer cache between trusted and untrusted CI jobs. A globally
installed IDE or external shell remains outside the helper's enforcement boundary.
