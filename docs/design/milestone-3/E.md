# E — Editing files in the browser (notes)

Workstream E of [milestone 3](../milestone-3.md). No migration (`0035` stays unused). The
text below is for the files the integrator owns (`CHANGELOG.md`, `README.md`,
`docs/ARCHITECTURE.md`, `docs/HANDOFF.md`, `deploy/rbit.example.toml`).

## As built

| Item | Where |
|---|---|
| Validated paths (`FilePath`: no `..`/`.`/empty parts, no `.git` incl. `.git.`, `.git `, `git~1`, not absolute, no NUL/control/backslash, ≤ 255 bytes per part, ≤ 1024 in all), line-ending rule, default commit titles, `{user}-patch-{n}` | `crates/rbit-core/src/file_edit.rs` |
| `[editor]` configuration section | `crates/rbit-core/src/config/editor.rs` |
| Scratch-repository write path: `EditWorkspace` (streamed blobs, tree checks, scratch index, `commit-tree`, lease) | `crates/rbit-git/src/edit.rs` |
| The hand-over (fetch + `update-ref` compare-and-swap) factored out of the merge so both share it; `Git`/`Signature` helpers made `pub(crate)` | `crates/rbit-git/src/merge.rs` |
| `EditService`: authorisation, mirrors read-only, push gate, identity, line endings, uploads, `PushService` wrapping | `crates/rbit-services-base/src/edits.rs` |
| `ProtectionService::branch_update_refusal` (the push decision without auditing, for the form) | `crates/rbit-services-base/src/protection.rs` |
| Routes `/{owner}/{repo}/new|edit|delete/{branch}/{path…}` (page router, own body limit) and `/{owner}/{repo}/upload/{branch}/{dir…}` (outside it, streaming) | `crates/rbit-web/src/routes/edits.rs` |
| Views and templates: editor, delete confirmation, upload form, shared commit box | `crates/rbit-web/src/views/edits.rs`, `templates/repo/{edit_file,delete_file,upload_files,commit_box}.html` |
| Buttons: pencil/trash on the blob page (editor only for displayable text), "Add file" + upload on tree pages; writers, branches, non-mirrors only | `routes/repo.rs` (`RepoPage::can_edit_on`), `views/repo.rs`, `templates/repo/{blob,tree}.html` |
| `RepoUrls::edit(action, branch, path)` | `crates/rbit-web/src/urls.rs` |
| e2e harness: `Client::post_multipart` (fields + several files) | `crates/rbit-e2e/src/lib.rs` |

### How a web edit is committed

1. **Checks before any object is written** (`EditService::check`): write access
   (`Forbidden`, pages answer 404), not a pull mirror, the branch exists, for a direct commit
   the form's commit is still the tip (early `BranchMoved`), for a new branch a valid unused
   name, then the push gate — `ProtectionService::check_push` with a `PushActor` for the
   user and one `RefUpdate` (update of the branch, or creation of the new one), exactly as a
   push would be decided, bypasses audited. A refusal of the branch itself is
   `EditFailure::Protected`: the form disables "commit directly" and preselects the new
   branch. The form already shows that up front via `branch_update_refusal` (no audit).
2. **Workspace**: a scratch bare repository under `<data_dir>/tmp/merges/<random>` whose
   only object source is the repository (the merge's `Overlay`), holding a repository lease
   so a transfer cannot move the repository meanwhile. Contents become blobs with
   `git hash-object -w --no-filters --stdin`, streamed and bounded (uploads never sit in
   memory; no attributes or line-ending filters apply).
3. **Tree checks** against the parent commit's tree (gitoxide lookups of every path and
   its ancestors): new files and move targets must be free (a file the same edit removes
   frees its name), edited/moved/deleted paths must be files (submodules and directories
   are refused), no path may run through an existing file or through another written path,
   no path written twice. Edits and moves keep the mode (executable, symlink); an upload
   replacing a file keeps its mode, but turns a symlink into a regular file.
4. **Tree building** on a scratch index (`GIT_INDEX_FILE` inside the workspace):
   `read-tree <parent>`, one `update-index -z --add --remove --index-info` (removals first,
   then writes, NUL-terminated on stdin — paths are never arguments), `write-tree`. A
   directory whose last file is deleted disappears. A tree equal to the parent's is refused
   ("there are no changes to commit").
5. **Commit**: `commit-tree` with the parent as the only parent, author = committer = the
   user, date = now.
6. **Hand-over** (shared with merges): the repository fetches the commit from the
   workspace, then `update-ref refs/heads/<branch> <new> <parent>` — the lease. A branch
   that moved is `BranchMoved` (409 page, nothing committed, the user's text kept, the form
   now based on the new tip and linking to the branch history). A new branch uses the zero
   id as the expected value, so an existing branch is never overwritten.
7. **Downstream**: the git work is wrapped in `PushService::begin`/`complete`, exactly like
   a merge, so `RepositoryPushed` carries the ref update and webhooks, commit references
   and `fixes #n`, cache warm-up and pull request mergeability react as to a push.

### Rules worth knowing

- **Line endings**: browsers submit textareas with CRLF line breaks whatever the file used.
  Submitted text has every CRLF (and lone CR) turned into LF, then is written in the file's
  existing style — CRLF when at least as many of its lines end in CRLF as in a bare LF —
  and LF for new files. Nothing else changes: no trailing newline is added or removed.
  The template emits a newline right after `<textarea>` so a file starting with a blank
  line keeps it. Uploads are stored byte for byte.
- **Author**: the signed-in user's display name (or username) with their address when it
  is verified, otherwise `<username>@<noreply domain>` (`[pulls] noreply_domain`, default
  `noreply.<host of public_url>`). Committer = author.
