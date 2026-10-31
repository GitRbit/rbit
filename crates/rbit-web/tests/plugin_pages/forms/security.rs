//! Proofs never substitute for live host permission and session checks.

use super::*;
use std::{future::Future as _, pin::Pin, task::Poll};

async fn refused(response: axum::response::Response, status: StatusCode, calls: &AtomicUsize) {
    assert_eq!(response.status(), status);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "refused input never reaches native code");
    let _ = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
}

#[tokio::test]
async fn runtime_form_mutations_proofs_do_not_grant_admin_or_private_repository_access() {
    for audience in [MutationAudience::SiteAdmin, MutationAudience::RepositoryAdmin, MutationAudience::RepositoryWriter] {
        let fixture = FormFixture::new(audience).await;
        let path = fixture.path("rocket", "save");
        let token = fixture.proof("rocket", &fixture.base.admin_cookie).await;
        let (_, _, html) = fixture.base.get(&fixture.path("rocket", "form"), Some(&fixture.base.user_cookie)).await;
        assert!(proof(&html).is_none(), "unauthorized viewers receive no signed action");
        refused(
            post(&fixture.base, &path, Some(&fixture.base.user_cookie), &form_body(&token, "refused"), &[]).await,
            StatusCode::NOT_FOUND,
            &fixture.calls,
        )
        .await;
        if fixture.page_scope == PageScope::Repository {
            refused(
                post(&fixture.base, &fixture.path("secret", "save"), None, &form_body(&token, "hidden"), &[]).await,
                StatusCode::NOT_FOUND,
                &fixture.calls,
            )
            .await;
        }
        fixture.base.services.shutdown().await;
    }
}

#[tokio::test]
async fn runtime_form_mutations_bind_the_active_session_repository_and_host_boot() {
    let fixture = FormFixture::new(MutationAudience::SignedIn).await;
    let token = fixture.proof("rocket", &fixture.base.user_cookie).await;
    let (_, _, anonymous) = fixture.base.get(&fixture.path("rocket", "form"), None).await;
    assert!(proof(&anonymous).is_none());
    let second_session = fixture.base.services.accounts.start_session(&fixture.base.user).await.unwrap();
    let second_cookie = format!("rbit_session={}", second_session.encode());
    refused(
        post(
            &fixture.base,
            &fixture.path("rocket", "save"),
            Some(&second_cookie),
            &form_body(&token, "same actor, different session"),
            &[],
        )
        .await,
        StatusCode::FORBIDDEN,
        &fixture.calls,
    )
    .await;
    refused(
        post(&fixture.base, &fixture.path("rocket", "save"), Some(&fixture.base.admin_cookie), &form_body(&token, "wrong session"), &[])
            .await,
        StatusCode::FORBIDDEN,
        &fixture.calls,
    )
    .await;
    let recreated =
        rbit_web::router(rbit_web::AppState::new(Arc::clone(&fixture.base.services.context().config), fixture.base.services.clone()));
    let request =
        post_request(&fixture.path("rocket", "save"), Some(&fixture.base.user_cookie), Body::from(form_body(&token, "previous host")));
    refused(recreated.oneshot(request).await.unwrap(), StatusCode::FORBIDDEN, &fixture.calls).await;
    fixture.base.services.shutdown().await;

    let fixture = FormFixture::new(MutationAudience::RepositoryAdmin).await;
    let token = fixture.proof("rocket", &fixture.base.admin_cookie).await;
    refused(
        post(
            &fixture.base,
            &fixture.path("secret", "save"),
            Some(&fixture.base.admin_cookie),
            &form_body(&token, "different repository"),
            &[],
        )
        .await,
        StatusCode::FORBIDDEN,
        &fixture.calls,
    )
    .await;
    fixture.base.services.shutdown().await;
}

