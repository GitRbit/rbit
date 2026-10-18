# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for a security problem. Report it privately through
GitHub instead:

1. Go to [github.com/GitRbit/rbit/security/advisories/new](https://github.com/GitRbit/rbit/security/advisories/new)
   (the repository's **Security** tab, then **Report a vulnerability**).
2. Describe the problem, the affected version (`rbit --version`) and configuration, and how
   to reproduce it. A proof of concept helps but is not required.

Only the maintainers and you can see the report. We aim to acknowledge it within a few days,
keep you informed while we work on a fix, and credit you in the advisory and the changelog
unless you prefer otherwise. Please give us reasonable time to release a fix before you
disclose the problem publicly; we will agree on a date with you.

## Supported versions

Security fixes go into the **latest release** only. Please upgrade to it before reporting,
and check whether the problem still exists on `main`. Upgrades are designed to be routine:
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
