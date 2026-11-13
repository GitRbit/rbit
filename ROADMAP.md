# Roadmap

rbit is an open-source, self-hosted Git platform. The server runs as one process without
Postgres or Redis; optional Actions jobs run on a separate `rbit-runner`. This page records
what is implemented on `main` and distinguishes possible future work from commitments. Some
features may precede the [latest published release](https://github.com/GitRbit/rbit/releases).
For an up-to-date
work queue, see [issues](https://github.com/GitRbit/rbit/issues) and
[feature requests](https://github.com/GitRbit/rbit/issues/new?template=feature_request.yml).

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
  delivery queue; commit statuses, required checks and a merge queue.
- **Actions:** GitHub-style workflows on rbit's own runner, `rbit-runner` — container and host
  backends, built-in checkout, cache and artifacts, secrets and variables, fork-safe pull
  request runs, job logs, commit statuses, `workflow_run`/`workflow_job` webhooks and a REST
  API. Off unless an administrator turns it on.
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
- **Wikis:** repository pages with search, history, comparison, restore, deletion and recovery;
  repository administrators can enable or disable the wiki.
- **Activity:** contribution heatmaps on user and organisation profiles and repository commits.
- **Administration:** instance branding and deployment configuration forms.
- **Personal dashboards:** cross-repository `/issues` and `/pulls` with created, assigned,
  mentioned and review-requested views.
- **Archived repositories:** read-only repositories that can still be cloned, forked and
  starred, with a banner, list badges and an Explore filter.
- **Languages:** a language picker for English, German, Spanish, French, Brazilian Portuguese,
  Russian, Japanese, Simplified Chinese and Korean, with English fallback where a catalog is
  incomplete.
- **Agent workflows:** a CLI and API for commit-specific CI status, failure digests, test
  reports, scratch CI runs, pull request readiness, safe retries and merge queue operations.
  See [the CLI guide](docs/cli.md) and [the Actions guide](docs/actions.md).

Markdown editors offer Write/Preview tabs for issue and pull request bodies, comments,
reviews and repository Markdown files, using the same sanitized rendering as saved text.

## Longer-term ideas

Ideas with no schedule yet:

- A native gitoxide transport for `receive-pack`, behind a configuration switch, so pushes
  no longer need the `git` binary.
- Git LFS over pure SSH (`git-lfs-transfer`); today SSH clients use
  `git-lfs-authenticate` and HTTP.

## Not planned

Deliberately out of scope for now, to keep rbit small and maintainable. If one of these
matters to you, open a discussion; the answer may change, but not soon:

- A package registry.
- Code search across repositories.
- Project boards.
- Reactions on issues and comments.
- `CODEOWNERS`, tag protection and commit signature verification.
- Databases other than SQLite (such as Postgres) and object storage for repositories or files.
- Running builds inside the rbit server process: builds run on `rbit-runner`, a separate
  program, never in the forge.
- A marketplace of downloadable actions: workflows use rbit's built-in actions, containers and
  `run:` steps.