- **Limits**: text edits and new files are limited to `[repository] max_render_bytes`
  (the editor is offered only for text the browser would display: binaries and larger
  files show a message and can still be deleted or replaced by an upload); the editor
  routes accept form bodies of `6 × max_render_bytes + 256 KiB` (percent-encoding and
  CRLF). Uploads: `[editor] max_upload_bytes` per file, `max_upload_files` per commit,
  `upload_timeout_seconds` per request; the upload body limit is their product plus
  256 KiB. Committing is bounded by `[pulls] merge_timeout_seconds`.
- **Commit message**: the title (defaults: `Create <path>`, `Update <path>`,
  `Rename <a> to <b>`, `Delete <path>`, `Add files via upload`; single line, ≤ 255
  characters) and an optional description (≤ 64 Ki characters).

## Deviations

- **Archived repositories**: rbit has no archived-repository concept yet (no flag, no
  migration, nothing in the push gate). Mirrors are refused; `EditService::read_only_reason`
  is the single place to add archived repositories when that feature exists (and the
  pages and buttons follow it through `EditService::can_edit` / `RepoPage::can_edit_on`,
  which would need the same flag).
- **Author address**: `rbit_core::pull::noreply_email` documents "rbit never writes a real
  email address into a commit"; the milestone contract asks for the primary verified
  address. The integrator chose privacy: web edits are authored with the noreply
  address, exactly like merges.
- **Renaming binaries** is not offered (the rename field lives in the text editor). They
  can be deleted and uploaded under the new name.
- **Empty repositories**: no "create the first file" from the browser (the empty-repository
  page keeps its push instructions); `EditWorkspace` would need a parent-less commit.
- **Mirrors in e2e**: covered by the service tests only (creating a mirror end to end needs
  a reachable upstream).

## Integration requests

1. **Markdown preview (W)** is integrated. `markdown_preview_url(target)` provides the
   repository and branch context for the shared Write/Preview enhancement. File previews
   submit only `content` and the current `path`; relative links use its directory. The
   endpoint uses the same renderer and configured text limit as browser file editing.
2. **Archived repositories** (whoever adds them): extend `EditService::read_only_reason`
   and `RepoPage::can_edit_on`.

## CHANGELOG entry

```markdown
### Added
- Edit files in the browser: create (with new directories), edit, rename or move, delete
  and upload files from the repository pages, one commit each, on the current branch or on
  a new branch followed by a pull request. Branch protection applies exactly as for
  pushes; a branch that moved meanwhile is reported, never silently rebased. Commits made
  this way trigger webhooks, `fixes #n` and everything else a push does. New `[editor]`
  configuration section for upload limits.
```

## README

Feature line (under the code-browsing / collaboration features):

```markdown
- **Edit in the browser**: create, edit, rename, delete and upload files as one commit,
  directly or on a new branch with a pull request; branch protection applies.
```

Configuration table row:

| Section | Key | Default | Meaning |
|---|---|---|---|
| `[editor]` | `max_upload_bytes` | `52428800` (50 MiB) | Largest file uploaded through the web interface |
| `[editor]` | `max_upload_files` | `100` | Files one upload (one commit) may add |
| `[editor]` | `upload_timeout_seconds` | `600` | Upper bound for one upload request |

## `deploy/rbit.example.toml`

```toml
# Editing and uploading files in the browser. Text edits are limited by
# [repository] max_render_bytes instead (the editor is only offered for files the
# browser displays).
[editor]
# Largest file that may be uploaded through the web interface, in bytes.
max_upload_bytes = 52428800
# Files one upload (one commit) may add.
max_upload_files = 100
# Upper bound for one upload request; uploads bypass the 30-second page timeout.
upload_timeout_seconds = 600
```

## ARCHITECTURE section

```markdown
### Editing files in the browser

Writers edit, create, rename, delete and upload files at
`/{owner}/{repo}/{new,edit,delete,upload}/{branch}/{path…}` (pages answer 404 to
everyone else, to mirrors and for anything that is not a branch). Like merges, a web
edit never checks anything out: `rbit_git::edit::EditWorkspace` is a scratch bare
repository under `<data_dir>/tmp/merges` borrowing the repository's objects; contents are
streamed into it with `hash-object --no-filters`, the changes are checked against the
parent's tree (no clobbering, no paths through files, modes kept), the tree is built
with `read-tree` / `update-index --index-info` / `write-tree` on a scratch index, and one
`commit-tree` commit reaches the branch through the merge's hand-over: a fetch and one
`update-ref <branch> <new> <commit the form was opened at>` compare-and-swap. A moved
branch is a 409 page (nothing committed, the text kept), never a rebase.

`EditService` decides everything first: write access, mirrors read-only, the push gate
(`check_push`, as for a push updating or creating the branch; a refusal of the branch
offers a new branch, then redirects to the new pull request page). The author is the
user with their noreply address, as for merges. The commit is wrapped in
`PushService::begin`/`complete`, so `RepositoryPushed` drives webhooks, commit references
(`fixes #n`), caches and mergeability as for any push. Text keeps the file's line-ending
style (browsers send CRLF); uploads are stored byte for byte. Limits: `[repository]
max_render_bytes` for text, `[editor]` for uploads (which stream outside the page router,
like release files).
```

HANDOFF §6 decision line:

```markdown
- Web edits never check files out (scratch repository + scratch index + `commit-tree`)
  and reach the branch through the merge's `update-ref` compare-and-swap; protection is
  the push gate (`check_push`), and they are reported through `PushService` like pushes.
```
