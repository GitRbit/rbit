//! A table-driven eval of Actions' HTTP surface: the REST API under `/api/v1` and the runner
//! protocol under `/api/actions/runner/v1`, asked by anonymous callers, readers, writers,
//! admins, owners of other repositories, workflow job tokens and runner credentials.
//!
//! Each row is one request against a fresh world (see [`World`]) and states the status it
//! must get, and what must not change when it is refused. Expectations follow GitHub's
//! permission model and `docs/actions.md`. Rows the docs list as gaps are not here: no
//! cache endpoints (`docs/design/actions.md` §12.5.6 defers a server-side cache) and no run
//! deletion (there is no route for it).
//!
//! Grading is programmatic. Every case runs on its own, failures are collected and listed
//! together by id, and each case is either `train` or `test` by a fixed hash of its id, so
//! fixes made against `train` can be checked on `test`. Cases in [`KNOWN_FAILURES`] are
//! reported but do not fail the test; a known failure that passes is reported too.
//!
//! Run with `cargo test -p rbit-web --test actions_eval -- --nocapture` for the report.

use std::{collections::BTreeMap, fmt::Write as _, sync::Arc};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use rbit_core::{
    Config, Timestamp, TokenScope, User, Visibility,
    actions::{BranchPolicy, RunEvent, RunId},
    membership::Permission,
};
use rbit_services::{
    ActionsService, CreateAccessToken, CreateRepository, RepoContext, Services, SignUp,
    actions::{
        environments::EnvironmentInput,
        runners::{ManagedScope, RegistrationOptions},
    },
};
use rbit_storage::{
    Database, KvStore,
    sqlite::actions_runs::{NewJob, NewRun, Rerun, RunStart},
};
use rbit_web::AppState;
use rbit_workflow::spec::{
    Access, DEFAULT_TIMEOUT_MINUTES, Env, JOB_TEMPLATE_VERSION, JobSpec, JobTemplate, RunDefaults, StepAction, StepTemplate, Strategy,
    TokenPermissions,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

/// The split's seed. Changing it reshuffles which cases are `train` and which are `test`.
const EVAL_SEED: u64 = 20_261_027;

/// Cases that fail today and are reported without failing the test. Each entry was checked
/// against the rule in its row before it was added here.
const KNOWN_FAILURES: &[&str] = &[];

// ---------------------------------------------------------------- the table vocabulary

/// Who is calling with a personal access token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Who {
    /// Owns `rocket` (private): the repository every run and runner of the world is in.
    Alice,
    /// Owns `garden` (public) and `vault` (private).
    Bob,
    /// Reads `rocket`.
    Carol,
    /// Writes `rocket`; reviewer of the `production` environment.
    Dave,
    /// Has no access to any of them.
    Eve,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Read,
    Write,
    Admin,
}

impl Scope {
    fn token(self) -> TokenScope {
        match self {
            Self::Read => TokenScope::Read,
            Self::Write => TokenScope::Write,
            Self::Admin => TokenScope::Admin,
        }
    }
}

/// Which registered runner is calling the runner protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Which {
    /// Registered to `alice/rocket`; claims the `build` job.
    A,
    /// Registered to `alice/rocket` too; never claims anything in the world.
    B,
    /// Registered to `bob/garden`.
    Garden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Actor {
    Anonymous,
    Pat(Who, Scope),
    /// The `build` job's token while it runs; declared `permissions: contents: read`.
    JobToken,
    /// The `lint` job's token after `lint` finished.
    FinishedJobToken,
    Runner(Which),
}

/// The state the request is made in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum World {
    /// `build` runs (its log and an artifact written), `lint` finished, `production` and
    /// `staging` runs wait for review, a secret and a variable exist, `garden` is public and
    /// `vault` private, both `bob`'s.
    Base,
    /// [`World::Base`] with run `{run}` cancelled and its `build` task still reporting.
    Cancelled,
    /// [`World::Cancelled`], then re-run for its failed jobs, and the new `build` attempt
    /// claimed by `runner_b`. The old `build` task `{task}` belongs to the attempt before.
    Rerun,
    /// [`World::Base`] with 97 more runs, 101 in all.
    Many,
}

#[derive(Debug, Clone, Copy)]
struct Case {
    id: &'static str,
    tags: &'static [&'static str],
    /// Why the row is hard, for the hard ones.
    why_hard: &'static str,
    /// The rule the row checks, quoted in failure reports.
    rule: &'static str,
    world: World,
    actor: Actor,
    method: &'static str,
    /// Below `/api/v1`, or `runner:` followed by a path below the runner protocol's base.
    /// `{name}` placeholders resolve to the world's ids (see [`Ids::pairs`]).
    path: &'static str,
    body: &'static str,
    status: u16,
    /// The same reads before and after must give the same answers.
    unchanged: bool,
    /// Text that must not appear in the response (a secret's value).
    hidden: &'static str,
    /// A top-level boolean the body must have as `true`.
    true_key: &'static str,
    /// The number of entries in the body's list.
    items: Option<usize>,
    /// `total_count` of the body.
    total: Option<u64>,
}

const fn case(
    id: &'static str,
    world: World,
    actor: Actor,
    method: &'static str,
    path: &'static str,
    status: u16,
    rule: &'static str,
) -> Case {
    Case {
        id,
        tags: &[],
        why_hard: "",
        rule,
        world,
        actor,
        method,
        path,
        body: "",
        status,
        unchanged: false,
        hidden: "",
        true_key: "",
        items: None,
        total: None,
    }
}

impl Case {
    const fn tags(mut self, tags: &'static [&'static str]) -> Self {
        self.tags = tags;
        self
    }

    const fn hard(mut self, why: &'static str) -> Self {
        self.why_hard = why;
        self
    }

    const fn body(mut self, body: &'static str) -> Self {
        self.body = body;
        self
    }

    const fn unchanged(mut self) -> Self {
        self.unchanged = true;
        self
    }

    const fn hidden(mut self, text: &'static str) -> Self {
        self.hidden = text;
        self
    }

    const fn true_key(mut self, key: &'static str) -> Self {
        self.true_key = key;
        self
    }

    const fn items(mut self, count: usize) -> Self {
        self.items = Some(count);
        self
    }

    const fn total(mut self, count: u64) -> Self {
        self.total = Some(count);
        self
    }
}

const ANON: Actor = Actor::Anonymous;
const ALICE_ADMIN: Actor = Actor::Pat(Who::Alice, Scope::Admin);
const ALICE_WRITE: Actor = Actor::Pat(Who::Alice, Scope::Write);
const BOB_ADMIN: Actor = Actor::Pat(Who::Bob, Scope::Admin);
const BOB_WRITE: Actor = Actor::Pat(Who::Bob, Scope::Write);
const CAROL_READ: Actor = Actor::Pat(Who::Carol, Scope::Read);
const CAROL_WRITE: Actor = Actor::Pat(Who::Carol, Scope::Write);
const CAROL_ADMIN: Actor = Actor::Pat(Who::Carol, Scope::Admin);
const DAVE_READ: Actor = Actor::Pat(Who::Dave, Scope::Read);
const DAVE_WRITE: Actor = Actor::Pat(Who::Dave, Scope::Write);
const DAVE_ADMIN: Actor = Actor::Pat(Who::Dave, Scope::Admin);
const EVE_READ: Actor = Actor::Pat(Who::Eve, Scope::Read);

const LONG_BODY_RUNNER_REGISTER: &str = r#"{"token":"{used_token}","name":"build-9","labels":[{"name":"linux","backend":"container"}],"capabilities":{"backends":["container","host"],"os":"linux","arch":"x86_64","capacity":2,"version":"0.2.0","ephemeral":false}}"#;

