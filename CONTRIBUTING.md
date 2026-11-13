# Contributing to rbit

Thanks for helping. rbit is a self-hosted Git platform in Rust; this page explains how to
build it, how to test it, and what a change needs before it can be merged.

By contributing you agree that your work is licensed under the project's terms: **MIT OR
Apache-2.0**, at the user's option (see [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE)). Everyone taking part follows the
[Code of Conduct](CODE_OF_CONDUCT.md). Security problems go through a private advisory, not
an issue — see [SECURITY.md](SECURITY.md).

## Before you start

- **Bugs:** open an issue with the bug report form: what you did, what happened, what you
  expected, and the version (`rbit --version`).
- **Features:** open an issue (or a discussion) first for anything larger than a small fix,
  so we can agree on the shape before you spend time on it. [ROADMAP.md](ROADMAP.md) lists
  what is planned and what is deliberately out of scope.

## Setting up

You need Rust 1.94 or newer, Git, and Node.js 24 for asset builds and developer tooling;
nothing from Node ships in the binary. The SSH tests also use the
OpenSSH client and skip when it is missing.

```bash
npm ci
npm run build          # vendors dependencies and builds the frontend assets
node scripts/workspace.mjs fetch --locked  # cold dependency bootstrap under the shared lease
node scripts/workspace.mjs run -- serve
```

The source guard uses offline metadata before compilation. Run the locked fetch again
when a changed lockfile requires registry sources that are not cached; it does not
approve compiler outputs or bypass source validation.

The first account you create becomes the site administrator. For day-to-day work the live
development server rebuilds and restarts on every change to Rust sources, templates or
manifests, and keeps the stylesheet compiled:

```bash
npm run dev            # prints this checkout's URL and isolated data directory
```

Developer builds share one guarded compiler-output lane across Git worktrees. Each
server runs an exact executable copy with its own data under `.worktree-state/` and a
stable HTTP port from 3100–3999. Existing data is not copied. Mail defaults to log mode
and SSH defaults to disabled; production defaults are unchanged.

Use [the worktree guide](docs/development-worktrees.md) to create checkouts under
`worktrees/`, configure your IDE, inspect cache usage and stop managed process trees
safely. Use the printed development URL rather than assuming port 3000. Set environment
overrides with your shell; for example, in a POSIX shell:

```bash
RBIT_SSH__ENABLED=true RBIT_SSH__BIND=127.0.0.1:3222 RBIT_SSH__PUBLIC_PORT=3222 npm run dev
RBIT_WEBHOOKS__ALLOW_PRIVATE_ADDRESSES=true npm run dev
```

## The quality gate

Run these local checks before submitting. Run them one after another: the end-to-end
suite boots real servers, and the helper serializes shared compiler-output access.

```bash
npm ci && npm run build          # must not change tracked frontend assets
cargo fmt --all --check
node scripts/workspace.mjs clippy --workspace --all-targets --locked -- -D warnings
node scripts/workspace.mjs test --workspace --locked
npm test                        # browser behavior regressions
```

Clippy runs with the **pedantic** group enabled workspace-wide, and warnings fail the build.
`rustfmt.toml` pins the formatting. CI also runs `cargo deny check licenses advisories`
(see [deny.toml](deny.toml)): a new dependency must use a license from its allow list.

The tests are unit tests next to the code, integration tests under each crate's `tests/`,
property tests, and an end-to-end suite (`crates/rbit-e2e`) that boots servers on ephemeral
ports and drives them with an HTTP client and the real `git` CLI. Run one crate's tests with
`node scripts/workspace.mjs test -p rbit-services --locked`, or one test with
`node scripts/workspace.mjs test -p rbit-e2e --locked -- pulls::`.

