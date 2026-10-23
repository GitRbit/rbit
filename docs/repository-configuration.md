# Public repository configuration

The source upstream is [GitRbit/rbit](https://github.com/GitRbit/rbit). GitRbit is a
personal GitHub account, not an organization. Repository files and live GitHub settings
both need to be maintained: a `SECURITY.md` file alone does not enable private reporting.

## Audit and expected settings

Run `npm run audit:repository` with Node.js 24. It makes read-only requests and does not
submit test issues, discussions or vulnerability reports. It checks public metadata,
the security reporting button, anonymous submission entry points, issue labels, active
default-branch rules, release targets, published checksums and anonymous container access.
GitHub sign-in is expected
when an anonymous visitor opens a submission form. An authenticated researcher must
still verify that the private form opens; do not submit a fabricated vulnerability.

Run `npm run audit:repository -- --admin` with an administrator's `GH_TOKEN` supplied in
the environment to also verify secret scanning, push protection, Dependabot alerts and
security updates, and workflow-token defaults. Never commit credentials or paste them
into issues. Public checks cannot verify private administrator settings or delivery of
notifications. Network errors and permission failures fail the audit rather than count
as passing checks.

Expected live settings:

| Area | Required configuration |
| --- | --- |
| Visibility | Public; default branch `main`; Issues and Discussions enabled |
| About | Description and topics for rbit; homepage `https://rbit.ori.click` |
| Security reports | Private vulnerability reporting enabled; policy and issue chooser link to `/security/advisories/new` |
| Main branch | Active rules prevent deletion and force pushes, require linear history, a pull request and resolved review threads |
| Required checks | All six CI jobs, produced by GitHub Actions; branch must be up to date |
| Merge methods | Squash or rebase; merge commits disabled; merged branches deleted |
| Secrets | Secret scanning and secret scanning push protection enabled |
| Dependencies | Dependabot alerts and security updates enabled; weekly Cargo, npm and Actions version updates |
| Actions | Read-only token default; Actions cannot approve pull requests; external actions pinned to commit SHAs |
| Release | `v*` tags protected against deletion and force updates; tag matches workspace version, nonempty notes, successful push CI on the exact commit on `main` |
| Downloads | Five platform archives plus SHA256SUMS available anonymously |
| Container | `ghcr.io/gitrbit/rbit` publicly pullable for linux/amd64 and linux/arm64; package visibility checked separately |

Required checks are `Format and clippy`, `Licenses and advisories`,
`Tests (ubuntu-latest)`, `Tests (windows-latest)`, `Front-end build` and `Release build`.
If CI job names change, update the live rules and `scripts/audit-repository.mjs` together.
No approving-review minimum is imposed while the project has a single maintainer; all
changes still require a pull request and green checks, with no configured ruleset bypass.

The weekly Repository health workflow checks public configuration, Rust dependency
advisories and high-severity npm advisories even when no code is pushed. It also supports
manual dispatch. Its token does not have administrator privileges, so it does not run
the administrator-only checks. Dependabot maintains pinned action references.

## Reporting and maintainer notifications

Researchers sign in to GitHub and select **Security → Advisories → Report a vulnerability**.
Private reporting uses GitHub's standard form for summary, details, reproduction and
impact. An email address is not required in the report. Investigations stay in that
private advisory until disclosure is coordinated; access can include invited collaborators.

Each responsible maintainer must enable repository notifications for **Security alerts**
(Watch → Custom) or **All Activity**, and check their GitHub notification inbox. Email
delivery is optional and follows personal notification preferences. Configuration checks
cannot prove that a human receives, reads or responds to a report. See
[GitHub's reporting and notification instructions](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository).

The `GitRbit` owner account was subscribed to **All Activity** during this audit, with
notifications not ignored. This establishes a GitHub notification subscription without
publishing an email address. The administrator audit verifies that the account running
it is subscribed; additional maintainers must configure their own subscriptions.

If reporting is unavailable, a public issue may report only the broken reporting path;
vulnerability details must stay private. No alternative private address is published yet.

## Audit findings, 2026-10-22

Fixed in live settings: disabled private reporting; an inactive, empty-target branch
ruleset; disabled secret scanning, push protection, Dependabot alerts and security
updates; and a homepage missing its URL scheme. Added release-tag protection and subscribed
the owner account to repository notifications. Verified that the public advisories page
now exposes **Report a vulnerability**, Issues and Discussions are enabled, issue labels
exist, the latest release is `v0.1.3` with five archives and checksums, and anonymous GHCR
access exposes both Linux architectures and provenance/SBOM manifests.

Source changes add the repeatable audit, scheduled health checks, Dependabot configuration,
action pinning and the release CI gate. These take effect after their pull request merges.
No runtime data or local configuration files are tracked at the known data paths.

The Code of Conduct previously linked to a nonexistent organization member directory.
There is currently no dedicated private conduct-reporting channel. The owner plans to
publish a private reporting email later; update `CODE_OF_CONDUCT.md` and `SECURITY.md`
when it is available. Security advisories remain reserved for software vulnerabilities.
Maintainer notification delivery and authenticated researcher form access still need
human verification. This is a repository configuration audit, not a full application
security review or a guarantee that the source contains no vulnerabilities.
