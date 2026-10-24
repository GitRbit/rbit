# Roadmap

rbit is an open-source, self-hosted Git platform that one person can run on a small server:
one binary, no external services, GitHub-compatible where that helps (API shapes, webhooks,
`fixes #12`), and secure by default. It is licensed under MIT OR Apache-2.0.
This page separates available features, near-term priorities, longer-term ideas and work
outside the current scope. Priorities are not release commitments and may change with feedback —
open a [feature request](https://github.com/GitRbit/rbit/issues/new?template=feature_request.yml)
to argue for something.

## Available today

See [README.md](README.md#features) for feature details and [CHANGELOG.md](CHANGELOG.md)
for release history.

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
  repository transfer and account username changes with redirects from old names.
- **Repository essentials:** releases with files, archives, blame, compare, forks, pull
  mirrors, README links and images.
- **Issues and pull requests:** one shared timeline; labels, milestones, assignees,
  references and closing keywords, search, in-app notifications; pull requests within a
  repository or across forks, line comments with resolvable conversations, reviews, mergeability
  checks, merge / squash / rebase; a base-branch picker for existing pull requests.
- **Branch protection:** no force pushes or deletion, required pull requests and approvals,
  restricted pushers — enforced over HTTP, SSH and for merges.
- **Automation:** a versioned REST API (`/api/v1`, OpenAPI 3.1), including collaborators,
  teams, releases with assets and forks; signed, GitHub-shaped webhooks with a retrying
  delivery queue; commit statuses and required checks for external CI.
- **Themes:** light, dark and about 500 tinted-theming schemes.
- **Distribution:** community contribution files, release binaries for
  Linux, macOS and Windows, and a multi-architecture image at `ghcr.io/gitrbit/rbit`.
- **Email notifications** on top of the in-app inbox, with per-user preferences, daily or
  weekly digests and one-click unsubscribe.
- **Editing files in the browser:** create, edit, rename, delete and upload files, committing
  to the branch or to a new branch with a pull request.
- **Git LFS:** the batch API, uploads and downloads, and file locking.
- **External sign-in:** OpenID Connect providers and GitHub OAuth.
- **Migration:** importing repositories with their issues, pull requests and releases from
  GitHub, GitHub Enterprise and Gitea/Forgejo, and push mirrors.
- **Repository documents:** document tabs, a searchable heading outline and Mermaid diagrams.
- **Activity:** contribution heatmaps on user and organisation profiles and repository commits.
- **Administration:** instance branding and deployment configuration forms.
- **Personal dashboards:** cross-repository `/issues` and `/pulls` with created, assigned,
  mentioned and review-requested views.

Markdown editors offer Write/Preview tabs for issue and pull request bodies, comments,
reviews and repository Markdown files, using the same sanitized rendering as saved text.

## Near-term priorities

The current near-term priorities are implemented in the features above.

## Longer-term ideas

Ideas with no schedule yet:

- A runner protocol on top of the status API, so external CI runners can pick up jobs from
  rbit (rbit itself still would not run builds).
- A native gitoxide transport for `receive-pack`, behind a configuration switch, so pushes
  no longer need the `git` binary.
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