CI runs the tests with [cargo-nextest](https://nexte.st), which can run test binaries in
parallel. Install it with `cargo install cargo-nextest --locked`, then run
`node scripts/workspace.mjs nextest run --workspace --locked`. Each crate's
integration tests are modules of one binary (`tests/main.rs`), so a new test file needs a
`mod` line there.

To exercise full upstream repositories against the development server, run
`npm run test:realworld -- http://localhost:3100`, replacing the example origin with the
development server's printed URL. It loads the real rbit repository and
Git's full history, verifies HTTP push/clone integrity and checks repository browsing.
See [the performance harness](scripts/perf/README.md#real-repositories) for setup,
generated credentials, reports and the separate synthetic scrolling workload.

## How the code is organised

[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) maps the crates, storage, routes, events, API,
security model and design system. The newer `rbit-identity`, `rbit-access`,
`rbit-repositories`, `rbit-collaboration`, `rbit-actions`, `rbit-wikis` and other domain
crates are being extracted from the original layers. Follow the ownership documented by
the code you change. [Actions](docs/actions.md), [the CLI](docs/cli.md) and
[localization](docs/design/localization-contracts.md) have their own guides.
The rules that keep changes coherent:

- **Layering:** handlers stay thin; application decisions live in domain services; SQL
  lives in stores; templates present validated data. Respect the current domain boundaries
  rather than adding new decisions to compatibility facades.
- **Events:** use the existing event contracts for cross-cutting reactions such as webhooks
  and notifications; check the current owner before extending an older `EventBus` path.
- **Types:** validate input once, into domain types (`rbit-core`), and pass those around.
  `unsafe` is forbidden; no `unwrap` outside tests; every module starts with a `//!` doc
  comment saying what it is for.
- **Migrations** are append-only: never edit a released migration (SQLite checks their
  checksums). New tables are `STRICT`, timestamps are Unix seconds, foreign keys say what
  happens `ON DELETE`. `crates/rbit-storage/tests/upgrade.rs` must keep passing.
- **Security defaults:** secrets are stored only as digests (webhook secrets are the one
  documented exception), anything a viewer may not know exists answers 404, forms are
  Origin-checked, and anything that verifies a secret is throttled.
- **Front end:** server-rendered Askama templates plus htmx — no JavaScript framework. The
  Content Security Policy is strict: no inline `style=` attributes or inline scripts.
- **Design:** monochrome and minimal. Reuse the existing components (`.box`, `.row`,
  `.btn`, `.input`, `.badge`, `.tab`); colour is for status, diffs, syntax highlighting and
  the language bar only. Pages are mobile first; check them at 375 px wide.

## Commits and pull requests

- **Atomic commits:** each commit builds, is formatted, lint-clean and passes the tests on
  its own. Split unrelated changes; history on `main` is linear (rebase, no merge commits).
- **Messages:** an imperative subject of at most 72 characters ("Add …", "Fix …", not
  "Added …"), a blank line, then a body that explains what changed and *why*.
- **Attribution:** commit authors and coauthors must be people who contributed. Do not
  add AI tools, models, agents or their vendors as authors or `Co-Authored-By` trailers.
- **Tests:** a bug fix comes with a test that fails without it; a feature comes with tests
  at the level where its behaviour lives (store, service, route, end to end).
- **Docs:** user-visible changes update the README, `docs/` or the example configuration,
  and add a line under "Unreleased" in [CHANGELOG.md](CHANGELOG.md) — including upgrade
  notes when a migration or a default changes.

Fill in the pull request template; a reviewer will look at it once CI is green.

## Preparing a release

The public upstream repository is [GitRbit/rbit](https://github.com/GitRbit/rbit).
If your checkout already has another `origin`,
add GitHub separately rather than replacing it:

```bash
git remote add github https://github.com/GitRbit/rbit.git
git push github main
```

Before publishing:

- Review the files and commit history being published for credentials, private
  addresses and local runtime data. Review and commit intended working-tree changes;
  a push includes committed history only. Do not upload local audit logs or data directories.
- Run the quality gate above and require a green GitHub CI run on the exact commit being
  released. CI's release-build artifact can be used for a Linux smoke test before tagging.
- Set the workspace version in `Cargo.toml` and update `Cargo.lock`. Move the relevant
  changelog entries into a nonempty `## [X.Y.Z]` section, with upgrade instructions.
  Review the notes against the exact release commit before pushing its tag.
- Check the README, deployment instructions, examples, licenses and third-party notices
  against the binaries and container image that will ship.
- Verify private vulnerability reporting and the reporting link in `SECURITY.md`.
  Run `npm run audit:repository` to check public repository settings, submission paths,
  branch rules and release assets. For administrator-only security settings, run
  `npm run audit:repository -- --admin` with a suitably authorized `GH_TOKEN` in your
  environment. Never put a token in command arguments or committed files.
  [docs/repository-configuration.md](docs/repository-configuration.md) records the expected
  configuration, notification setup and remaining community contact work.
- Review container package visibility separately: making the repository public does not
  by itself establish anonymous access to `ghcr.io/gitrbit/rbit`.

Pushing a `vX.Y.Z` tag to GitHub starts `.github/workflows/release.yml` immediately. It
publishes a GitHub release, platform archives, SHA-256 checksums and the container image;
it is not a dry run. Tag only the reviewed commit after CI passes:

```bash
git tag -a vX.Y.Z <reviewed-commit> -m "Release X.Y.Z"
git push github refs/tags/vX.Y.Z
```

A prerelease tag must match the workspace version too (for example `0.1.0-rc.1` and
`v0.1.0-rc.1`). Prereleases can use the `Unreleased` notes and do not move the container's
`latest` tag. After the workflow completes, verify archive checksums, each supported
binary's `--version`, and a fresh container start. Confirm anonymous downloads and pulls
for the published release and package.
