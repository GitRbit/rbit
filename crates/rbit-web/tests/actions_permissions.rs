//! Actions: settings API. GitHub's `actions/permissions` endpoints for a repository, an
//! organisation and the caller: what they read and write, the owner's ceilings, and that
//! the policies reach runs (the switch, the token and the job's actions policy).

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use rbit_core::{TokenScope, User, actions::DefaultTokenPermission};
use rbit_services::{CreateAccessToken, CreateOrganization, CreateRepository, SignUp};
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{
    actions_runner::Fixture,
    actions_runner_tasks::{payload, poll, seed, spec, template},
};

async fn token(fixture: &Fixture, user: &User) -> String {
    let input = CreateAccessToken { name: "admin".into(), scope: TokenScope::Admin, expires_in_days: None };
    fixture.services.tokens.create(user, input).await.unwrap().1
}

async fn api(fixture: &Fixture, method: Method, path: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(format!("/api/v1{path}")).header(header::HOST, "git.example.com");
    if let Some(token) = token {
        request = request.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let body = match body {
        Some(body) => {
            request = request.header(header::CONTENT_TYPE, "application/json");
            Body::from(body.to_string())
        }
        None => Body::empty(),
    };
    let response = fixture.router.clone().oneshot(request.body(body).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

#[tokio::test]
async fn a_repositorys_permissions_read_back_and_reach_its_runs() {
    let fixture = Fixture::new().await;
    let admin = token(&fixture, &fixture.alice).await;
    let base = "/repos/alice/rocket/actions/permissions";
    let get = |path: &'static str| api(&fixture, Method::GET, path, Some(&admin), None);

    assert_eq!(
        get("/repos/alice/rocket/actions/permissions").await,
        (StatusCode::OK, json!({"enabled": true, "allowed_actions": "all", "sha_pinning_required": false}))
    );
    assert_eq!(
        get("/repos/alice/rocket/actions/permissions/workflow").await,
        (StatusCode::OK, json!({"default_workflow_permissions": "read", "can_approve_pull_request_reviews": false}))
    );
    assert_eq!(
        get("/repos/alice/rocket/actions/permissions/fork-pr-contributor-approval").await,
        (StatusCode::OK, json!({"approval_policy": "first_time_contributors"}))
    );

    // The workflow token: raised to write, and the jobs' tokens follow.
    let body = json!({"default_workflow_permissions": "write", "can_approve_pull_request_reviews": true});
    assert_eq!(api(&fixture, Method::PUT, &format!("{base}/workflow"), Some(&admin), Some(body.clone())).await.0, StatusCode::NO_CONTENT);
    assert_eq!(get("/repos/alice/rocket/actions/permissions/workflow").await.1, body);
    let repo = fixture.repo().await;
    assert_eq!(
        fixture.actions().repo_settings(&repo).await.unwrap().token_permission,
        DefaultTokenPermission::Write,
        "the settings page agrees"
    );
    for bad in [json!({"default_workflow_permissions": "admin"}), json!({"default_workflow_permissions": 3})] {
        assert_eq!(
            api(&fixture, Method::PUT, &format!("{base}/workflow"), Some(&admin), Some(bad.clone())).await.0,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{bad}"
        );
    }

    // Selected actions: only while `allowed_actions` is `selected`, and the job's spec carries them.
    let selected = json!({"github_owned_allowed": false, "verified_allowed": true, "patterns_allowed": ["octo-org/*"]});
    assert_eq!(
        api(&fixture, Method::PUT, &format!("{base}/selected-actions"), Some(&admin), Some(selected.clone())).await.0,
        StatusCode::CONFLICT
    );
    let body = json!({"enabled": true, "allowed_actions": "selected"});
    assert_eq!(api(&fixture, Method::PUT, base, Some(&admin), Some(body)).await.0, StatusCode::NO_CONTENT);
    assert_eq!(
        api(&fixture, Method::PUT, &format!("{base}/selected-actions"), Some(&admin), Some(selected)).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        get("/repos/alice/rocket/actions/permissions/selected-actions").await.1,
        json!({"github_owned_allowed": false, "verified_allowed": false, "patterns_allowed": ["octo-org/*"]})
    );
    let shown = get("/repos/alice/rocket/actions/permissions").await.1;
    assert_eq!(shown["allowed_actions"], "selected");
    assert!(shown["selected_actions_url"].as_str().unwrap().ends_with("/repos/alice/rocket/actions/permissions/selected-actions"));
    let bad = json!({"patterns_allowed": ["no slash"]});
    assert_eq!(
        api(&fixture, Method::PUT, &format!("{base}/selected-actions"), Some(&admin), Some(bad)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let credential = fixture.runner(&["linux"]).await;
    seed(&fixture, &[payload(&template("build", &["linux"], &[]))]).await;
    let job = spec(&poll(&fixture, &credential, Some("a")).await.1);
    let rules = serde_json::to_value(&job.actions_policy).unwrap();
    assert_eq!(rules, json!({"rules": [{"selected": {"github_owned": false, "patterns": ["octo-org/*"]}}]}), "the owner adds no rule");

    // The approval policy, and an unknown one.
    let body = json!({"approval_policy": "all_external_contributors"});
    assert_eq!(
        api(&fixture, Method::PUT, &format!("{base}/fork-pr-contributor-approval"), Some(&admin), Some(body)).await.0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        get("/repos/alice/rocket/actions/permissions/fork-pr-contributor-approval").await.1["approval_policy"],
        "all_external_contributors"
    );
    let body = json!({"approval_policy": "everyone"});
    assert_eq!(
        api(&fixture, Method::PUT, &format!("{base}/fork-pr-contributor-approval"), Some(&admin), Some(body)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Turning Actions off through the API stops runs; `enabled` is required.
    assert_eq!(
        api(&fixture, Method::PUT, base, Some(&admin), Some(json!({"allowed_actions": "all"}))).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(api(&fixture, Method::PUT, base, Some(&admin), Some(json!({"enabled": false}))).await.0, StatusCode::NO_CONTENT);
    assert_eq!(get("/repos/alice/rocket/actions/permissions").await.1["enabled"], false);
    assert!(!fixture.database.actions_settings().is_enabled(repo.repo.repo.id).await.unwrap());
}

#[tokio::test]
async fn owners_cap_their_repositories() {
    let fixture = Fixture::new().await;
    let admin = token(&fixture, &fixture.alice).await;
    let repo = fixture.repo().await;

    // The user's own ceilings: nothing capped until set.
    assert_eq!(
        api(&fixture, Method::GET, "/user/actions/permissions", Some(&admin), None).await,
        (StatusCode::OK, json!({"enabled_repositories": "all", "allowed_actions": "all", "sha_pinning_required": false}))
    );
    assert_eq!(
        api(&fixture, Method::GET, "/user/actions/permissions/workflow", Some(&admin), None).await.1,
        json!({"default_workflow_permissions": "write", "can_approve_pull_request_reviews": true})
    );
    let read_only = json!({"default_workflow_permissions": "read", "can_approve_pull_request_reviews": false});
    assert_eq!(
        api(&fixture, Method::PUT, "/user/actions/permissions/workflow", Some(&admin), Some(read_only)).await.0,
        StatusCode::NO_CONTENT
    );
    let raise = json!({"default_workflow_permissions": "write"});
    let (status, body) = api(&fixture, Method::PUT, "/repos/alice/rocket/actions/permissions/workflow", Some(&admin), Some(raise)).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["message"].as_str().unwrap().contains("default_workflow_permissions"));
    let local = json!({"enabled_repositories": "all", "allowed_actions": "local_only"});
    assert_eq!(api(&fixture, Method::PUT, "/user/actions/permissions", Some(&admin), Some(local)).await.0, StatusCode::NO_CONTENT);
    let wider = json!({"enabled": true, "allowed_actions": "selected"});
    assert_eq!(
        api(&fixture, Method::PUT, "/repos/alice/rocket/actions/permissions", Some(&admin), Some(wider)).await.0,
        StatusCode::CONFLICT
    );
    let effective = fixture.actions().effective_policies(repo.repo.repo.id).await.unwrap();
    assert_eq!(effective.token_permission, DefaultTokenPermission::Read);

    // Selected repositories: the switch is on, but only a selected repository runs.
    let selected = json!({"enabled_repositories": "selected"});
    assert_eq!(api(&fixture, Method::PUT, "/user/actions/permissions", Some(&admin), Some(selected)).await.0, StatusCode::NO_CONTENT);
    assert!(!fixture.database.actions_settings().is_enabled(repo.repo.repo.id).await.unwrap());
    let (status, body) = api(&fixture, Method::GET, "/user/actions/permissions", Some(&admin), None).await;
    assert_eq!((status, body["selected_repositories_url"].as_str().is_some()), (StatusCode::OK, true));
    let id = repo.repo.repo.id.0;
    assert_eq!(
        api(&fixture, Method::PUT, &format!("/user/actions/permissions/repositories/{id}"), Some(&admin), None).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(fixture.database.actions_settings().is_enabled(repo.repo.repo.id).await.unwrap());
    let listed = api(&fixture, Method::GET, "/user/actions/permissions/repositories", Some(&admin), None).await.1;
    assert_eq!(listed, json!({"total_count": 1, "repositories": [{"id": id, "name": "rocket", "full_name": "alice/rocket"}]}));
    let ids = json!({"selected_repository_ids": []});
    assert_eq!(
        api(&fixture, Method::PUT, "/user/actions/permissions/repositories", Some(&admin), Some(ids)).await.0,
        StatusCode::NO_CONTENT
    );
    assert!(!fixture.database.actions_settings().is_enabled(repo.repo.repo.id).await.unwrap());

    // An organisation's endpoints are its owners'; another account's repository is not theirs to select.
    let org = CreateOrganization { name: "acme".into(), display_name: String::new(), description: String::new() };
    fixture.services.orgs.create(&fixture.alice, org).await.unwrap();
    assert_eq!(api(&fixture, Method::GET, "/orgs/acme/actions/permissions", Some(&admin), None).await.0, StatusCode::OK);
    let ids = json!({"selected_repository_ids": [id]});
    assert_eq!(
        api(&fixture, Method::PUT, "/orgs/acme/actions/permissions/repositories", Some(&admin), Some(ids)).await.0,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn only_administrators_see_or_change_permissions() {
    let fixture = Fixture::new().await;
    let bob = fixture
        .services
        .accounts
        .create_user(
            SignUp { username: "bob".into(), email: "bob@example.com".into(), password: "correct horse battery".into() },
            Some(false),
        )
        .await
        .unwrap();
    let input = CreateRepository { name: "garden".into(), description: String::new(), visibility: rbit_core::Visibility::Public };
    fixture.services.repositories.create(&bob, input).await.unwrap();
    let bob_token = token(&fixture, &bob).await;
    let path = "/repos/alice/rocket/actions/permissions";
    assert_eq!(api(&fixture, Method::GET, path, None, None).await.0, StatusCode::NOT_FOUND, "anonymous: a private repository");
    assert_eq!(api(&fixture, Method::GET, path, Some(&bob_token), None).await.0, StatusCode::NOT_FOUND, "an outsider learns nothing");
    assert_eq!(api(&fixture, Method::GET, "/repos/bob/garden/actions/permissions", None, None).await.0, StatusCode::UNAUTHORIZED);
    // Alice is the instance's administrator here; Bob only reads her public repository.
    let input = CreateRepository { name: "open".into(), description: String::new(), visibility: rbit_core::Visibility::Public };
    fixture.services.repositories.create(&fixture.alice, input).await.unwrap();
    let body = Some(json!({"enabled": false}));
    assert_eq!(api(&fixture, Method::PUT, "/repos/alice/open/actions/permissions", Some(&bob_token), body).await.0, StatusCode::FORBIDDEN);
    assert_eq!(
        api(&fixture, Method::GET, "/repos/alice/open/actions/permissions/workflow", Some(&bob_token), None).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(api(&fixture, Method::GET, "/orgs/nobody/actions/permissions", Some(&bob_token), None).await.0, StatusCode::NOT_FOUND);
}

// Actions: reusable workflows.
#[tokio::test]
async fn who_may_call_a_private_repositorys_workflows() {
    let fixture = Fixture::new().await;
    let admin = token(&fixture, &fixture.alice).await;
    let path = "/repos/alice/rocket/actions/permissions/access";
    assert_eq!(api(&fixture, Method::GET, path, Some(&admin), None).await, (StatusCode::OK, json!({"access_level": "none"})));
    assert_eq!(api(&fixture, Method::PUT, path, Some(&admin), Some(json!({"access_level": "user"}))).await.0, StatusCode::NO_CONTENT);
    assert_eq!(api(&fixture, Method::GET, path, Some(&admin), None).await.1, json!({"access_level": "user"}));
    let refused = api(&fixture, Method::PUT, path, Some(&admin), Some(json!({"access_level": "everyone"}))).await.0;
    assert_eq!(refused, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(api(&fixture, Method::GET, path, None, None).await.0, StatusCode::NOT_FOUND);
    assert_eq!(
        api(&fixture, Method::GET, "/user/actions/permissions/access", Some(&admin), None).await.0,
        StatusCode::NOT_FOUND,
        "repositories only"
    );
}

// Actions: workflow_run and repository_dispatch.
#[tokio::test]
async fn writers_dispatch_repository_events() {
    let fixture = Fixture::new().await;
    let admin = token(&fixture, &fixture.alice).await;
    let path = "/repos/alice/rocket/dispatches";
    let body = json!({"event_type": "deploy", "client_payload": {"env": "staging"}});
    assert_eq!(api(&fixture, Method::POST, path, Some(&admin), Some(body)).await.0, StatusCode::NO_CONTENT, "nothing to run is fine");
    assert_eq!(api(&fixture, Method::POST, path, Some(&admin), Some(json!({}))).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let long = json!({"event_type": "x".repeat(101)});
    assert_eq!(api(&fixture, Method::POST, path, Some(&admin), Some(long)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    let list = json!({"event_type": "deploy", "client_payload": [1]});
    assert_eq!(api(&fixture, Method::POST, path, Some(&admin), Some(list)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(api(&fixture, Method::POST, path, None, Some(json!({"event_type": "deploy"}))).await.0, StatusCode::NOT_FOUND);
}
