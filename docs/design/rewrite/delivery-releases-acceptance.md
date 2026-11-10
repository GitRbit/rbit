# Release delivery acceptance

Acceptance evidence for the `delivery.releases` ledger row ("Five binary platforms,
paired runner, containers, checksums and provenance"). Its target, `repository-ci`, is
the repository's own CI configuration rather than a crate, so retiring it does not move
code: `.github/workflows/release.yml` stays the owner, and the rewrite check now fails
if the workflow loses one of the guarantees the row names. Per section 10 of the
[rewrite plan](../ground-up-rewrite-plan.md), the existing pipeline's strengths are
preserved; this slice does not change the release build.

**Status: the row stays `baseline-retained`.** The contract is enforced and most of it
has run in a published release, but the paired runner (`rbit-runner` in every archive,
the `ghcr.io/gitrbit/rbit-runner` image) was added after the last tagged release and
has not yet run through `release.yml`. The row becomes `ported` once the first tag of
the current workflow (the 0.2.0 release, or a `v0.2.0-rc.N` pre-release) passes the
remaining gate below.

## Scope

| Guarantee | Where | Enforced by |
| --- | --- | --- |
| Five binary platforms: Linux x86_64/aarch64 (glibc 2.35), macOS aarch64/x86_64, Windows x86_64 | `binaries` matrix | contract check: exactly these five targets |
| Paired runner: every archive holds `rbit` and `rbit-runner` and their example config and unit files | `binaries` build and package steps | contract check |
| Upgrade rehearsal of each Linux release binary before packaging | `binaries` | contract check; CI `Release build` runs the same script on every PR |
| Checksums: `SHA256SUMS` over every archive, release created for the verified tag | `release` | contract check |
| Containers: `rbit` and `rbit-runner`, linux/amd64 and linux/arm64, from the released binaries, same tags, `latest` only for final releases | `image`, `runner-image` | contract check; CI `Runner image` builds and starts the runner image on every PR |
| Provenance: BuildKit provenance `mode=max` and SBOM on both images; exact source by tag | `image`, `runner-image` | contract check |
| Tag equal to the workspace version, notes from `CHANGELOG.md` | `prepare` | contract check |
| Trust boundary: tag-only trigger, read-only default token, write scopes only in the three publishing jobs, no compiler caches, every action pinned to a commit, never cancelled half-way | whole workflow | contract check; `rewrite-ci-cache.test.mjs` (caches) |

## Boundary that fails if a guarantee is lost

`scripts/rewrite/release-retirement.mjs`, run by `scripts/rewrite/check.mjs` (and so by
the `Rewrite contracts` CI job on every pull request), reads the release workflow and
reports each lost guarantee by name. `scripts/tests/release-retirement.test.mjs` takes
the real workflow and removes or weakens one guarantee at a time: a dropped platform,
`rbit-runner` missing from the build or the archive, the rehearsal skipped, no
`SHA256SUMS`, no `--verify-tag`, image provenance off, the runner image built from
the wrong file or with different tags, a `pull_request` or
`workflow_dispatch` trigger, a compiler cache, a write default token or an extra write
scope, an unpinned action, cancellation. Each must fail; the current file must pass.

The check is textual: it reads the workflow's own two-space job layout rather than a
YAML parser (the repository has no YAML dependency in its scripts). It does not prove
that GitHub runs the steps as written; the evidence below does.

## Evidence

Published release `v0.1.5` (tag commit `b4c36263`, release run `40238642767`, all nine
jobs successful):

- Assets: five archives (`x86_64`/`aarch64` Linux, `aarch64`/`x86_64` macOS, `x86_64`
  Windows zip) and `SHA256SUMS`.
- `ghcr.io/gitrbit/rbit:0.1.5` is a multi-platform index with `linux/amd64` and
  `linux/arm64` images and one attestation manifest each (provenance and SBOM), read
  from the public registry. Tags `0.1.0`–`0.1.5`, `0.1` and `latest` exist; no `0`
  tag, as the `v0.` rule intends.

Since `v0.1.5` the workflow gained `rbit-runner` in every build, smoke test and archive,
the runner's deployment files, the Linux upgrade rehearsal, `deferred.css` in the
stylesheet artifact, the `runner-image` job and a newer pinned `docker/login-action`
(`git diff b4c36263 -- .github/workflows/release.yml`: +86/−9). These parts are
covered on every pull request only by CI's `Release build` (Linux x86_64 build of both
binaries, `--version` of each, the rehearsal) and `Runner image` (build and start of
`Dockerfile.runner`) jobs. `ghcr.io/gitrbit/rbit-runner` does not exist yet.

Locally: `node scripts/rewrite/check.mjs` reports zero violations and
`node --test scripts/tests/release-retirement.test.mjs` passes 3 of 3 (20 mutations).

## Gates

| Gate | State |
| --- | --- |
| Contract enforced on every pull request | Done: rewrite check and its test |
| Five platforms, checksums, `rbit` image with provenance and SBOM in a real release | Done: `v0.1.5` |
| Paired runner in a real release: five archives each listing `rbit` and `rbit-runner`, `SHA256SUMS` verifying them, `rbit-runner` image index for both platforms with attestation manifests and the same tags as `rbit` | **Remaining**: the first tag of the current workflow |
| Security group (build and release) | Untrusted PR code, artifacts and caches cannot reach publication: tag-only trigger, no caches, artifacts only from the same run, job-scoped writes, pinned actions |

When the remaining gate passes, record the run id here and change the ledger row to
`ported` with `replacement` `.github/workflows/release.yml` and
`scripts/rewrite/release-retirement.mjs` and this file as `acceptance`.

## Not covered

- Binary provenance: archives carry SHA-256 sums but no signed attestation; only the
  images carry provenance. The plan's proposed artifact attestations for binaries,
  archive smoke tests on every platform and download verification are improvements
  for later, not part of this row's baseline.
- Windows and macOS binaries are smoke-tested only where native (`x86_64-apple-darwin`
  is cross-built and not run), as in the baseline.

## Cost

| Measure | Value |
| --- | --- |
| Code commits | 1 (plus this record) |
| Files changed | 3: a 92-line Node check, a 49-line test, two checker lines |
| Release workflow changes | none |

Tricky parts: finding that the last published release predates the paired runner, so
"passing evidence" for the whole row cannot exist until the next tag; keeping the check
free of a YAML dependency while still scoping write permissions to individual jobs.

Release gate change (0.2.0): the workflow no longer requires a successful push CI run on
`main` for the tagged commit. Releases are gated by the maintainer's pre-tag verification
instead; the version and release-notes checks remain.
