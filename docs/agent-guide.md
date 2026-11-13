# Reading CI results from a coding agent

How a coding agent should find out what CI made of its push, using rbit's REST API. Every
path below is under `/api/v1` and takes `Authorization: token <token>`. The goal is the
right answer for the exact commit you pushed, read with as few calls and bytes as
possible.

## The question

You pushed commit `<sha>` to branch `<branch>` of `<owner>/<repo>`. Answer:

- `success`: a run for that commit finished and passed;
- `failure`, with the file and line of the first failure;
- `pending`: a run for that commit exists and has not finished;
- `none_expected`: no workflow applies to that commit, so no run will come.

## Steps

1. **Find the run for your commit.**
   `GET /repos/<owner>/<repo>/actions/runs?branch=<branch>&per_page=5` and take the run
   whose `head_sha` equals your commit. Never use the repository's newest run: it may be
   another branch's.
2. **If no run has that commit,** look at what changed. A push that only touches paths
   the workflow ignores (`paths-ignore`, often `docs/**` and `*.md`) starts no run.
3. **Read the run's state.** `status` is `queued`, `in_progress` or `completed`;
   `conclusion` is set once it completes.
4. **If it failed, list its jobs.** `GET /repos/<owner>/<repo>/actions/runs/<id>/jobs`
   and pick the jobs whose `conclusion` is not `success` or `skipped`.
5. **Read the failed job's log.** `GET /repos/<owner>/<repo>/actions/jobs/<job id>/logs`
   returns the whole log as text. Look for, in this order: an annotation
   `::error file=<path>,line=<n>`, a compiler line `<path>:<line>:<col>: error:`, a
   failing test line naming a file and line, then `##[error]`.

## Traps

- An empty list of runs or of commit statuses is not a pass.
- A commit status of `pending` with no statuses means nothing has reported yet.
