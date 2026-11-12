# Agent loop: landing safely, review and tasks

Status: built (first part). Work packages 4.1 to 4.9 and 5.1 to 5.5 of the agent loop
plan. Each endpoint answers one compact object with a `schema`; the OpenAPI document
describes every one.

## 4.1 Merge queue

- `POST …/pulls/{n}/queue {head, merge_method}` puts a pull request in line for its base
  branch when every requirement but its checks is met now (`405` naming what is not),
  for the head the caller saw (`409 head_changed`); a conflict check still running does not
  hold it, since building the candidate finds a conflict. `GET` gives the entry and its place,
  `DELETE` takes it out. Migration `0081_merge_queue.sql`; one live entry per pull request.
- One entry per line is tested at a time. Its merge on the current base tip (by the chosen
  method, with the merge box's default message) is built into
  `refs/merge-queue/<base>/pr-<n>` (`rbit_git::merge::candidate`) and reported as a push;
  workflows see a push to the branch `merge-queue/<base>/pr-<n>`, as GitHub's queue
  branches, so a workflow without a branch filter tests it and one limited to `main` does
  not run. Clients may not push to `refs/merge-queue/`.
- Every context the base's protection requires passing on that commit lands it: the base
  moves from the tip it was built on to exactly that commit by compare-and-swap, and the
  pull request is recorded as merged by it. A base that moved meanwhile means a new
  candidate and a new test: nothing lands untested. A failed required check, a conflict, a
  head moved after queuing, or a closed pull request takes the entry out with a `reason`.
  With no required context, the candidate lands once built.
- The queue advances on commit statuses and pushes of its repository, and a sweep every 30
  seconds. `rbit pr queue <n> [--watch]` and `rbit pr dequeue <n>`.

## 4.2 Readiness

`GET …/pulls/{n}/readiness[?head=]` answers the evaluation the merge itself runs (the merge
box shows the same one): `ready`, `blocked`, `may_bypass`, each requirement, and each
status context as `expected`, `running`, `failed` or `passed`, required or not. The
required list is the protection's minus contexts no workflow will report for the head
(2.2). `POST …/pulls/{n}/ready {head}` marks a draft ready for that exact head.
`rbit pr ready <n> [--head] [--mark] [--watch]`: exit 0 ready, 1 a required check failed, 8
still expected or running.

## 4.3 `pr ensure`

`POST …/pulls/ensure {head, base, title, body, draft, request_id}` returns the open pull
request for the pair of branches (`existing`) or creates one (`created`, a draft unless
`draft: false`). A request id is remembered per account for a week (migration
`0080_agent_requests.sql`): replayed with the same inputs it answers the same pull request
(`replayed`), even when the first response was lost; with other inputs it is `409`.

## 4.4 Server-side writes

Each takes the commit the caller saw (`409 head_changed` otherwise), goes through write
access, the archive guard and branch protection as a push would (`405` with the reason),
is made with the merge machinery and handed over by compare-and-swap, and is reported as a
push: `POST …/pulls/{n}/update-branch {expected_head}` (merge the base into a same-repository
head branch), `POST …/agent/revert {commit, base}` (branch `revert-<commit>` with one commit
undoing it, and a pull request; never merged on its own), `POST …/agent/branch-update
{branch, sha, expected}` (a move that drops commits is a force push: refused where denied). `rbit pr update-branch`, `rbit pr revert`. Left out: a conditional
change of one file over the API (the web editor's path).

## 4.5 Queries

`GET …/agent/compare?base=&head=` (merge base, ahead, behind), `…/agent/contains?commit=&ref=`
(is the commit in the default branch or `ref`), `…/agent/commits/{sha}/pulls` (pull
requests the commit is the head, the merge commit or the merged head of, any state).

## 4.6 Artifacts by commit; releases promote them

`GET …/agent/artifacts?sha=&name=` finds the newest artifact of that name a run of the
commit kept. `POST …/releases/{id}/assets/from-artifact {artifact_id}` attaches its files (at
most 20) to the release as built, so what ships is what CI tested.

## 4.7 Red `main`

A failed push run of a workflow on the default branch opens one issue "CI is failing on
`<branch>`: `<workflow file>`" (by whoever triggered the run; the file, since a workflow that
cannot be read has no name) with the run, its commit, the
failed jobs and the commands that find the culprit and propose a revert; while it keeps
failing, each failure is a comment on that one issue; the next success closes it as
completed. Nothing is reverted on its own.

## 4.8 Scoped tokens

A personal access token may be limited to one repository and to rights within its scope:
`push` (also releases, which make tags), `statuses`, `merge` (merge, queue, ready),
`dispatch` (also repository dispatch), `approve` (approve, rerun, cancel, review verdicts and
dismissals, deployment approvals), `read_logs` (logs, failure digests, artifacts, the log lines
in status and watch). Such a token reaches only its repository's endpoints (Git and LFS included) and
`GET /user`, never with the `admin` scope: nothing account-wide, so no repository list naming the others and no new
repository. Writes no right names (switching workflows, deleting artifacts and caches) need
an unlimited token. The limit lives in its own table beside the
token (migration `0082_token_limits.sql`), so the credential table and its writer protocol
are unchanged. Every API repository lookup refuses other repositories with 404; Git over
HTTP refuses them, and pushes without `push`. `GET`, `POST`, `DELETE /user/tokens` list,
create (with `repository` and `rights`) and revoke tokens, for an unlimited `admin` token
only, so a limited token makes no broader one.

## 4.9 Bisect from recorded results

`GET …/agent/bisect?good=&bad=[&workflow=]` and `rbit bisect GOOD BAD` read the results CI
recorded for each commit of `bad`'s first-parent line down to `good` (at most 1,000) and
answer the last that passed, the first that failed, the untested commits between them and
the middle one to test next, or the culprit when none is left.

## 5.1 to 5.3 Review

`GET …/pulls/{n}/manifest` lists changed files with old and new blob ids and line counts,
no patch text, with the change `classes` (`docs_only`, `tests_only`, `workflows`,
`dependencies`); `?path=` gives that file's hunks, `&hunk=` one. `GET …/pulls/{n}/inbox`
lists unresolved threads of submitted reviews with excerpts; an anchor is `current`, `moved`
to the one head line that still matches, or `ambiguous`. `PUT …/comments/{id}/resolution`
resolves a thread; replies use `POST …/comments` with `in_reply_to`. Left out: review rules
that act on the change classes.

## 5.4 Issues as the task queue

`POST …/issues/{n}/take` assigns the caller with one conditional write, only while nobody
holds the issue; otherwise `409 conflict` naming the holder. `GET …/agent/issues?q=` finds
issues and pull requests by their words, at most 20 short rows. `rbit issue take`, `rbit
issue search`.

## 5.5 Tasks

`rbit task start <issue|branch>` takes the issue, fetches the base and adds a worktree on
`feature/<n>-<title>` from that exact commit; `resume` reports the worktree's state and its
CI; `finish` removes a clean worktree and refuses a dirty one.

## Tests

End to end: `agent_land.rs` (an issue taken once, a pull request ensured twice, readiness,
the queue rebuilding when `main` moves and landing exactly the tested commit, the commit
queries, a refused push to `refs/merge-queue/`) and `agent_tokens.rs` (a limited token over
the API and Git, token management, revocation). Unit: bisection narrowing, change classes,
anchor mapping, rights.
