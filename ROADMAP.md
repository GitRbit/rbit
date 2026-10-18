# Roadmap

rbit aims to be a fast, self-hosted Git platform that one person can run on a small server:
one binary, no external services, GitHub-compatible where that helps (API shapes, webhooks,
`fixes #12`), and secure by default. This page says what exists, what is being built next,
and what is deliberately left out. Dates are not promised; priorities change with feedback —
open a [feature request](https://github.com/GitRbit/rbit/issues/new?template=feature_request.yml)
to argue for something.

## Done

- **Foundation:** accounts, repositories, browsing with syntax highlighting, Git over smart
  HTTP (protocol v2), an installable progressive web app.
- **Everyday Git hosting:** commit diffs, branches and tags, personal access tokens with
  scopes, organisations, stars, language statistics, repository settings, an administration
  panel with runtime settings.
- **Safe to self-host:** rate limiting, two-factor authentication (passkeys and TOTP),
  session management, an audit log, email verification and password recovery, a built-in
  SSH server, background maintenance, Prometheus metrics, health checks, consistent backups,
  Docker and systemd deployment.
- **Access and collaboration:** collaborators, organisation teams, base permissions,
  repository transfer with redirects from old names.
- **Repository essentials:** releases with files, archives, blame, compare, forks, pull
  mirrors, README links and images.
- **Issues and pull requests:** one shared timeline; labels, milestones, assignees,
  references and closing keywords, search, in-app notifications; pull requests within a
  repository or across forks, line comments, reviews, mergeability checks, merge / squash /
  rebase.
- **Branch protection:** no force pushes or deletion, required pull requests and approvals,
  restricted pushers — enforced over HTTP, SSH and for merges.
- **Automation:** a versioned REST API (`/api/v1`, OpenAPI 3.1) and signed, GitHub-shaped
  webhooks with a retrying delivery queue.
- **Themes:** light, dark and about 500 tinted-theming schemes.

From the "ready to open-source" milestone:

- **Open-source packaging:** MIT OR Apache-2.0, community files, release binaries for
  Linux, macOS and Windows, and a multi-architecture image at `ghcr.io/gitrbit/rbit`.
- **Commit statuses and required checks:** a GitHub-compatible status API that any external
  CI can post to, shown on pull requests and commits, and a "require status checks" rule for
  branch protection.
- **REST API continuation:** collaborators, teams, releases (with assets) and forks.
- **Email notifications** on top of the in-app inbox, with per-user preferences and
  one-click unsubscribe.
- **Editing files in the browser:** create, edit, rename, delete and upload files, committing
  to the branch or to a new branch with a pull request.
- **Git LFS:** the batch API, uploads and downloads, and file locking.
- **External sign-in:** OpenID Connect providers and GitHub OAuth.

## Next — finishing the "ready to open-source" milestone

Being built now:

- **Pull request polish:** resolving review threads, changing a pull request's base branch,
  cross-repository `/issues` and `/pulls` dashboards, a Markdown preview, redirects for
  renamed accounts.
- **Migration:** importing repositories with their issues, pull requests and releases from
  GitHub and Gitea/Forgejo, and push mirrors.

Then the first public release, `v0.1.0`.

## Later

Ideas with no schedule yet:

- A runner protocol on top of the status API, so external CI runners can pick up jobs from
  rbit (rbit itself still would not run builds).
- A native gitoxide transport for `receive-pack`, behind a configuration switch, so pushes
  no longer need the `git` binary.
- Email digests.
- Archived (read-only) repositories.
- Git LFS over pure SSH (`git-lfs-transfer`); today SSH clients use
  `git-lfs-authenticate` and HTTP.

## Not planned

Deliberately out of scope for now, to keep rbit small and maintainable. If one of these
matters to you, open a discussion; the answer may change, but not soon:

- A package registry.
- Code search across repositories.
- Wikis and project boards.
- Reactions on issues and comments.
- `CODEOWNERS`, tag protection and commit signature verification.
- Translations (i18n).
- Databases other than SQLite (such as Postgres) and object storage for repositories or files.
- A built-in Actions runner.