const CASES: &[Case] = &[
    // ------------------------------------------------------------ reading runs and jobs
    case(
        "read-run-reader",
        World::Base,
        CAROL_READ,
        "GET",
        "/repos/alice/rocket/actions/runs/{run}",
        200,
        "a reader of a private repository reads its runs",
    )
    .tags(&["runs", "reads"]),
    case(
        "read-run-outsider-private",
        World::Base,
        EVE_READ,
        "GET",
        "/repos/alice/rocket/actions/runs/{run}",
        404,
        "a private repository is invisible to users without access",
    )
    .tags(&["runs", "reads", "privacy"]),
    case(
        "read-run-anonymous-private",
        World::Base,
        ANON,
        "GET",
        "/repos/alice/rocket/actions/runs/{run}",
        404,
        "anonymous callers learn nothing of a private repository",
    )
    .tags(&["runs", "reads", "privacy"]),
    case(
        "read-run-anonymous-public",
        World::Base,
        ANON,
        "GET",
        "/repos/bob/garden/actions/runs/{garden_run}",
        200,
        "a public repository's runs are public",
    )
    .tags(&["runs", "reads"]),
    case(
        "read-run-id-of-other-repo",
        World::Base,
        BOB_ADMIN,
        "GET",
        "/repos/bob/garden/actions/runs/{run}",
        404,
        "a run id from one repository is not found under another repository's path",
    )
    .tags(&["runs", "idor"])
    .hard("bob reads garden, so the run exists for him, but the id belongs to alice's rocket"),
    case(
        "read-run-private-other-owner",
        World::Base,
        ALICE_ADMIN,
        "GET",
        "/repos/bob/vault/actions/runs/{vault_run}",
        404,
        "an admin of one repository cannot read a private repository of another owner",
    )
    .tags(&["runs", "idor", "privacy"])
    .hard("alice owns a repository and has admin rights, but nothing in bob's private vault"),
    // ------------------------------------------------------------------ logs and artifacts
    case(
        "logs-reader",
        World::Base,
        CAROL_READ,
        "GET",
        "/repos/alice/rocket/actions/jobs/{job}/logs",
        200,
        "readers of a private repository read its job logs",
    )
    .tags(&["logs", "reads"]),
    case(
        "logs-outsider-private",
        World::Base,
        EVE_READ,
        "GET",
        "/repos/alice/rocket/actions/jobs/{job}/logs",
        404,
        "logs of a private repository are hidden from outsiders",
    )
    .tags(&["logs", "privacy"]),
    case(
        "logs-other-repo-job",
        World::Base,
        BOB_ADMIN,
        "GET",
        "/repos/bob/garden/actions/jobs/{job}/logs",
        404,
        "a job id from alice's repository has no log under bob's path",
    )
    .tags(&["logs", "idor"]),
    case(
        "logs-job-token-declared-read",
        World::Base,
        Actor::JobToken,
        "GET",
        "/repos/alice/rocket/actions/jobs/{job}/logs",
        403,
        "a job token may call only the workflow-task routes, so raw logs are refused",
    )
    .tags(&["logs", "job-token"]),
    case(
        "artifacts-repo-list-name-traversal",
        World::Base,
        CAROL_READ,
        "GET",
        "/repos/alice/rocket/actions/artifacts?name=..%2F..%2Fetc",
        200,
        "the name filter is data, not a path: a traversal-shaped name matches nothing",
    )
    .tags(&["artifacts", "traversal"])
    .hard("the filter goes into a database query, so a path-shaped name must be a plain non-match")
    .items(0)
    .total(0),
    case(
        "artifact-download-reader",
        World::Base,
        CAROL_READ,
        "GET",
        "/repos/alice/rocket/actions/artifacts/{artifact}/tar.gz",
        200,
        "readers download a private repository's artifact",
    )
    .tags(&["artifacts", "reads"]),
    case(
        "artifact-download-zip-reader",
        World::Base,
        CAROL_READ,
        "GET",
        "/repos/alice/rocket/actions/artifacts/{artifact}/zip",
        200,
        "readers download an artifact as GitHub's zip, its archive_download_url",
    )
    .tags(&["artifacts", "reads"]),
    case(
        "artifact-download-zip-outsider",
        World::Base,
        EVE_READ,
        "GET",
        "/repos/alice/rocket/actions/artifacts/{artifact}/zip",
        404,
        "the zip of a private artifact is as hidden as the artifact",
    )
    .tags(&["artifacts", "privacy"]),
    case(
        "artifact-show-outsider",
        World::Base,
        EVE_READ,
        "GET",
        "/repos/alice/rocket/actions/artifacts/{artifact}",
        404,
        "an outsider learns nothing about a private artifact",
    )
    .tags(&["artifacts", "privacy"]),
    case(
        "artifact-delete-reader",
        World::Base,
        CAROL_ADMIN,
        "DELETE",
        "/repos/alice/rocket/actions/artifacts/{artifact}",
        403,
        "deleting needs write access, whatever the token's scope",
    )
    .tags(&["artifacts", "writes"])
    .unchanged(),
    case(
        "artifact-delete-other-repo-id",
        World::Base,
        BOB_WRITE,
        "DELETE",
        "/repos/bob/garden/actions/artifacts/{artifact}",
        404,
        "an artifact id from another repository is not found, even for its owner's writer",
    )
    .tags(&["artifacts", "idor"])
    .unchanged(),
    case(
        "artifact-delete-writer",
        World::Base,
        DAVE_WRITE,
        "DELETE",
        "/repos/alice/rocket/actions/artifacts/{artifact}",
        204,
        "a writer deletes an artifact of their repository",
    )
    .tags(&["artifacts", "writes"]),
    // ------------------------------------------------------------ job tokens
    case(
        "job-token-list-runs-refused",
        World::Base,
        Actor::JobToken,
        "GET",
        "/repos/alice/rocket/actions/runs",
        403,
        "task tokens are refused on endpoints outside the workflow allowlist",
    )
    .tags(&["job-token"]),
    case("job-token-read-own-repo", World::Base, Actor::JobToken, "GET", "/repos/alice/rocket", 200, "a task may read its own repository")
        .tags(&["job-token", "reads"]),
    case(
        "job-token-release-declared-read",
        World::Base,
        Actor::JobToken,
        "POST",
        "/repos/alice/rocket/releases",
        403,
        "a job that declares contents: read cannot create a release",
    )
    .tags(&["job-token", "writes"])
    .hard("the repository's default token permission may be write, so only the job's own declaration lowers it")
    .body(r#"{"tag_name":"v1","name":"v1"}"#)
    .unchanged(),
    case(
        "job-token-release-other-public-repo",
        World::Base,
        Actor::JobToken,
        "POST",
        "/repos/bob/garden/releases",
        403,
        "a task writes only its own repository, even where the other one is public",
    )
    .tags(&["job-token", "idor"])
    .body(r#"{"tag_name":"v1","name":"v1"}"#)
    .unchanged(),
    case(
        "job-token-read-other-private-repo",
        World::Base,
        Actor::JobToken,
        "GET",
        "/repos/bob/vault",
        404,
        "a task cannot see another owner's private repository",
    )
    .tags(&["job-token", "privacy", "idor"]),
    case(
        "job-token-cancel-own-run-refused",
        World::Base,
        Actor::JobToken,
        "POST",
        "/repos/alice/rocket/actions/runs/{run}/cancel",
        403,
        "a task cannot cancel runs, its own included",
    )
    .tags(&["job-token", "runs"])
    .unchanged(),
    case(
        "job-token-secrets-refused",
        World::Base,
        Actor::JobToken,
        "GET",
        "/repos/alice/rocket/actions/secrets",
        403,
        "a task cannot list secrets",
    )
    .tags(&["job-token", "secrets"])
    .hidden("hunter2-deploy-value"),
    case(
        "job-token-variables-refused",
        World::Base,
        Actor::JobToken,
        "GET",
        "/repos/alice/rocket/actions/variables",
        403,
        "a task cannot list variables",
    )
    .tags(&["job-token", "variables"]),
    case(
        "job-token-after-finish-refused",
        World::Base,
        Actor::FinishedJobToken,
        "GET",
        "/repos/alice/rocket",
        401,
        "a finished job's token no longer authenticates",
    )
    .tags(&["job-token"])
    .hard("the token is still a well-formed task token, and the same token authenticated while the job ran"),
    // ------------------------------------------------------------- cancelling and reruns
    case(
        "cancel-reader-with-write-scope",
        World::Base,
        CAROL_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runs/{run}/cancel",
        403,
        "cancelling needs write access, not just a write-scoped token",
    )
    .tags(&["runs", "writes"])
    .unchanged(),
    case(
        "cancel-writer-read-scope",
        World::Base,
        DAVE_READ,
        "POST",
        "/repos/alice/rocket/actions/runs/{run}/cancel",
        403,
        "a read-scoped token cannot cancel, even for a writer",
    )
    .tags(&["runs", "writes", "scopes"])
    .unchanged(),
    case(
        "cancel-writer",
        World::Base,
        DAVE_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runs/{run}/cancel",
        202,
        "a writer cancels a running run",
    )
    .tags(&["runs", "writes"]),
    case(
        "cancel-finished-run-conflicts",
        World::Base,
        DAVE_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runs/{done}/cancel",
        409,
        "a finished run cannot be cancelled",
    )
    .tags(&["runs", "writes"])
    .unchanged(),
    case(
        "cancel-other-repo-run-id",
        World::Base,
        ALICE_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runs/{garden_run}/cancel",
        404,
        "a run id from another repository is not found under this repository",
    )
    .tags(&["runs", "idor"])
    .hard("alice owns rocket and so may cancel its runs; the id belongs to bob's garden")
    .unchanged(),
    case(
        "cancel-anonymous-public-repo",
        World::Base,
        ANON,
        "POST",
        "/repos/bob/garden/actions/runs/{garden_run}/cancel",
        401,
        "anonymous callers are asked to authenticate before a write",
    )
    .tags(&["runs", "writes"])
    .unchanged(),
    case(
        "rerun-finished-writer",
        World::Base,
        DAVE_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runs/{done}/rerun",
        201,
        "a writer re-runs a finished run",
    )
    .tags(&["runs", "writes"]),
    case(
        "rerun-running-run-conflicts",
        World::Base,
        DAVE_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runs/{run}/rerun",
        409,
        "a run that has not finished cannot be re-run",
    )
    .tags(&["runs", "writes"])
    .unchanged(),
    case(
        "rerun-reader",
        World::Base,
        CAROL_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runs/{done}/rerun",
        403,
        "a reader cannot re-run, whatever the token's scope",
    )
    .tags(&["runs", "writes"])
    .unchanged(),
    case(
        "rerun-job-other-repo-id",
        World::Base,
        BOB_WRITE,
        "POST",
        "/repos/bob/garden/actions/jobs/{done_job}/rerun",
        404,
        "a job id from another repository is not found, even for its owner's writer",
    )
    .tags(&["runs", "idor"])
    .unchanged(),
    // -------------------------------------------------------------- secrets and variables
    case(
        "secrets-list-owner-admin",
        World::Base,
        ALICE_ADMIN,
        "GET",
        "/repos/alice/rocket/actions/secrets",
        200,
        "an owner's admin token lists secret names",
    )
    .tags(&["secrets", "reads"])
    .items(1)
    .hidden("hunter2-deploy-value"),
    case(
        "secret-show-owner-admin-no-value",
        World::Base,
        ALICE_ADMIN,
        "GET",
        "/repos/alice/rocket/actions/secrets/DEPLOY_KEY",
        200,
        "a secret is shown without its value",
    )
    .tags(&["secrets", "reads"])
    .hidden("hunter2-deploy-value"),
    case(
        "secret-show-writer-refused",
        World::Base,
        DAVE_ADMIN,
        "GET",
        "/repos/alice/rocket/actions/secrets/DEPLOY_KEY",
        403,
        "a writer who is not an admin cannot read secrets",
    )
    .tags(&["secrets", "reads"])
    .hidden("hunter2-deploy-value")
    .hard("dave's token has admin scope, but dave is only a writer of rocket"),
    case(
        "secret-show-reader-admin-scope",
        World::Base,
        CAROL_ADMIN,
        "GET",
        "/repos/alice/rocket/actions/secrets/DEPLOY_KEY",
        403,
        "a reader's admin-scoped token gives no admin rights",
    )
    .tags(&["secrets", "reads"])
    .hidden("hunter2-deploy-value"),
    case(
        "secrets-list-anonymous-public-repo",
        World::Base,
        ANON,
        "GET",
        "/repos/bob/garden/actions/secrets",
        401,
        "secrets of a public repository still need an admin to read them",
    )
    .tags(&["secrets", "reads"]),
    case(
        "secrets-list-anonymous-private-repo",
        World::Base,
        ANON,
        "GET",
        "/repos/alice/rocket/actions/secrets",
        404,
        "anonymous callers learn nothing of a private repository's secrets",
    )
    .tags(&["secrets", "privacy"]),
    case(
        "secret-rotate-owner",
        World::Base,
        ALICE_ADMIN,
        "PUT",
        "/repos/alice/rocket/actions/secrets/DEPLOY_KEY",
        204,
        "an owner rotates a secret's value, and the response never repeats it",
    )
    .tags(&["secrets", "writes"])
    .body(r#"{"value":"rotated-deploy-value"}"#)
    .hidden("rotated-deploy-value"),
    case(
        "secret-put-writer-refused",
        World::Base,
        DAVE_WRITE,
        "PUT",
        "/repos/alice/rocket/actions/secrets/NEW_SECRET",
        403,
        "a writer cannot create secrets",
    )
    .tags(&["secrets", "writes"])
    .body(r#"{"value":"newval-123"}"#)
    .unchanged(),
    case(
        "secret-put-sealed-box-refused",
        World::Base,
        ALICE_ADMIN,
        "PUT",
        "/repos/alice/rocket/actions/secrets/OTHER_KEY",
        422,
        "sealed boxes are refused with a message, and nothing is stored",
    )
    .tags(&["secrets", "writes"])
    .body(r#"{"encrypted_value":"never-stored-value"}"#)
    .hidden("never-stored-value")
    .unchanged(),
    case(
        "secret-put-lowercase-name",
        World::Base,
        ALICE_ADMIN,
        "PUT",
        "/repos/alice/rocket/actions/secrets/lower-case",
        422,
        "a secret name must be upper case letters, digits and underscores",
    )
    .tags(&["secrets", "writes"])
    .body(r#"{"value":"never-stored-value"}"#)
    .hidden("never-stored-value")
    .unchanged(),
    case(
        "variable-update-reader-refused",
        World::Base,
        CAROL_ADMIN,
        "PATCH",
        "/repos/alice/rocket/actions/variables/REGION",
        403,
        "a reader cannot change a variable",
    )
    .tags(&["variables", "writes"])
    .body(r#"{"value":"us-east-1"}"#)
    .unchanged(),
    // ------------------------------------------------------------ environments and reviews
    case(
        "environment-put-writer-refused",
        World::Base,
        DAVE_WRITE,
        "PUT",
        "/repos/alice/rocket/environments/staging",
        403,
        "a writer cannot change environments",
    )
    .tags(&["environments", "writes"])
    .body(r#"{"prevent_self_review":false}"#)
    .unchanged(),
    case(
        "review-reviewer-approves",
        World::Base,
        DAVE_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runs/{prod}/pending_deployments",
        200,
        "a reviewer of an environment approves a job waiting for it",
    )
    .tags(&["environments", "reviews"])
    .body(r#"{"environment_ids":[{env_prod}],"state":"approved","comment":"ok"}"#),
    case(
        "review-self-review-prevented",
        World::Base,
        ALICE_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runs/{stage}/pending_deployments",
        409,
        "with self-review prevented, whoever started a run cannot approve it",
    )
    .tags(&["environments", "reviews"])
    .hard("alice is a reviewer of staging and started the run, so she may review it only if prevent_self_review is off")
    .body(r#"{"environment_ids":[{env_stage}],"state":"approved","comment":"me"}"#)
    .unchanged(),
    case(
        "review-non-reviewer-owner-refused",
        World::Base,
        ALICE_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runs/{prod}/pending_deployments",
        403,
        "an owner who is not a reviewer cannot approve a deployment",
    )
    .tags(&["environments", "reviews"])
    .body(r#"{"environment_ids":[{env_prod}],"state":"approved","comment":""}"#)
    .unchanged(),
    case(
        "review-job-token-refused",
        World::Base,
        Actor::JobToken,
        "POST",
        "/repos/alice/rocket/actions/runs/{prod}/pending_deployments",
        403,
        "a job cannot approve its own environment's deployment",
    )
    .tags(&["environments", "reviews", "job-token"])
    .body(r#"{"environment_ids":[{env_prod}],"state":"approved","comment":""}"#)
    .unchanged(),
    // ------------------------------------------------------------- runners and tokens
    case(
        "runners-list-writer-refused",
        World::Base,
        DAVE_ADMIN,
        "GET",
        "/repos/alice/rocket/actions/runners",
        403,
        "a writer who is not an admin cannot list runners",
    )
    .tags(&["runners", "reads"]),
    case(
        "runners-list-owner-admin",
        World::Base,
        ALICE_ADMIN,
        "GET",
        "/repos/alice/rocket/actions/runners",
        200,
        "an owner lists the runners of the repository",
    )
    .tags(&["runners", "reads"])
    .items(2)
    .total(2),
    case(
        "runner-show-other-repo-id",
        World::Base,
        ALICE_ADMIN,
        "GET",
        "/repos/alice/rocket/actions/runners/{garden_runner}",
        404,
        "a runner of another repository is not found under this repository",
    )
    .tags(&["runners", "idor"]),
    case(
        "runner-delete-writer-refused",
        World::Base,
        DAVE_ADMIN,
        "DELETE",
        "/repos/alice/rocket/actions/runners/{runner_b}",
        403,
        "a writer cannot remove runners",
    )
    .tags(&["runners", "writes"])
    .unchanged(),
    case(
        "runner-delete-owner",
        World::Base,
        ALICE_ADMIN,
        "DELETE",
        "/repos/alice/rocket/actions/runners/{runner_b}",
        204,
        "an owner removes a runner of their repository",
    )
    .tags(&["runners", "writes"]),
    case(
        "registration-token-writer-refused",
        World::Base,
        DAVE_WRITE,
        "POST",
        "/repos/alice/rocket/actions/runners/registration-token",
        403,
        "a writer cannot create a registration token",
    )
    .tags(&["runners", "tokens"]),
    case(
        "registration-token-owner",
        World::Base,
        ALICE_ADMIN,
        "POST",
        "/repos/alice/rocket/actions/runners/registration-token",
        201,
        "an owner creates a registration token",
    )
    .tags(&["runners", "tokens"]),
    case("register-replayed-token", World::Base, ANON, "POST", "runner:register", 401, "a registration token is single use")
        .tags(&["runners", "registration"])
        .body(LONG_BODY_RUNNER_REGISTER)
        .unchanged(),
    case("runner-poll-no-credential", World::Base, ANON, "POST", "runner:poll", 401, "a poll needs a runner credential")
        .tags(&["runners", "protocol"])
        .body(r#"{"free_slots":1}"#),
    case(
        "runner-poll-other-repo-gets-nothing",
        World::Many,
        Actor::Runner(Which::Garden),
        "POST",
        "runner:poll",
        204,
        "a runner of another repository is never handed this repository's jobs",
    )
    .tags(&["runners", "claims", "idor"])
    .hard("the garden runner is idle, and rocket's queue holds 97 linux jobs it could run")
    .body(r#"{"free_slots":1}"#),
    case(
        "runner-poll-claims-own-repo-job",
        World::Many,
        Actor::Runner(Which::B),
        "POST",
        "runner:poll",
        200,
        "a runner of the repository takes a queued job of it",
    )
    .tags(&["runners", "claims"])
    .body(r#"{"free_slots":1}"#),
    case(
        "runner-poll-with-personal-token",
        World::Base,
        ALICE_ADMIN,
        "POST",
        "runner:poll",
        401,
        "a personal access token is not a runner credential",
    )
    .tags(&["runners", "protocol"])
    .body(r#"{"free_slots":1}"#),
    // -------------------------------------------------------------- runner task calls
    case(
        "task-state-wrong-runner-same-repo",
        World::Base,
        Actor::Runner(Which::B),
        "POST",
        "runner:tasks/{task}/state",
        403,
        "a runner may report only the tasks it claimed",
    )
    .tags(&["protocol", "claims", "idor"])
    .body(r#"{"steps":[],"outputs":{},"result":null}"#)
    .unchanged(),
    case(
        "task-state-runner-of-other-repo",
        World::Base,
        Actor::Runner(Which::Garden),
        "POST",
        "runner:tasks/{task}/state",
        403,
        "a runner of another repository cannot report a task of this one",
    )
    .tags(&["protocol", "claims", "idor"])
    .hard("the runner is authentic and registered; only the task's repository and runner decide")
    .body(r#"{"steps":[],"outputs":{},"result":null}"#)
    .unchanged(),
    case(
        "task-state-unknown-task",
        World::Base,
        Actor::Runner(Which::A),
        "POST",
        "runner:tasks/999999/state",
        404,
        "a task that does not exist is not found",
    )
    .tags(&["protocol", "claims"])
    .body(r#"{"steps":[],"outputs":{},"result":null}"#)
    .unchanged(),
    case("task-state-anonymous", World::Base, ANON, "POST", "runner:tasks/{task}/state", 401, "a task report needs a credential")
        .tags(&["protocol", "claims"])
        .body(r#"{"steps":[],"outputs":{},"result":null}"#)
        .unchanged(),
    case(
        "task-state-after-cancel-answers-cancel",
        World::Cancelled,
        Actor::Runner(Which::A),
        "POST",
        "runner:tasks/{task}/state",
        200,
        "a runner reporting for a cancelled run is told to stop",
    )
    .tags(&["protocol", "cancellation"])
    .hard("the run is cancelled but the task still exists, so the report must succeed and say cancel")
    .body(r#"{"steps":[],"outputs":{},"result":null}"#)
    .true_key("cancel"),
    case(
        "task-state-old-attempt-heartbeat-after-rerun",
        World::Rerun,
        Actor::Runner(Which::A),
        "POST",
        "runner:tasks/{task}/state",
        200,
        "a task of an earlier attempt is told to stop and changes nothing",
    )
    .tags(&["protocol", "cancellation", "rerun"])
    .hard("the same runner that claimed the old attempt reports after the job was re-run and claimed by another runner")
    .body(r#"{"steps":[],"outputs":{},"result":null}"#)
    .true_key("cancel")
    .unchanged(),
    case(
        "task-state-old-attempt-result-after-rerun",
        World::Rerun,
        Actor::Runner(Which::A),
        "POST",
        "runner:tasks/{task}/state",
        200,
        "a result reported for an earlier attempt does not settle the new attempt",
    )
    .tags(&["protocol", "cancellation", "rerun"])
    .hard("a finished result from the old task must not mark the job the new attempt is running as finished")
    .body(r#"{"steps":[],"outputs":{"version":"old"},"result":"success"}"#)
    .true_key("cancel")
    .unchanged(),
    case(
        "task-logs-wrong-runner",
        World::Base,
        Actor::Runner(Which::B),
        "POST",
        "runner:tasks/{task}/logs?from=1",
        403,
        "a runner may append logs only to its own task",
    )
    .tags(&["protocol", "logs", "claims"])
    .body("evil\n")
    .unchanged(),
    case(
        "artifact-upload-wrong-runner",
        World::Base,
        Actor::Runner(Which::B),
        "PUT",
        "runner:tasks/{task}/artifacts/planted",
        403,
        "a runner may upload artifacts only through its own task",
    )
    .tags(&["protocol", "artifacts", "claims"])
    .body("planted bytes")
    .unchanged(),
    // ---------------------------------------------------------- artifact names
    case(
        "artifact-upload-dotdot",
        World::Base,
        Actor::Runner(Which::A),
        "PUT",
        "runner:tasks/{task}/artifacts/..",
        400,
        "an artifact name may not be `..`",
    )
    .tags(&["protocol", "artifacts", "traversal"])
    .body("bytes")
    .unchanged(),
    case(
        "artifact-upload-percent-encoded-dotdot",
        World::Base,
        Actor::Runner(Which::A),
        "PUT",
        "runner:tasks/{task}/artifacts/%2e%2e",
        400,
        "a percent-encoded `..` is still `..`",
    )
    .tags(&["protocol", "artifacts", "traversal"])
    .hard("the path is decoded before the name is checked, so an encoded dot-dot must be refused too")
    .body("bytes")
    .unchanged(),
    case(
        "artifact-upload-encoded-slash",
        World::Base,
        Actor::Runner(Which::A),
        "PUT",
        "runner:tasks/{task}/artifacts/a%2Fb",
        400,
        "an artifact name may not contain a slash, encoded or not",
    )
    .tags(&["protocol", "artifacts", "traversal"])
    .hard("%2F decodes to a slash inside one path segment, which would name a path below the artifact directory")
    .body("bytes")
    .unchanged(),
    case(
        "artifact-upload-too-long",
        World::Base,
        Actor::Runner(Which::A),
        "PUT",
        "runner:tasks/{task}/artifacts/{long_name}",
        400,
        "an artifact name is at most 128 characters",
    )
    .tags(&["protocol", "artifacts"])
    .body("bytes")
    .unchanged(),
    // ------------------------------------------------------------------- pagination
    case(
        "runs-default-page-size",
        World::Many,
        CAROL_READ,
        "GET",
        "/repos/alice/rocket/actions/runs",
        200,
        "without per_page a page holds 30 runs",
    )
    .tags(&["pagination"])
    .items(30)
    .total(101),
    case(
        "runs-per-page-clamped-to-max",
        World::Many,
        CAROL_READ,
        "GET",
        "/repos/alice/rocket/actions/runs?per_page=500",
        200,
        "per_page is clamped to the maximum of 100",
    )
    .tags(&["pagination"])
    .hard("the request asks for 500; the answer must hold 100, not 500 and not an error")
    .items(100)
    .total(101),
    case(
        "runs-per-page-not-a-number",
        World::Many,
        CAROL_READ,
        "GET",
        "/repos/alice/rocket/actions/runs?per_page=abc",
        422,
        "a non-numeric per_page is refused",
    )
    .tags(&["pagination"]),
    case(
        "runs-page-past-end-is-empty",
        World::Many,
        CAROL_READ,
        "GET",
        "/repos/alice/rocket/actions/runs?page=99999",
        200,
        "a page past the end is empty, not an error",
    )
    .tags(&["pagination"])
    .items(0)
    .total(101),
];

// ---------------------------------------------------------------- the world

/// Ids the table's `{name}` placeholders resolve to, and the credentials of the world.
#[derive(Default)]
struct Ids {
    run: i64,
    done: i64,
    prod: i64,
    stage: i64,
    job: i64,
    done_job: i64,
    garden_run: i64,
    garden_job: i64,
    vault_run: i64,
    task: i64,
    done_task: i64,
    artifact: i64,
    runner_b: i64,
    garden_runner: i64,
    env_prod: i64,
    env_stage: i64,
    used_token: String,
    /// A personal access token per user and scope.
    pats: Vec<(Who, Scope, String)>,
    job_token: String,
    finished_token: String,
    runner_a: String,
    runner_b_credential: String,
    garden_credential: String,
}

impl Ids {
    fn pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("run", self.run.to_string()),
            ("done", self.done.to_string()),
            ("prod", self.prod.to_string()),
            ("stage", self.stage.to_string()),
            ("job", self.job.to_string()),
            ("done_job", self.done_job.to_string()),
            ("garden_run", self.garden_run.to_string()),
            ("garden_job", self.garden_job.to_string()),
            ("vault_run", self.vault_run.to_string()),
            ("task", self.task.to_string()),
            ("done_task", self.done_task.to_string()),
            ("artifact", self.artifact.to_string()),
            ("runner_b", self.runner_b.to_string()),
            ("garden_runner", self.garden_runner.to_string()),
            ("env_prod", self.env_prod.to_string()),
            ("env_stage", self.env_stage.to_string()),
            ("used_token", self.used_token.clone()),
            ("long_name", "a".repeat(129)),
        ]
    }

    fn fill(&self, template: &str) -> String {
        self.pairs().into_iter().fold(template.to_owned(), |text, (name, value)| text.replace(&format!("{{{name}}}"), &value))
    }

    fn pat(&self, who: Who, scope: Scope) -> &str {
        self.pats.iter().find(|(user, granted, _)| *user == who && *granted == scope).map_or("", |(_, _, token)| token.as_str())
    }
}

struct Fixture {
    _directory: tempfile::TempDir,
    services: Services,
    database: Database,
    router: Router,
    alice: User,
    bob: User,
    carol: User,
    dave: User,
    eve: User,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut config = Config::default();
        config.storage.data_dir = directory.path().join("data");
        config.security.rate_limit = false;
        config.actions.enabled = true;
        config.actions.long_poll_seconds = 1;
        "https://git.example.com".clone_into(&mut config.server.public_url);
        std::fs::create_dir_all(config.storage.repositories_dir()).unwrap();
        let database = Database::open(&config.storage.sqlite_path()).await.unwrap();
        let kv = KvStore::open(&config.storage.kv_path()).unwrap();
        let config = Arc::new(config);
        let services = Services::new(config.clone(), &database, &kv).await.unwrap();
        let mut users = Vec::new();
        for name in ["alice", "bob", "carol", "dave", "eve"] {
            let user = SignUp { username: name.into(), email: format!("{name}@example.com"), password: "correct horse battery".into() };
            // Not site administrators: every right below comes from the repository.
            users.push(services.accounts.create_user(user, Some(false)).await.unwrap());
        }
        let [alice, bob, carol, dave, eve]: [User; 5] = <[User; 5]>::try_from(users).ok().unwrap();
        let router = rbit_web::router(AppState::new(config, services.clone()));
        Self { _directory: directory, services, database, router, alice, bob, carol, dave, eve }
    }

    fn actions(&self) -> &ActionsService {
        self.services.actions.as_ref().unwrap()
    }

    async fn repo(&self, owner: &User, name: &str) -> RepoContext {
        let login = owner.name().to_string();
        self.services.repositories.open(Some(owner), &login, name).await.unwrap()
    }

    async fn create_repo(&self, owner: &User, name: &str, visibility: Visibility) -> RepoContext {
        let input = CreateRepository { name: name.into(), description: String::new(), visibility };
        self.services.repositories.create(owner, input).await.unwrap();
        let repo = self.repo(owner, name).await;
        self.database.actions_settings().set_enabled(repo.repo.repo.id, true, Timestamp::now()).await.unwrap();
        repo
    }

    async fn pat(&self, user: &User, scope: Scope) -> String {
        let input = CreateAccessToken { name: format!("{scope:?}"), scope: scope.token(), expires_in_days: None };
        self.services.tokens.create(user, input).await.unwrap().1
    }

    async fn runner_credential(&self, actor: &User, repo: &RepoContext, name: &str) -> (i64, String, String) {
        let issued =
            self.actions().create_registration_token(actor, ManagedScope::Repository(repo), RegistrationOptions::default()).await.unwrap();
        let token = issued.secret.expose().to_owned();
        let answer = send(self, "POST", "runner:register", None, register_body(&token, name).into_bytes(), None).await;
        assert_eq!(answer.status, StatusCode::OK, "{}", answer.json());
        let id = answer.json()["runner_id"].as_i64().unwrap();
        (id, answer.json()["credential"].as_str().unwrap().to_owned(), token)
    }
}

fn register_body(token: &str, name: &str) -> String {
    json!({
        "token": token,
        "name": name,
        "labels": [{"name": "linux", "backend": "container"}],
        "capabilities": {"backends": ["container", "host"], "os": "linux", "arch": "x86_64", "capacity": 2, "version": "0.2.0", "ephemeral": false},
    })
    .to_string()
}

/// A job below `runs-on: linux` with two steps; it needs `needs` and waits for
/// `environment`, when given. It declares `contents: read`.
fn job(key: &str, needs: &[&str], environment: Option<&str>) -> JobTemplate {
    let step = |name: Option<&str>, script: &str| StepTemplate {
        id: None,
        name: name.map(str::to_owned),
        condition: None,
        action: StepAction::Run(script.to_owned()),
        shell: None,
        working_directory: None,
        env: Env::new(),
        continue_on_error: None,
        timeout_minutes: None,
    };
    JobTemplate {
        version: JOB_TEMPLATE_VERSION,
        job_id: key.to_owned(),
        name: key.to_owned(),
        needs: needs.iter().map(|need| (*need).to_owned()).collect(),
        condition: None,
        runs_on: vec!["linux".to_owned()],
        matrix: serde_json::Map::new(),
        strategy: Strategy::SINGLE,
        permissions: Some(TokenPermissions { contents: Access::Read, issues: Access::None, pull_requests: Access::None }),
        concurrency: None,
        timeout_minutes: DEFAULT_TIMEOUT_MINUTES,
        continue_on_error: false,
        workflow_env: Env::new(),
        env: Env::new(),
        defaults: RunDefaults::default(),
        outputs: BTreeMap::new(),
        container: None,
        services: Vec::new(),
        steps: vec![step(Some("Build"), "make"), step(None, "make test")],
        environment: environment.map(str::to_owned),
        environment_url: None,
        calls: Vec::new(),
        need_names: std::collections::BTreeMap::new(),
        deferred: false,
        default_ceiling: false,
    }
}

/// A run of `repo` started by `actor` with these jobs; wakes the polls.
async fn seed(fixture: &Fixture, repo: &RepoContext, actor: &User, jobs: &[JobTemplate]) -> rbit_core::actions::Run {
    let payloads: Vec<Vec<u8>> = jobs.iter().map(|template| serde_json::to_vec(template).unwrap()).collect();
    let rows: Vec<(Vec<String>, Vec<String>)> = jobs.iter().map(|template| (template.needs.clone(), template.runs_on.clone())).collect();
    let new_jobs: Vec<NewJob<'_>> = jobs
        .iter()
        .zip(&rows)
        .zip(&payloads)
        .map(|((template, (needs, runs_on)), payload)| NewJob {
            key: &template.job_id,
            name: &template.name,
            matrix: None,
            needs,
            runs_on,
            permissions: None,
            payload,
            condition: false,
            max_parallel: None,
            group: None,
            environment: template.environment.as_deref(),
        })
        .collect();
    let run = fixture
        .database
        .actions_runs()
        .create_run(&NewRun {
            repo: repo.repo.repo.id,
            workflow_path: ".rbit/workflows/ci.yml",
            workflow_name: "CI",
            title: "Build it",
            event: RunEvent::Push,
            reference: "refs/heads/main",
            sha: SHA,
            actor: Some(actor.id()),
            trigger_actor: Some(actor.id()),
            pull_number: None,
            is_fork: false,
            trusted: true,
            concurrency_group: None,
            cancel_in_progress: false,
            event_payload: r#"{"ref":"refs/heads/main"}"#,
            dispatch_inputs: None,
            start: RunStart::Runnable,
            jobs: &new_jobs,
            error: None,
            now: Timestamp::now(),
        })
        .await
        .unwrap();
    fixture.actions().bump_tasks_version();
    run
}

/// A reply: status and body.
struct Reply {
    status: StatusCode,
    body: Vec<u8>,
}

impl Reply {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
}

async fn send(fixture: &Fixture, method: &str, path: &str, credential: Option<&str>, body: Vec<u8>, digest: Option<String>) -> Reply {
    let (uri, runner) = match path.strip_prefix("runner:") {
        Some(rest) => (format!("/api/actions/runner/v1/{rest}"), true),
        None => (format!("/api/v1{path}"), false),
    };
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "git.example.com")
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::USER_AGENT, "rbit-runner/0.2.0");
    if runner {
        request = request.header("rbit-runner-protocol", "1");
    }
    if let Some(digest) = digest {
        request = request.header("rbit-content-sha256", digest);
    }
    if let Some(credential) = credential {
        request = request.header(header::AUTHORIZATION, format!("Bearer {credential}"));
    }
    let response = fixture.router.clone().oneshot(request.body(Body::from(body)).unwrap()).await.unwrap();
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec();
    Reply { status, body }
}

/// The first 200 characters of a line, for a failure report.
fn clip(line: &str) -> String {
    line.chars().take(200).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().fold(String::new(), |mut hex, byte| {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
        hex
    })
}

/// The job specification of a claimed task, as the runner receives it.
async fn poll(fixture: &Fixture, credential: &str) -> JobSpec {
    let answer = send(fixture, "POST", "runner:poll", Some(credential), br#"{"free_slots":1}"#.to_vec(), None).await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.json());
    serde_json::from_value(answer.json()["task"].clone()).unwrap()
}

/// Builds the world the cases name. Panics when the setup itself is wrong.
#[allow(clippy::too_many_lines)]
async fn build(world: World) -> (Fixture, Ids) {
    let fx = Fixture::new().await;
    let rocket = fx.create_repo(&fx.alice, "rocket", Visibility::Private).await;
    let garden = fx.create_repo(&fx.bob, "garden", Visibility::Public).await;
    let vault = fx.create_repo(&fx.bob, "vault", Visibility::Private).await;
    fx.services.collaborators.add(&fx.alice, &rocket, "carol", Permission::Read).await.unwrap();
    fx.services.collaborators.add(&fx.alice, &rocket, "dave", Permission::Write).await.unwrap();

    let mut ids = Ids::default();
    let (_, credential_a, used) = fx.runner_credential(&fx.alice, &rocket, "build-a").await;
    ids.runner_a = credential_a;
    ids.used_token = used;
    let (runner_b, credential_b, _) = fx.runner_credential(&fx.alice, &rocket, "build-b").await;
    ids.runner_b = runner_b;
    ids.runner_b_credential = credential_b;
    let (garden_runner, garden_credential, _) = fx.runner_credential(&fx.bob, &garden, "site-1").await;
    ids.garden_runner = garden_runner;
    ids.garden_credential = garden_credential;

    let production =
        EnvironmentInput { reviewers: vec!["dave".into()], prevent_self_review: false, branch_policy: BranchPolicy::All, wait_timer: 0 };
    let staging =
        EnvironmentInput { reviewers: vec!["alice".into()], prevent_self_review: true, branch_policy: BranchPolicy::All, wait_timer: 0 };
    let production = fx.actions().put_environment(&fx.alice, &rocket, "production", &production).await.unwrap();
    let staging = fx.actions().put_environment(&fx.alice, &rocket, "staging", &staging).await.unwrap();
    ids.env_prod = production.id.0;
    ids.env_stage = staging.id.0;

    let run = seed(&fx, &rocket, &fx.alice, &[job("build", &[], None), job("test", &["build"], None)]).await;
    let done = seed(&fx, &rocket, &fx.alice, &[job("lint", &[], None)]).await;
    ids.run = run.id.0;
    ids.done = done.id.0;
    // Two claims: which job each poll gets is the queue's choice, so match on the key.
    for _ in 0..2 {
        let spec = poll(&fx, &ids.runner_a).await;
        match spec.template.job_id.as_str() {
            "build" => {
                ids.task = spec.task_id.0;
                spec.token.expose().clone_into(&mut ids.job_token);
            }
            "lint" => {
                ids.done_task = spec.task_id.0;
                spec.token.expose().clone_into(&mut ids.finished_token);
            }
            other => panic!("unexpected job {other}"),
        }
    }
    let log =
        send(&fx, "POST", &format!("runner:tasks/{}/logs?from=0", ids.task), Some(&ids.runner_a), b"compiling\n".to_vec(), None).await;
    assert_eq!(log.status, StatusCode::OK, "{}", log.json());
    // An artifact as rbit-runner stores it: a tar.gz with the files under `w/`.
    let artifact = {
        let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        let mut header = tar::Header::new_gnu();
        header.set_size(5);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, "w/app.txt", &b"built"[..]).unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    };
    let upload = send(
        &fx,
        "PUT",
        &format!("runner:tasks/{}/artifacts/dist", ids.task),
        Some(&ids.runner_a),
        artifact.clone(),
        Some(sha256_hex(&artifact)),
    )
    .await;
    assert_eq!(upload.status, StatusCode::CREATED, "{}", upload.json());
    let finished = json!({"steps": [], "outputs": {}, "result": "success"}).to_string();
    let state = send(&fx, "POST", &format!("runner:tasks/{}/state", ids.done_task), Some(&ids.runner_a), finished.into_bytes(), None).await;
    assert_eq!(state.status, StatusCode::OK, "{}", state.json());

    let prod_run = seed(&fx, &rocket, &fx.alice, &[job("deploy", &[], Some("production"))]).await;
    let stage_run = seed(&fx, &rocket, &fx.alice, &[job("stage", &[], Some("staging"))]).await;
    ids.prod = prod_run.id.0;
    ids.stage = stage_run.id.0;
    let garden_run = seed(&fx, &garden, &fx.bob, &[job("site", &[], None)]).await;
    let vault_run = seed(&fx, &vault, &fx.bob, &[job("secret", &[], None)]).await;
    ids.garden_run = garden_run.id.0;
    ids.vault_run = vault_run.id.0;
    // Bob's runner takes bob's job, so the garden queue is empty from here on.
    let claimed = poll(&fx, &ids.garden_credential).await;
    assert_eq!(claimed.template.job_id, "site", "the garden runner takes its own job");
    let garden_jobs = fx.database.actions_runs().jobs(garden_run.id).await.unwrap();
    ids.garden_job = garden_jobs[0].id.0;
    let rocket_jobs = fx.database.actions_runs().jobs(run.id).await.unwrap();
    ids.job = rocket_jobs.iter().find(|job| job.key == "build").unwrap().id.0;
    let done_jobs = fx.database.actions_runs().jobs(done.id).await.unwrap();
    ids.done_job = done_jobs[0].id.0;

    for who in [Who::Alice, Who::Bob, Who::Carol, Who::Dave, Who::Eve] {
        let owner = match who {
            Who::Alice => &fx.alice,
            Who::Bob => &fx.bob,
            Who::Carol => &fx.carol,
            Who::Dave => &fx.dave,
            Who::Eve => &fx.eve,
        };
        for scope in [Scope::Read, Scope::Write, Scope::Admin] {
            let token = fx.pat(owner, scope).await;
            ids.pats.push((who, scope, token));
        }
    }
    let admin = ids.pat(Who::Alice, Scope::Admin).to_owned();
    let listed = send(&fx, "GET", &format!("/repos/alice/rocket/actions/runs/{}/artifacts", ids.run), Some(&admin), Vec::new(), None).await;
    ids.artifact = listed.json()["artifacts"][0]["id"].as_i64().unwrap();
    let secret = send(
        &fx,
        "PUT",
        "/repos/alice/rocket/actions/secrets/DEPLOY_KEY",
        Some(&admin),
        br#"{"value":"hunter2-deploy-value"}"#.to_vec(),
        None,
    )
    .await;
    assert_eq!(secret.status, StatusCode::CREATED, "{}", secret.json());
    let variable = json!({"name": "REGION", "value": "eu-west-1"}).to_string();
    let variable = send(&fx, "POST", "/repos/alice/rocket/actions/variables", Some(&admin), variable.into_bytes(), None).await;
    assert_eq!(variable.status, StatusCode::CREATED, "{}", variable.json());

    match world {
        World::Base => {}
        World::Cancelled => {
            let repo = fx.repo(&fx.alice, "rocket").await;
            fx.actions().cancel(&fx.alice, &repo, RunId(ids.run)).await.unwrap();
        }
        World::Rerun => {
            let repo = fx.repo(&fx.alice, "rocket").await;
            fx.actions().cancel(&fx.alice, &repo, RunId(ids.run)).await.unwrap();
            fx.actions().rerun(&fx.alice, &repo, RunId(ids.run), Rerun::Failed).await.unwrap();
            let spec = poll(&fx, &ids.runner_b_credential).await;
            assert_eq!(spec.template.job_id, "build", "the new attempt of build is claimed");
        }
        World::Many => {
            for _ in 0..97 {
                seed(&fx, &rocket, &fx.alice, &[job("build", &[], None)]).await;
            }
        }
    }
    (fx, ids)
}

/// Every answer that must not change between a refused request and its retry: the reads a
/// reviewer would look at, as the owner and as bob.
async fn snapshot(fx: &Fixture, ids: &Ids) -> String {
    let admin = ids.pat(Who::Alice, Scope::Admin);
    let bob = ids.pat(Who::Bob, Scope::Admin);
    let paths: Vec<(&str, String)> = vec![
        (admin, format!("/repos/alice/rocket/actions/runs/{}", ids.run)),
        (admin, format!("/repos/alice/rocket/actions/runs/{}", ids.done)),
        (admin, format!("/repos/alice/rocket/actions/runs/{}/jobs", ids.run)),
        (admin, format!("/repos/alice/rocket/actions/runs/{}/jobs", ids.done)),
        (admin, format!("/repos/alice/rocket/actions/runs/{}/artifacts", ids.run)),
        (admin, format!("/repos/alice/rocket/actions/runs/{}/pending_deployments", ids.prod)),
        (admin, format!("/repos/alice/rocket/actions/runs/{}/pending_deployments", ids.stage)),
        (admin, format!("/repos/alice/rocket/actions/jobs/{}/logs", ids.job)),
        (admin, "/repos/alice/rocket/actions/runners".to_owned()),
        (admin, "/repos/alice/rocket/actions/secrets".to_owned()),
        (admin, "/repos/alice/rocket/actions/variables".to_owned()),
        (admin, "/repos/alice/rocket/actions/artifacts".to_owned()),
        (bob, format!("/repos/bob/garden/actions/runs/{}", ids.garden_run)),
        (bob, "/repos/bob/garden/actions/runners".to_owned()),
        (admin, "/repos/alice/rocket/releases".to_owned()),
        (admin, "/repos/alice/rocket/environments".to_owned()),
        (bob, "/repos/bob/garden/releases".to_owned()),
    ];
    let mut out = String::new();
    for (token, path) in paths {
        let reply = send(fx, "GET", &path, Some(token), Vec::new(), None).await;
        let mut body = String::from_utf8_lossy(&reply.body).into_owned();
        if path.ends_with("/runners") {
            // A runner's busy flag and last-seen time follow its own polls and reports; a
            // refused request does not change them, but a finished task frees its runner.
            body = strip_runner_liveness(&body);
        }
        let _ = writeln!(out, "{path} {} {body}", reply.status);
    }
    out
}

/// `text` without the runners' `busy`, `status` and `last_seen_at`, which change with
/// their tasks rather than with what a request did to the attempt.
fn strip_runner_liveness(text: &str) -> String {
    fn strip(value: &mut Value) {
        match value {
            Value::Object(map) => {
                for key in ["busy", "status", "last_seen_at"] {
                    map.remove(key);
                }
                map.values_mut().for_each(strip);
            }
            Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    match serde_json::from_str::<Value>(text) {
        Ok(mut value) => {
            strip(&mut value);
            value.to_string()
        }
        Err(_) => text.to_owned(),
    }
}

// ---------------------------------------------------------------- running the table

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Split {
    Train,
    Test,
}

/// FNV-1a over the seed and the id: fixed, so the split is the same on every run.
fn split(id: &str) -> Split {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in EVAL_SEED.to_le_bytes().iter().chain(id.as_bytes()) {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    if hash % 100 < 70 { Split::Train } else { Split::Test }
}

#[derive(Debug)]
struct Outcome {
    id: &'static str,
    split: Split,
    request: String,
    expected: String,
    actual: String,
    rule: &'static str,
    failure: Option<String>,
}

async fn run_case(case: Case) -> Outcome {
    let (fx, ids) = Box::pin(build(case.world)).await;
    let credential = match case.actor {
        Actor::Anonymous => None,
        Actor::Pat(who, scope) => Some(ids.pat(who, scope).to_owned()),
        Actor::JobToken => Some(ids.job_token.clone()),
        Actor::FinishedJobToken => Some(ids.finished_token.clone()),
        Actor::Runner(Which::A) => Some(ids.runner_a.clone()),
        Actor::Runner(Which::B) => Some(ids.runner_b_credential.clone()),
        Actor::Runner(Which::Garden) => Some(ids.garden_credential.clone()),
    };
    let path = ids.fill(case.path);
    let body = ids.fill(case.body).into_bytes();
    let digest = (case.method == "PUT" && path.contains("/artifacts/")).then(|| sha256_hex(&body));
    let request = format!("{} {}{}", case.method, path, if case.body.is_empty() { "" } else { " (body)" });
    let before = if case.unchanged { snapshot(&fx, &ids).await } else { String::new() };
    let reply = send(&fx, case.method, &path, credential.as_deref(), body, digest).await;
    let text = String::from_utf8_lossy(&reply.body).into_owned();
    let shown: String = text.chars().take(240).collect();

    let mut problems = Vec::new();
    if reply.status.as_u16() != case.status {
        problems.push(format!("status {} (expected {})", reply.status.as_u16(), case.status));
    }
    if !case.hidden.is_empty() && text.contains(case.hidden) {
        problems.push(format!("the response shows `{}`", case.hidden));
    }
    if !case.true_key.is_empty() && reply.json()[case.true_key] != json!(true) {
        problems.push(format!("`{}` is not true", case.true_key));
    }
    if let Some(count) = case.items {
        let json = reply.json();
        let list = match &json {
            Value::Array(items) => Some(items.len()),
            Value::Object(map) => map.values().find_map(|value| value.as_array().map(Vec::len)),
            _ => None,
        };
        if list != Some(count) {
            problems.push(format!("{list:?} list entries (expected {count})"));
        }
    }
    if let Some(total) = case.total {
        let actual = reply.json()["total_count"].as_u64();
        if actual != Some(total) {
            problems.push(format!("total_count {actual:?} (expected {total})"));
        }
    }
    if case.unchanged {
        let after = snapshot(&fx, &ids).await;
        if after != before {
            let (was, now) = before.lines().zip(after.lines()).find(|(was, now)| was != now).unwrap_or(("", ""));
            problems.push(format!("state changed although the request was refused: before `{}` after `{}`", clip(was), clip(now)));
        }
    }
    let expected = {
        let mut parts = vec![format!("status {}", case.status)];
        if case.unchanged {
            parts.push("nothing changes".into());
        }
        if let Some(count) = case.items {
            parts.push(format!("{count} entries"));
        }
        parts.join(", ")
    };
    let actual = if problems.is_empty() { expected.clone() } else { format!("status {} · {}", reply.status.as_u16(), shown) };
    Outcome {
        id: case.id,
        split: split(case.id),
        request,
        expected,
        actual,
        rule: case.rule,
        failure: (!problems.is_empty()).then(|| problems.join("; ")),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn actions_http_eval() {
    // The table is well formed: unique ids, a size in the designed range, known failures real.
    let mut ids: Vec<&str> = CASES.iter().map(|case| case.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), CASES.len(), "case ids are unique");
    assert!((40..=120).contains(&CASES.len()), "{} cases", CASES.len());
    for known in KNOWN_FAILURES {
        assert!(CASES.iter().any(|case| case.id == *known), "known failure `{known}` is a case");
    }

    let mut tasks = tokio::task::JoinSet::new();
    for case in CASES {
        let case = *case;
        tasks.spawn(async move { (case.id, Box::pin(run_case(case)).await) });
    }
    let mut outcomes = Vec::new();
    let mut crashed = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        match joined {
            Ok((_, outcome)) => outcomes.push(outcome),
            Err(error) => crashed.push(error.to_string()),
        }
    }
    outcomes.sort_by_key(|outcome| outcome.id);

    let mut report = String::new();
    for (name, wanted) in [("train", Split::Train), ("test", Split::Test)] {
        let rows: Vec<&Outcome> = outcomes.iter().filter(|outcome| outcome.split == wanted).collect();
        let passed = rows.iter().filter(|outcome| outcome.failure.is_none()).count();
        let _ = writeln!(report, "{name}: {passed}/{} passed", rows.len());
    }
    let failing: Vec<&Outcome> = outcomes.iter().filter(|outcome| outcome.failure.is_some()).collect();
    let _ = writeln!(report, "cases: {}, failing: {}, crashed setups: {}", outcomes.len(), failing.len(), crashed.len());
    for outcome in &failing {
        let known = if KNOWN_FAILURES.contains(&outcome.id) { "known" } else { "UNEXPECTED" };
        let _ = write!(
            report,
            "\n[{}] {known} ({:?})\n  request:  {}\n  expected: {}\n  actual:   {}\n  failure:  {}\n  rule:     {}\n",
            outcome.id,
            outcome.split,
            outcome.request,
            outcome.expected,
            outcome.actual,
            outcome.failure.as_deref().unwrap_or(""),
            outcome.rule,
        );
    }
    for outcome in outcomes.iter().filter(|outcome| outcome.failure.is_none() && KNOWN_FAILURES.contains(&outcome.id)) {
        let _ = writeln!(report, "\n[{}] known failure now passes; remove it from KNOWN_FAILURES", outcome.id);
    }
    for error in &crashed {
        let _ = writeln!(report, "\ncase setup crashed: {error}");
    }
    println!("{report}");

    let unexpected: Vec<&str> = failing.iter().filter(|outcome| !KNOWN_FAILURES.contains(&outcome.id)).map(|outcome| outcome.id).collect();
    assert!(crashed.is_empty(), "{} case(s) panicked: {crashed:?}", crashed.len());
    assert!(unexpected.is_empty(), "unexpected failures: {unexpected:?}");
}
