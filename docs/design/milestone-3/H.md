# H — Open-source hygiene and release packaging (notes)

Workstream H of [milestone 3](../milestone-3.md). H owns `README.md` this milestone; the text
below is for the files the integrator owns (`CHANGELOG.md`, `docs/ARCHITECTURE.md`,
`docs/HANDOFF.md`, `docs/deployment.md`). H has no migrations and no configuration section.

## As built

| Item | Where |
|---|---|
| License **MIT OR Apache-2.0**: full texts (copyright "The rbit contributors"), workspace `license` (every crate already inherited it), `package.json` and its lock file (whose root name still said `rustit-web-assets`), OpenAPI `info.license` | `LICENSE-MIT`, `LICENSE-APACHE`, `Cargo.toml`, `package.json`, `package-lock.json`, `crates/rbit-web/src/routes/api/openapi.json` |
| Third-party notice: vendored htmx and its preload extension (both 0BSD, verified against npm metadata and the extension's own LICENSE), the tinted-theming schemes (MIT, `LICENSE.txt` stays), Tailwind's generated CSS, the logo and icons, fonts (none bundled), how Rust dependency licenses are checked | `THIRD-PARTY.md` |
| `cargo deny`: allows exactly the licenses found in the tree for all five release targets (`cargo deny list`), denies yanked crates, one advisory ignored with its reason (below) | `deny.toml` |
| CI: new `deny` job (`cargo deny --locked check licenses advisories`, prebuilt cargo-deny via `taiki-e/install-action`); the existing jobs were reviewed for a first public run | `.github/workflows/ci.yml` |
| Community files: contributing guide (setup, `npm run dev`, gate, layering/design/commit rules), security policy (GitHub private advisories, latest release supported), Contributor Covenant 2.1 (reports to maintainers through GitHub, no email), issue forms + config, PR template, public roadmap | `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `.github/ISSUE_TEMPLATE/*`, `.github/pull_request_template.md`, `ROADMAP.md` |
| Release workflow on `v*` tags (below) | `.github/workflows/release.yml` |
| Dockerfile: OCI labels; `BINARY=prebuilt` variant used by the release image | `Dockerfile` |
| Token prefix `rbit_`; `rit_` still parses, routes as a token and authenticates (same digest) | `crates/rbit-auth/src/access_token.rs`, tokens page, OpenAPI text, docs, tests |
| Rename sweep | test fixtures, comments (below) |
| README: badges, install from release binaries / image / source, community and license sections, token prefix | `README.md` |

### Release workflow

`release.yml` runs on `v*` tags:

1. **prepare** — fails unless the tag equals the `rbit-server` version (`cargo metadata`) and
   `CHANGELOG.md` has a non-empty `## [X.Y.Z]` section (its body becomes the release notes).
   A tag with a `-` suffix is a pre-release; it falls back to `## [Unreleased]`.
2. **assets** — `npm ci && npm run build` once, checks the vendored files are unchanged, and
   hands `app.css` to every build (the stylesheet is embedded at compile time).
3. **binaries** — `cargo build --release --locked --package rbit-server --target …`:

   | Target | Runner | Why |
   |---|---|---|
   | `x86_64-unknown-linux-gnu` | `ubuntu-22.04` | glibc 2.35 baseline: runs on Debian 12, Ubuntu 22.04 and newer |
   | `aarch64-unknown-linux-gnu` | `ubuntu-22.04-arm` | **native Arm runner**, so bundled SQLite, ring, zstd and mimalloc compile with the runner's own gcc — no cross C toolchain, no `cross`/Docker, no QEMU |
   | `aarch64-apple-darwin` | `macos-15` | native |
   | `x86_64-apple-darwin` | `macos-15` | Apple's clang targets x86_64 from Apple silicon out of the box (no smoke test: no Rosetta guarantee) |
   | `x86_64-pc-windows-msvc` | `windows-2025` | native; ring needs no NASM on MSVC |

   Native targets run `rbit --version` as a smoke test. Archives are
   `rbit-<version>-<target>.tar.gz` (`.zip` for Windows) with the binary, `README.md`,
   `CHANGELOG.md`, both licenses, `THIRD-PARTY.md`, `rbit.example.toml` and `rbit.service`.
4. **release** — `SHA256SUMS` over all archives, then `gh release create <tag> --verify-tag
   --notes-file notes.md` (`--prerelease` for pre-release tags).
5. **image** — unpacks the two Linux archives into `dist/linux-{amd64,arm64}/rbit` and runs
   `docker/build-push-action` for `linux/amd64,linux/arm64` with `BINARY=prebuilt`, pushing
   `ghcr.io/gitrbit/rbit:{version}`, `:{major}.{minor}`, `:{major}` (not for 0.x) and `:latest`
   (final releases only), with OCI labels/annotations, provenance and an SBOM.

Why prebuilt binaries in the image: building the arm64 image from source under QEMU with
fat LTO and one codegen unit takes hours; packaging the release binaries is fast, and the
image then contains byte-for-byte the binaries whose checksums are published. The
`prebuilt` stage is `FROM scratch` + `COPY --chmod=0755`; BuildKit skips unused stages, so
the default `docker build .` still compiles from source and never reads `dist/`. Verified
locally: `docker build --check` is clean for both variants, and a prebuilt build with a
stand-in binary skipped the Node/Rust stages and produced an image with the expected labels.

Validation: `actionlint` 1.7.12 is clean on both workflows; every action version used was
checked to exist (checkout v7, setup-node v7, upload-artifact v7, download-artifact v8,
setup-qemu/buildx/login v4, metadata v6, build-push v7, taiki-e/install-action v2);
the runner labels were checked against actions/runner-images (`ubuntu-22.04-arm`,
`macos-15` and `windows-2025` are current, none deprecated). The CHANGELOG extraction was
tested against the real file and a sample with `## [0.1.0] - date` sections.

### Advisory ignored in deny.toml

`RUSTSEC-2023-0071` (Marvin attack, `rsa`, no fixed version exists): a timing side channel
in RSA **private-key** operations. rbit only verifies RSA signatures with public keys
(WebAuthn RS256 passkeys, users' `ssh-rsa` keys); the SSH host key is always Ed25519
(`rbit-ssh/src/host_key.rs` refuses anything else), so no private RSA key exists in rbit.
Revisit if rbit ever signs or decrypts with RSA.

### Rename sweep: what stays and why

- Kept, each with a comment: `rustit.db` / `rustit.redb` fallbacks (`StorageConfig`), the
  `RUSTIT_*` env prefix and `RUSTIT_CONFIG` (`rbit-server`), the `rustit_session` cookie
  (`middleware.rs`), the `rustit-theme` cookie and localStorage key (`themes/mod.rs`,
  `theme.js`), and `/rustit.toml` in `.gitignore` (comment added).
- Kept on purpose: the HTTP header names `X-Rbit-Event`, `X-Rbit-Delivery`, `X-Rbit-Hook-Id`,
  `X-Rbit-Signature-256` and `Rbit-Nav-Epoch` — that is HTTP's Title-Case header style (like
  `X-GitHub-Event`), not a spelling of the name, and the webhook headers are a documented
  interface since the rename. Comments added in `webhooks/delivery.rs` and `nav.js`.
- **Webhook user agent: kept as `rbit-Hookshot/<version>`.** It already writes the name in
  lower case and mirrors GitHub's `GitHub-Hookshot/<id>` (product + `Hookshot`); receivers
  that allow-list user agents may match on it, and it is documented in the CHANGELOG as the
  breaking rename change. Lower-casing it would be a second breaking change for no gain.
- Not edited: the historical comments in migrations `0004`, `0009` (`rustit_core`,
  `rustit_auth`) and `0021` ("Rbit"). sqlx validates migration checksums, so editing an
  applied migration — even a comment — makes every existing database refuse to start.
- Fixed: seven test fixtures that named their temporary databases `rustit.db`/`.redb`.
- `.claude/launch.json` already had no absolute or personal paths: the three configurations
  use `./data`, `data-alt` and `./data-live`, all relative and gitignored. No change needed.

### Token prefix

`AccessTokenSecret::PREFIX = "rbit_"`, `LEGACY_PREFIX = "rit_"`. `parse` and
`looks_like_token` accept either (one prefix only; `rbit_rit_…` is rejected); `encode`
always writes `rbit_`. The digest covers only the 32 random bytes, so nothing in storage
changes and a token works in either spelling; revoking it revokes both. Tests: unit tests in
`access_token.rs`, `tokens_with_the_legacy_prefix_still_authenticate` in
`rbit-services/tests/tokens.rs`, a `rit_` clone in `e2e/tokens.rs` and a `rit_` API call in
`e2e/api.rs`. The token list shows hints as `…a1b2` (it used to print `rit_…a1b2`): the
stored 4-character hint does not record which prefix a token was issued with.

## Gate results

`cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets -- -D warnings`
clean; `cargo test --workspace`: 878 passed, 0 failed (one run hit a timing flake in
`e2e::api::a_hook_created_through_the_api_receives_a_signed_push` — the redelivery was
already `delivering` instead of `pending`; it passes alone and in the two later full runs;
unrelated to H). `cargo deny check licenses advisories`: ok. `npm ci && npm run build`
leaves the tree unchanged.

## CHANGELOG entry

For `## [Unreleased]` (and later the `## [0.1.0] - <date>` section — see the integration
requests):

```markdown
### Open source

rbit is now open source under **MIT OR Apache-2.0** (it was AGPL-3.0-or-later), at
github.com/GitRbit/rbit. Releases ship binaries for Linux (x86_64, aarch64), macOS (Intel,
Apple silicon) and Windows, with SHA-256 sums, and a multi-architecture image at
`ghcr.io/gitrbit/rbit`.

#### Upgrade notes

- **Personal access tokens now start with `rbit_`.** Existing `rit_…` tokens keep working
  unchanged, over Git and the API; nothing needs to be regenerated. Secret scanners and
  scripts that recognise tokens by prefix should accept both. The token list shows hints
  as `…a1b2`.

#### Added

- `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD-PARTY.md`, `CONTRIBUTING.md`, `SECURITY.md`
  (private vulnerability reports through GitHub security advisories), `CODE_OF_CONDUCT.md`
  (Contributor Covenant 2.1), `ROADMAP.md`, issue forms and a pull request template.
- A release workflow: version tags build and publish the binaries, their checksums, release
  notes from this changelog, and the container image.
- `cargo deny` checks dependency licenses and RustSec advisories in CI (`deny.toml`).
- The container image carries OCI labels (source, license, documentation, version).
```

## README

Done in `README.md` (H owns it this milestone). Other workstreams' feature lines and
configuration rows still need to be added by the integrator.

## ARCHITECTURE section text

Replace the token mentions:

- API authentication bullet (≈ line 600): "`Authorization: Bearer rbit_…` (or GitHub's
  `token rbit_…`; tokens issued before 0.1 as `rit_…` are accepted too)."
- Security table, *Access tokens* row (≈ line 790): "`rbit_` + 256 random bits (`rit_` from
  earlier releases is still accepted: the digest covers only the random part); only the
  SHA-256 digest is stored, plus a 4-character hint shown as `…a1b2`. …" (rest unchanged).

New short section (e.g. after "Deployment" or at the end):

```markdown
## Releases and packaging

A `v*` tag runs `.github/workflows/release.yml`: the tag must equal the workspace version
and `CHANGELOG.md` must have a `## [X.Y.Z]` section, whose body becomes the release notes.
The stylesheet is built once and embedded into every binary. Binaries are built natively
(Linux x86_64 and aarch64 on Ubuntu 22.04 runners for a glibc 2.35 baseline, macOS both
architectures on Apple silicon, Windows x86_64), archived with the licenses, docs and
deployment examples, and published with `SHA256SUMS`. The image `ghcr.io/gitrbit/rbit`
(amd64, arm64) packages those same Linux binaries through the Dockerfile's
`BINARY=prebuilt` stage instead of compiling under emulation; a plain `docker build .`
still compiles from source. CI checks licenses and advisories with `cargo deny`
(`deny.toml`); each ignored advisory carries its reason.
```

## HANDOFF text

- §2 item 1 ("CI has never run on GitHub"): still true until the repository is pushed; add
  "The release workflow has never run either: push a `v0.1.0-rc.1` tag first (a pre-release:
  it uses the Unreleased section, does not move `latest`) to exercise the whole pipeline,
  including the ghcr.io push, before the real `v0.1.0`."
- §4 Standards: "Dependencies must pass `cargo deny check licenses advisories`; new licenses
  are added to `deny.toml` deliberately."
- §6 Decisions: "Licensed MIT OR Apache-2.0 (was AGPL). Tokens are `rbit_…`; `rit_…` is
  accepted forever (it costs nothing). The webhook user agent stays `rbit-Hookshot/…` and
  the header names `X-Rbit-*`. Release images package the release binaries
  (`BINARY=prebuilt`), never a second build under QEMU."

## deployment.md text

At the top of "Docker", before the `docker build` example:

```markdown
Released images are published to `ghcr.io/gitrbit/rbit` for `linux/amd64` and `linux/arm64`,
tagged with the version (`0.1.0`), the minor series (`0.1`) and `latest`:

    docker run -d --name rbit --restart unless-stopped -p 3000:3000 -v rbit-data:/data \
      -e RBIT_SERVER__PUBLIC_URL=https://git.example.com ghcr.io/gitrbit/rbit:0.1

Pin a version (or the minor series) in production rather than `latest`. To build the image
yourself, `docker build -t rbit .` compiles from source.
```

In "systemd", before installing the binary: "Download the archive for your platform from the
GitHub release, check it against `SHA256SUMS` (`sha256sum --check --ignore-missing
SHA256SUMS`), and install `rbit` to `/usr/local/bin`; the archive also contains
`rbit.service` and `rbit.example.toml`. Linux binaries need glibc 2.35 or newer (Debian 12,
Ubuntu 22.04, RHEL 10 and later); on older systems build from source."

## Integration requests

1. **Before tagging `v0.1.0`**: turn `## [Unreleased]` in `CHANGELOG.md` into
   `## [0.1.0] - <date>` (keep an empty `## [Unreleased]` above it) and add the H entry
   above — the release job refuses a tag without that section. The workspace version is
   already `0.1.0`.
2. **GitHub repository settings** (owner, after the first push): enable *Private
   vulnerability reporting* (Settings → Code security) — `SECURITY.md` and the issue
   template link to it; enable *Discussions* (linked from the issue chooser and README) or
   drop those two links; after the first image push, make the `rbit` package on ghcr.io
   public and link it to the repository (new packages are private by default).
3. Add the `bug` and `enhancement` labels if the repository does not have GitHub's defaults
   (the issue forms apply them).
4. Apply the ARCHITECTURE, HANDOFF and deployment.md text above, and the CHANGELOG entry.
5. Owner decision to confirm: `THIRD-PARTY.md` states the logo and icons are distributed
   under the project's MIT OR Apache-2.0 terms. If the mascot should instead be reserved
   (a trademark-style "all rights reserved" for the artwork), change that row and the README.

## Deviations

- **Token hints are shown without a prefix** (`…a1b2` instead of `rbit_…a1b2`): the stored
  hint cannot tell `rbit_` from `rit_` tokens, and a migration for it is not worth it.
- **Webhook user agent unchanged** (`rbit-Hookshot/…`, see above) — the contract left the
  choice to H.
- **Release image built from the release binaries** instead of a QEMU source build (see
  above); the Dockerfile's default path is unchanged.
- **Linux binaries are glibc, not musl**: native Arm runners make glibc builds of bundled
  SQLite, ring and mimalloc work without any cross toolchain, which is the most likely to
  succeed on the first run. musl would need `musl-tools` (x86_64) and an aarch64 musl
  cross compiler; it can be added later as extra targets if users ask for static binaries.
- Historical migration comments still say `rustit_*`/`Rbit` (checksums, see above).
