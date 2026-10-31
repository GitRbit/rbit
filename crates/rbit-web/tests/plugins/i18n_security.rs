//! Host identity and upload authorization stay current across browser transitions.

use super::*;
use std::{future::Future as _, pin::Pin, task::Poll, time::Duration};

fn browser_cookie(headers: &axum::http::HeaderMap) -> String {
    headers
        .get_all(header::SET_COOKIE)
        .iter()
        .find_map(|value| {
            let value = value.to_str().unwrap();
            value.starts_with("rbit-plugin-browser=").then(|| value.split(';').next().unwrap().to_owned())
        })
        .unwrap()
}

#[tokio::test]
async fn native_i18n_routes_guest_and_authenticated_proofs_cannot_change_identity() {
    let fixture = Fixture::new().await;
    fixture.services.install_language_pack(&fixture.admin, &language_pack("ar", "rtl", "استكشاف")).await.unwrap();
    fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
    let (guest, headers) = native_form(&fixture, LANGUAGE, None, LANGUAGE).await;
    let browser = browser_cookie(&headers);
    let both = format!("{}; {browser}", fixture.admin_cookie);
    let (status, _, headers) =
        fixture.request(Method::POST, LANGUAGE, Some(&both), "http://localhost:3000", &format!("_csrf={guest}&locale=ar")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!headers.get_all(header::SET_COOKIE).iter().any(|value| value.to_str().unwrap().starts_with("rbit-locale=")));
    assert_eq!(fixture.get("/").await.2[header::CONTENT_LANGUAGE], "en", "a guest form cannot persist an account preference after login");

    let (signed_in, _) = native_form(&fixture, LANGUAGE, Some(&fixture.admin_cookie), LANGUAGE).await;
    let (status, _, headers) =
        fixture.request(Method::POST, LANGUAGE, Some(&browser), "http://localhost:3000", &format!("_csrf={signed_in}&locale=ar")).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(!headers.get_all(header::SET_COOKIE).iter().any(|value| value.to_str().unwrap().starts_with("rbit-locale=")));
    fixture.services.shutdown().await;
}

struct PausedUpload {
    first: Option<axum::body::Bytes>,
    entered: Option<tokio::sync::oneshot::Sender<()>>,
    rest: Option<tokio::sync::oneshot::Receiver<axum::body::Bytes>>,
}

impl http_body::Body for PausedUpload {
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
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                self.rest = None;
                Poll::Ready(result.ok().map(|bytes| Ok(http_body::Frame::data(bytes))))
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Transition {
    Logout,
    DisableProvider,
    Shutdown,
}

#[tokio::test]
async fn native_i18n_routes_uploads_recheck_sessions_survive_provider_disable_and_cancel_on_shutdown() {
    for transition in [Transition::Logout, Transition::DisableProvider, Transition::Shutdown] {
        let fixture = Fixture::new().await;
        fixture.services.set_i18n_enabled(&fixture.admin, true).await.unwrap();
        let (proof, _) = native_form(&fixture, PACKS, Some(&fixture.admin_cookie), INSTALL).await;
        let wire = pack_wire(&proof, &language_pack("ar", "rtl", "استكشاف"));
        let split = wire.len() / 2;
        let (entered, waiting) = tokio::sync::oneshot::channel();
        let (finish, rest) = tokio::sync::oneshot::channel();
        let body = Body::new(PausedUpload {
            first: Some(axum::body::Bytes::copy_from_slice(&wire[..split])),
            entered: Some(entered),
            rest: Some(rest),
        });
        let request = Request::builder()
            .method(Method::POST)
            .uri(INSTALL)
            .header(header::HOST, "localhost:3000")
            .header(header::ORIGIN, "http://localhost:3000")
            .header(header::COOKIE, &fixture.admin_cookie)
            .header(header::CONTENT_TYPE, "multipart/form-data; boundary=rbitpack")
            .body(body)
            .unwrap();
        let task = tokio::spawn(fixture.router.clone().oneshot(request));
        tokio::time::timeout(Duration::from_secs(2), waiting).await.unwrap().unwrap();
        let expected = match transition {
            Transition::Logout => {
                let session = rbit_auth::SessionToken::parse(fixture.admin_cookie.split_once('=').unwrap().1).unwrap();
                fixture.services.accounts.end_session(&session).await.unwrap();
                StatusCode::FORBIDDEN
            }
            Transition::DisableProvider => {
                fixture.services.set_i18n_enabled(&fixture.admin, false).await.unwrap();
                StatusCode::SEE_OTHER
            }
            Transition::Shutdown => {
                fixture.services.prepare_shutdown();
                StatusCode::SERVICE_UNAVAILABLE
            }
        };
        if !matches!(transition, Transition::Shutdown) {
            finish.send(axum::body::Bytes::copy_from_slice(&wire[split..])).unwrap();
        }
        let response = tokio::time::timeout(Duration::from_secs(2), task).await.unwrap().unwrap().unwrap();
        assert_eq!(response.status(), expected, "{transition:?}");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let _ = to_bytes(response.into_body(), 2 * 1024 * 1024).await.unwrap();
        let packs = fixture.services.language_packs(&fixture.admin).await.unwrap();
        assert_eq!(packs.len(), usize::from(matches!(transition, Transition::DisableProvider)));
        if matches!(transition, Transition::DisableProvider) {
            assert!(!fixture.services.i18n_enabled());
            assert_eq!(fixture.get(PACKS).await.0, StatusCode::OK, "installed management survives active provider cleanup");
        }
        fixture.services.shutdown().await;
    }
}
