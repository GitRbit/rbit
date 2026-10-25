# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for a security problem. Report it privately through
GitHub instead:

1. Sign in to GitHub and [report a vulnerability privately](https://github.com/GitRbit/rbit/security/advisories/new)
   (the repository's **Security → Advisories** tab, then **Report a vulnerability**).
2. Describe the problem, the affected version (`rbit --version`) and configuration, and how
   to reproduce it. A proof of concept helps but is not required.

Reports use GitHub's private advisory workflow; they are not public issues. Maintainers can
invite collaborators to help investigate. No email address is required in the report.
We aim to acknowledge it within a few days,
keep you informed while we work on a fix, and credit you in the advisory and the changelog
unless you prefer otherwise. Please give us reasonable time to release a fix before you
disclose the problem publicly; we will agree on a date with you.

If the direct link fails, open [Security → Advisories](https://github.com/GitRbit/rbit/security/advisories)
and select **Report a vulnerability** after signing in. If the button is missing, open a
public issue saying only that private reporting is unavailable; do not include vulnerability
details, logs, credentials or a proof of concept. A separate private reporting email address
will be published here when it is available.

## Supported versions

Security fixes go into the **latest release** only. Include the version you tested; if you
can safely test the latest release or `main`, tell us whether it is also affected. You do
not need to upgrade a production instance before reporting. Upgrades are designed to be routine:
database migrations run automatically, and the [changelog](CHANGELOG.md) lists every step
that needs attention.

## Scope

In scope: the `rbit` server and binary in this repository (web interface, REST API, Git over
HTTP and SSH, webhooks, background jobs, backups) and the container image
`ghcr.io/gitrbit/rbit`.

Out of scope: problems that need an administrator account or access to the server's data
directory or configuration; deployments that ignore [docs/deployment.md](docs/deployment.md)
(for example no TLS in front of rbit); vulnerabilities in dependencies that rbit's code does
not reach. If you are unsure, report it anyway.

rbit's security model (what it protects, how secrets are stored, how access is checked) is
described in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