#[tokio::test]
async fn runtime_form_mutations_replacement_and_reused_generation_invalidate_old_proofs() {
    let fixture = FormFixture::new(MutationAudience::SignedIn).await;
    let path = fixture.path("rocket", "save");
    let old = fixture.proof("rocket", &fixture.base.user_cookie).await;
    fixture.mutations.replace(vec![mutation(fixture.audience, &fixture.calls, &fixture.observed)]).unwrap();
    refused(
        post(&fixture.base, &path, Some(&fixture.base.user_cookie), &form_body(&old, "old registration"), &[]).await,
        StatusCode::FORBIDDEN,
        &fixture.calls,
    )
    .await;
    let previous_activation = fixture.proof("rocket", &fixture.base.user_cookie).await;
    fixture.scope.dispose(Duration::from_secs(2)).await.unwrap();
    // A host accidentally reusing a numeric generation still gets a new registration version.
    let next_scope = super::super::scope(fixture.scope.generation().get());
    let pages = fixture
        .base
        .services
        .page_registry()
        .stage(
            OWNER.parse().unwrap(),
            &next_scope,
            next_scope.generation(),
            vec![compiled_form_page(
                PageDescriptor::new("form", PageScope::Global, "form", PageAudience::Public, Duration::from_secs(1))
                    .unwrap()
                    .with_forms(vec![FormReference::new("save", "save").unwrap()])
                    .unwrap(),
                |_, actions| async move {
                    CompiledPage::new("Reactivated", FormView { form: actions.get("save").cloned(), message: String::new() })
                },
            )],
        )
        .unwrap();
    let mutations = fixture
        .base
        .services
        .mutation_registry()
        .stage(
            OWNER.parse().unwrap(),
            &next_scope,
            next_scope.generation(),
            vec![mutation(fixture.audience, &fixture.calls, &fixture.observed)],
        )
        .unwrap();
    mutations.publish().unwrap();
    pages.publish().unwrap();
    refused(
        post(&fixture.base, &path, Some(&fixture.base.user_cookie), &form_body(&previous_activation, "retired activation"), &[]).await,
        StatusCode::FORBIDDEN,
        &fixture.calls,
    )
    .await;
    assert!(!fixture.mutations.unpublish());
    assert!(!fixture.pages.unpublish());
    let current = fixture.proof("rocket", &fixture.base.user_cookie).await;
    assert_eq!(
        post(&fixture.base, &path, Some(&fixture.base.user_cookie), &form_body(&current, "current"), &[]).await.status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    next_scope.dispose(Duration::from_secs(2)).await.unwrap();
    fixture.base.services.shutdown().await;
}

#[tokio::test]
async fn runtime_form_mutations_repository_id_prevents_delete_and_recreate_aba() {
    let fixture = FormFixture::new(MutationAudience::RepositoryAdmin).await;
    let token = fixture.proof("rocket", &fixture.base.admin_cookie).await;
    let original = fixture.base.services.repositories.open(Some(&fixture.base.admin), "alice", "rocket").await.unwrap();
    let original_id = original.repo.repo.id;
    fixture.base.services.repo_settings.delete(&fixture.base.admin, &original, "alice/rocket").await.unwrap();
    let replacement = fixture
        .base
        .services
        .repositories
        .create(&fixture.base.admin, CreateRepository { name: "rocket".into(), description: String::new(), visibility: Visibility::Public })
        .await
        .unwrap();
    assert_ne!(replacement.repo.id, original_id);
    refused(
        post(&fixture.base, &fixture.path("rocket", "save"), Some(&fixture.base.admin_cookie), &form_body(&token, "old repository"), &[])
            .await,
        StatusCode::FORBIDDEN,
        &fixture.calls,
    )
    .await;
    fixture.base.services.shutdown().await;
}

struct UploadGate {
    first: Option<axum::body::Bytes>,
    entered: Option<tokio::sync::oneshot::Sender<()>>,
    rest: Option<tokio::sync::oneshot::Receiver<axum::body::Bytes>>,
}
impl http_body::Body for UploadGate {
    type Data = axum::body::Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        if let Some(first) = self.first.take() {
            let _ = self.entered.take().unwrap().send(());
            return Poll::Ready(Some(Ok(http_body::Frame::data(first))));
        }
        let Some(rest) = self.rest.as_mut() else { return Poll::Ready(None) };
        match Pin::new(rest).poll(cx) {
            Poll::Ready(result) => {
                self.rest = None;
                Poll::Ready(result.ok().map(|bytes| Ok(http_body::Frame::data(bytes))))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

#[tokio::test]
async fn runtime_form_mutations_recheck_logout_and_archival_after_the_upload() {
    for archive in [false, true] {
        let fixture = FormFixture::new(if archive { MutationAudience::RepositoryWriter } else { MutationAudience::SignedIn }).await;
        let cookie = &fixture.base.admin_cookie;
        let token = fixture.proof("rocket", cookie).await;
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let (finish, rest) = tokio::sync::oneshot::channel();
        let body = Body::new(UploadGate {
            first: Some(axum::body::Bytes::from(format!("_csrf={token}&title="))),
            entered: Some(entered),
            rest: Some(rest),
        });
        let request = post_request(&fixture.path("rocket", "save"), Some(cookie), body);
        let task = tokio::spawn(fixture.base.router.clone().oneshot(request));
        tokio::time::timeout(Duration::from_secs(2), waiting).await.unwrap().unwrap();
        if archive {
            let repository = fixture.base.services.repositories.open(Some(&fixture.base.admin), "alice", "rocket").await.unwrap();
            fixture.base.services.repo_settings.set_archived(&fixture.base.admin, &repository, true).await.unwrap();
        } else {
            let session = rbit_auth::SessionToken::parse(cookie.split_once('=').unwrap().1).unwrap();
            fixture.base.services.accounts.end_session(&session).await.unwrap();
        }
        finish.send(axum::body::Bytes::from_static(b"after-revocation")).unwrap();
        let response = tokio::time::timeout(Duration::from_secs(2), task).await.unwrap().unwrap().unwrap();
        refused(response, if archive { StatusCode::NOT_FOUND } else { StatusCode::FORBIDDEN }, &fixture.calls).await;
        fixture.base.services.shutdown().await;
    }
}

#[tokio::test]
async fn runtime_form_mutations_writer_access_is_effective_permission_not_repository_ownership() {
    let fixture = FormFixture::new(MutationAudience::RepositoryWriter).await;
    let repository = fixture.base.services.repositories.open(Some(&fixture.base.admin), "alice", "secret").await.unwrap();
    fixture
        .base
        .services
        .collaborators
        .add(&fixture.base.admin, &repository, "bob", rbit_core::membership::Permission::Write)
        .await
        .unwrap();
    let token = fixture.proof("secret", &fixture.base.user_cookie).await;
    let response =
        post(&fixture.base, &fixture.path("secret", "save"), Some(&fixture.base.user_cookie), &form_body(&token, "collaborator"), &[])
            .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let _ = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
    {
        let observed = fixture.observed.lock().unwrap();
        let repository = observed[0].0.repository.as_ref().unwrap();
        assert!(repository.can_write);
        assert!(!repository.can_admin);
        assert_eq!(observed[0].0.actor.account_id, fixture.base.user.id().0);
    }
    fixture.base.services.shutdown().await;
}
