//! Actions: environments. Deployment environments end to end through the router: their
//! settings pages, a job that waits for a reviewer until one approves it on the run page,
//! its environment's secrets and variables in the job a runner receives, the URL it
//! reports, branch policies and self-review, and the REST API.

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_core::{
    TokenScope,
    actions::{BranchPolicy, SecretVisibility},
};
use rbit_services::{
    CreateAccessToken,
    actions::{environments::EnvironmentInput, secrets::ActionsTarget},
};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    actions_runner::Fixture,
    actions_runner_tasks::{payload, poll, seed, spec, state, template},
};

struct Web<'a> {
    fixture: &'a Fixture,
    cookie: String,
}

impl Web<'_> {
    async fn send(&self, method: Method, path: &str, body: &str) -> (StatusCode, String, Option<String>) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::COOKIE, &self.cookie)
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "http://localhost:3000")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(Body::from(body.to_owned()))
            .unwrap();
        let response = self.fixture.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let location = response.headers().get(header::LOCATION).map(|value| value.to_str().unwrap().to_owned());
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&body).into_owned(), location)
    }

    async fn page(&self, path: &str) -> String {
        let (status, body, _) = self.send(Method::GET, path, "").await;
        assert_eq!(status, StatusCode::OK, "{path}: {body}");
        body
    }
}

