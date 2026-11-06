//! Valid personal tokens bypass password capacity for real Git and LFS requests.

use axum::http::{Method, StatusCode, header};
use rbit_kernel::AdmissionError;

use super::fixture::Fixture;

#[tokio::test]
async fn personal_tokens_keep_git_lfs_api_and_health_available_when_password_work_is_busy_or_closed() {
    for closed in [false, true] {
        let fixture = Fixture::new().await;
        let held = if closed {
            fixture.passwords.close();
            None
        } else {
            Some(fixture.passwords.begin().unwrap())
        };
        let basic = fixture.basic_pat_header();
        let (status, headers, body) =
            fixture.request(Method::GET, "/keeper/project.git/info/refs?service=git-upload-pack", &[("authorization", &basic)], "").await;
        assert_eq!(status, StatusCode::OK, "a valid PAT must not fall back to password admission");
        assert_eq!(headers[header::CONTENT_TYPE], "application/x-git-upload-pack-advertisement");
        assert!(!headers.contains_key(header::WWW_AUTHENTICATE));
        assert!(body.contains("# service=git-upload-pack\n"), "the real public repository is advertised by Git");

        let (status, headers, body) = fixture
            .request(
                Method::POST,
                "/keeper/project.git/info/lfs/objects/batch",
                &[("authorization", &basic), ("content-type", "application/vnd.git-lfs+json")],
                r#"{"operation":"download","objects":[]}"#,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "LFS Basic PAT authentication must bypass password capacity");
        assert_eq!(headers[header::CONTENT_TYPE], "application/vnd.git-lfs+json");
        assert!(!headers.contains_key(header::WWW_AUTHENTICATE));
        assert!(!headers.contains_key("lfs-authenticate"));
        let batch: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(batch["objects"], serde_json::json!([]));

        let bearer = format!("Bearer {}", fixture.token);
        let (status, _, body) = fixture.request(Method::GET, "/api/v1/user", &[("authorization", &bearer)], "").await;
        assert_eq!(status, StatusCode::OK);
        let account: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(account["login"], "keeper");
        let (status, _, _) = fixture.request(Method::GET, "/healthz", &[], "").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            fixture.passwords.begin().unwrap_err(),
            if closed { AdmissionError::Closed } else { AdmissionError::Busy },
            "token requests cannot reopen admission or release the held password slot"
        );
        drop(held);
    }
}