async fn api(fixture: &Fixture, method: Method, path: &str, token: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(format!("/api/v1{path}"))
        .header(header::HOST, "git.example.com")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(body.map_or_else(Body::empty, |body| Body::from(body.to_string())))
        .unwrap();
    let response = fixture.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn deploy(environment: &str) -> rbit_workflow::spec::JobTemplate {
    let mut job = template("deploy", &["linux"], &[]);
    job.environment = Some(environment.to_owned());
    job.environment_url = Some("https://example.com/${{ github.run_id }}".to_owned());
    job
}

#[tokio::test]
async fn a_protected_environment_holds_its_jobs_until_a_reviewer_approves_them() {
    let fixture = Fixture::with(|config| config.actions.long_poll_seconds = 1).await;
    let actions = fixture.actions();
    let repo = fixture.repo().await;
    let web = Web {
        fixture: &fixture,
        cookie: format!("rbit_session={}", fixture.services.accounts.start_session(&fixture.alice).await.unwrap().encode()),
    };
    let credential = fixture.runner(&["linux"]).await;

    // The settings pages: a new environment, its rules, a secret and a variable.
    let (status, _, location) = web.send(Method::POST, "/alice/rocket/settings/actions/environments", "name=production").await;
    assert!(status.is_redirection(), "{status}");
    assert_eq!(location.as_deref(), Some("/alice/rocket/settings/actions/environments/production"));
    let (status, _, _) = web
        .send(
            Method::POST,
            "/alice/rocket/settings/actions/environments/production",
            "reviewers=alice&policy=selected&patterns=main%0Arelease%2F*",
        )
        .await;
    assert!(status.is_redirection(), "{status}");
    let (status, page, _) =
        web.send(Method::POST, "/alice/rocket/settings/actions/environments/production", "reviewers=nobody&policy=all").await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(page.contains("There is no user called nobody"), "{page}");
    let environment = actions.environment(&repo, "production").await.unwrap();
    let target = ActionsTarget::Environment(&repo, &environment);
    actions.put_secret(&fixture.alice, target, "DEPLOY_KEY", "s3cret-deploy-key", SecretVisibility::All).await.unwrap();
    actions.put_variable(&fixture.alice, target, "REGION", "eu-west").await.unwrap();
    let list = web.page("/alice/rocket/settings/actions/environments").await;
    assert!(list.contains("production") && list.contains("1 reviewer · branches main, release/*"), "{list}");
    let settings = web.page("/alice/rocket/settings/actions/environments/production").await;
    assert!(settings.contains("DEPLOY_KEY") && settings.contains("eu-west") && !settings.contains("s3cret-deploy-key"), "{settings}");

    // The job waits: no runner may take it, and the run page asks the reviewer.
    let run = seed(&fixture, &[payload(&deploy("production"))]).await;
    let (status, _) = poll(&fixture, &credential, Some("a")).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "a job waiting for review is not handed out");
    let run_page = web.page(&format!("/alice/rocket/actions/runs/{}", run.number)).await;
    assert!(run_page.contains("Waiting for review") && run_page.contains("Approve and deploy"), "{run_page}");

    let token = fixture
        .services
        .tokens
        .create(&fixture.alice, CreateAccessToken { name: "t".into(), scope: TokenScope::Write, expires_in_days: None })
        .await
        .unwrap()
        .1;
    let (_, shown) = api(&fixture, Method::GET, &format!("/repos/alice/rocket/actions/runs/{}", run.id.0), &token, None).await;
    assert_eq!(shown["status"], "waiting", "as GitHub shows a run whose deployments wait for review");
    let (status, pending) =
        api(&fixture, Method::GET, &format!("/repos/alice/rocket/actions/runs/{}/pending_deployments", run.id.0), &token, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        (pending[0]["environment"]["name"].as_str(), pending[0]["current_user_can_approve"].as_bool()),
        (Some("production"), Some(true))
    );

    // Approved on the run page: a runner gets it, with the environment's values.
    let (status, _, _) = web
        .send(
            Method::POST,
            &format!("/alice/rocket/actions/runs/{}/review", run.number),
            "environment=production&decision=approve&comment=go",
        )
        .await;
    assert!(status.is_redirection(), "{status}");
    let (status, body) = poll(&fixture, &credential, Some("b")).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let job = spec(&body);
    assert_eq!(job.secrets.get("DEPLOY_KEY").map(rbit_workflow::secret::Secret::expose), Some("s3cret-deploy-key"));
    assert_eq!(job.contexts.vars.get("REGION").map(String::as_str), Some("eu-west"));
    let done = json!({"steps": [], "outputs": {}, "result": "success", "environment_url": "https://example.com/42"});
    assert_eq!(state(&fixture, &credential, job.task_id.0, &done).await.0, StatusCode::OK);
    let run_page = web.page(&format!("/alice/rocket/actions/runs/{}", run.number)).await;
    assert!(run_page.contains("https://example.com/42") && !run_page.contains("Waiting for review"), "{run_page}");

    // A branch the policy does not allow fails when a runner takes the job.
    let changes = EnvironmentInput {
        reviewers: Vec::new(),
        prevent_self_review: false,
        branch_policy: BranchPolicy::Selected(vec!["release/*".into()]),
    };
    actions.put_environment(&fixture.alice, &repo, "production", &changes).await.unwrap();
    let refused = seed(&fixture, &[payload(&deploy("production"))]).await;
    let (status, _) = poll(&fixture, &credential, Some("c")).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "the claimed job failed at once");
    let refused = fixture.database.actions_runs().run(refused.id).await.unwrap().unwrap();
    assert_eq!(refused.status, rbit_core::actions::Status::Failure);

    let only_excluding = EnvironmentInput { branch_policy: BranchPolicy::Selected(vec!["!main".into()]), ..EnvironmentInput::default() };
    assert!(actions.put_environment(&fixture.alice, &repo, "production", &only_excluding).await.is_err(), "a pattern must admit something");

    // With self-review prevented, whoever started the run cannot approve it.
    let changes = EnvironmentInput { reviewers: vec!["alice".into()], prevent_self_review: true, branch_policy: BranchPolicy::All };
    actions.put_environment(&fixture.alice, &repo, "production", &changes).await.unwrap();
    let held = seed(&fixture, &[payload(&deploy("production"))]).await;
    let held_run = fixture.database.actions_runs().run(held.id).await.unwrap().unwrap();
    let (status, body) = api(
        &fixture,
        Method::POST,
        &format!("/repos/alice/rocket/actions/runs/{}/pending_deployments", held.id.0),
        &token,
        Some(json!({"environment_ids": [environment.id.0], "state": "approved", "comment": ""})),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    // Rejected instead (self-review applies to approving and rejecting alike on GitHub, so
    // drop the rule first): the job fails and the run with it.
    let changes = EnvironmentInput { prevent_self_review: false, ..changes };
    actions.put_environment(&fixture.alice, &repo, "production", &changes).await.unwrap();
    let (status, left) = api(
        &fixture,
        Method::POST,
        &format!("/repos/alice/rocket/actions/runs/{}/pending_deployments", held.id.0),
        &token,
        Some(json!({"environment_ids": [environment.id.0], "state": "rejected", "comment": "not now"})),
    )
    .await;
    assert_eq!((status, left), (StatusCode::OK, json!([])));
    assert_eq!(fixture.database.actions_runs().run(held_run.id).await.unwrap().unwrap().status, rbit_core::actions::Status::Failure);

    // The REST API shows and changes environments; deleting one lets nothing wait for it.
    let (status, listed) = api(&fixture, Method::GET, "/repos/alice/rocket/environments", &token, None).await;
    assert_eq!((status, listed["total_count"].as_u64()), (StatusCode::OK, Some(1)));
    assert_eq!(listed["environments"][0]["protection_rules"][0]["reviewers"][0]["reviewer"]["login"], "alice");
    let protected = json!({"deployment_branch_policy": {"protected_branches": true, "custom_branch_policies": false}});
    let (status, _) = api(&fixture, Method::PUT, "/repos/alice/rocket/environments/staging", &token, Some(protected.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "changing environments takes an admin token");
    let admin = fixture
        .services
        .tokens
        .create(&fixture.alice, CreateAccessToken { name: "a".into(), scope: TokenScope::Admin, expires_in_days: None })
        .await
        .unwrap()
        .1;
    let (status, staging) = api(&fixture, Method::PUT, "/repos/alice/rocket/environments/staging", &admin, Some(protected)).await;
    assert_eq!((status, staging["deployment_branch_policy"]["protected_branches"].as_bool()), (StatusCode::OK, Some(true)), "{staging}");
    // `null` clears a rule, as on GitHub; an absent field keeps it.
    let (status, staging) = api(&fixture, Method::PUT, "/repos/alice/rocket/environments/staging", &admin, Some(json!({}))).await;
    assert_eq!((status, staging["deployment_branch_policy"]["protected_branches"].as_bool()), (StatusCode::OK, Some(true)), "{staging}");
    let cleared = json!({"deployment_branch_policy": null});
    let (status, staging) = api(&fixture, Method::PUT, "/repos/alice/rocket/environments/staging", &admin, Some(cleared)).await;
    assert_eq!((status, staging["deployment_branch_policy"].is_null()), (StatusCode::OK, true), "{staging}");
    assert_eq!(api(&fixture, Method::DELETE, "/repos/alice/rocket/environments/staging", &admin, None).await.0, StatusCode::NO_CONTENT);
    let (status, _, location) = web.send(Method::POST, "/alice/rocket/settings/actions/environments/production/delete", "").await;
    assert_eq!((status.is_redirection(), location.as_deref()), (true, Some("/alice/rocket/settings/actions/environments")));
    assert_eq!(api(&fixture, Method::GET, "/repos/alice/rocket/environments/production", &token, None).await.0, StatusCode::NOT_FOUND);
}
