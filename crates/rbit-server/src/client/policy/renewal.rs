//! Deliberate bounded epoch renewal; historical evidence never reopens a gate.
use super::*;
use rbit_plugin_api::{MAX_POLICY_RENEWAL_REQUEST_BYTES, PackagePolicyRenewalRequest};

pub(super) async fn renew(api: &Api, path: PathBuf, supplied: Option<String>) -> anyhow::Result<i32> {
    let request = request_file(&path).await?;
    let id = match supplied {
        Some(id) => parse_id(&id)?,
        None => fresh_id()?,
    };
    eprintln!("Policy renewal operation: {}", id.as_str());
    eprintln!("Recover with: rbit plugin policy renewal-operation -- {}", id.as_str());
    eprintln!("Use the same instance (--hostname); also inspect policy status. Missing receipts do not prove rollback.");
    execute(api, &id, Some(request)).await
}
pub(super) async fn operation(api: &Api, id: String) -> anyhow::Result<i32> {
    execute(api, &parse_id(&id)?, None).await
}
async fn request_file(path: &Path) -> anyhow::Result<PackagePolicyRenewalRequest> {
    let source = tokio::fs::File::open(path).await.context("cannot open renewal request file")?;
    let mut bytes = Vec::new();
    source.take(MAX_POLICY_RENEWAL_REQUEST_BYTES as u64 + 1).read_to_end(&mut bytes).await.context("cannot read renewal request file")?;
    if bytes.len() > MAX_POLICY_RENEWAL_REQUEST_BYTES {
        bail!("renewal request file exceeds 1 KiB");
    }
    serde_json::from_slice(&bytes).context("invalid strict renewal request file")
}
fn path(id: &PackageOperationId) -> String {
    format!("admin/plugin-policy-renewals/{}", id.as_str())
}
async fn execute(api: &Api, id: &PackageOperationId, request: Option<PackagePolicyRenewalRequest>) -> anyhow::Result<i32> {
    let mut builder = api.request(if request.is_some() { Method::PUT } else { Method::GET }, &path(id))?.timeout(TIMEOUT);
    if let Some(request) = &request {
        builder = builder.json(request);
    }
    let result: anyhow::Result<i32> = async {
        let response = builder.send().await.context("policy renewal response unavailable")?;
        let (status, value) = read_response(response).await?;
        let code = validate_response(status, &value, id, request.as_ref())?;
        println!("{}", serde_json::to_string_pretty(&value)?);
        if code != 0 {
            eprintln!("Renewal returned non-success. Inspect policy status and the retained renewal key; do not infer rollback.");
        } else if value["outcome"] == "historical_receipt" {
            eprintln!("Historical renewal receipt only; no new epoch was opened.");
        }
        Ok(code)
    }
    .await;
    result.context("no automatic retry was made; inspect the same instance's gate and retained renewal key before retrying")
}
fn check_receipt(value: &Value, id: &PackageOperationId, historical: bool) -> anyhow::Result<()> {
    if value["operation_id"].as_str() != Some(id.as_str()) || value["kind"] != "renew" || value["historical"].as_bool() != Some(historical)
    {
        bail!("unexpected renewal receipt identity or outcome");
    }
    for field in ["source_epoch", "destination_epoch"] {
        let _: Sha256Digest = serde_json::from_value(value[field].clone()).context("invalid renewal epoch")?;
    }
    for field in ["source_host_policy", "destination_host_policy"] {
        let _: HostPolicyFingerprint = serde_json::from_value(value[field].clone()).context("invalid renewal fingerprint")?;
    }
    if value["source_epoch"] == value["destination_epoch"] || value["source_host_policy"] == value["destination_host_policy"] {
        bail!("renewal did not change epoch and fingerprint");
    }
    let seal = parse_id(value["expected_seal_operation"].as_str().context("missing renewal seal operation")?)?;
    if seal.as_str() == id.as_str() {
        bail!("renewal receipt reuses its reserved source seal key");
    }
    match value.get("actor_id") {
        Some(Value::Null) => {}
        Some(Value::String(actor)) if actor.parse::<i64>().is_ok_and(|number| number > 0 && number.to_string() == *actor) => {}
        _ => bail!("invalid renewal actor identity"),
    }
    let timestamp = value["created_at"].as_str().context("missing renewal receipt timestamp")?;
    time::OffsetDateTime::parse(timestamp, &time::format_description::well_known::Rfc3339).context("invalid renewal receipt timestamp")?;
    Ok(())
}
fn validate_response(
    status: StatusCode,
    value: &Value,
    id: &PackageOperationId,
    request: Option<&PackagePolicyRenewalRequest>,
) -> anyhow::Result<i32> {
    let retained = status == StatusCode::CONFLICT && value["outcome"] == "retention_full";
    if !status.is_success() && !retained {
        return Ok(1);
    }
    if value["schema_version"].as_u64() != Some(1)
        || value["execution"] != "unavailable"
        || value["operation_id"].as_str() != Some(id.as_str())
    {
        bail!("unrecognized passive renewal response schema or identity");
    }
    if let Some(request) = request {
        check_gate(&value["gate"])?;
        if retained {
            if value["gate"]["state"] != "sealed"
                || value.get("receipt") != Some(&Value::Null)
                || value["gate"]["host_policy"].as_str() != Some(request.expected_host_policy.as_str())
                || value["gate"]["seal"]["operation_id"].as_str() != Some(request.expected_seal_operation.as_str())
            {
                bail!("retention rejection does not match the requested sealed gate");
            }
            return Ok(1);
        }
    }
    let historical = match (status, value["outcome"].as_str(), request.is_some()) {
        (StatusCode::OK, Some("applied"), true) => false,
        (StatusCode::OK, Some("historical_receipt"), _) => true,
        _ => bail!("unexpected renewal outcome; inspect gate and retained key"),
    };
    let receipt = &value["receipt"];
    check_receipt(receipt, id, historical)?;
    if let Some(request) = request {
        if receipt["source_host_policy"].as_str() != Some(request.expected_host_policy.as_str())
            || receipt["expected_seal_operation"].as_str() != Some(request.expected_seal_operation.as_str())
        {
            bail!("renewal receipt differs from the requested source policy or seal");
        }
        if !historical
            && (value["gate"]["state"] != "open"
                || value["gate"]["epoch"] != receipt["destination_epoch"]
                || value["gate"]["host_policy"] != receipt["destination_host_policy"])
        {
            bail!("applied renewal does not match its open destination gate");
        }
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;
    fn request() -> PackagePolicyRenewalRequest {
        PackagePolicyRenewalRequest::new(HostPolicyFingerprint::new("b".repeat(64).parse().unwrap()), "old-seal").unwrap()
    }
    fn success() -> Value {
        json!({"schema_version":1,"execution":"unavailable","operation_id":"known","outcome":"applied","gate":{"state":"open","epoch":"c".repeat(64),"host_policy":"d".repeat(64),"seal":null},"receipt":{"operation_id":"known","kind":"renew","historical":false,"source_epoch":"a".repeat(64),"source_host_policy":"b".repeat(64),"destination_epoch":"c".repeat(64),"destination_host_policy":"d".repeat(64),"expected_seal_operation":"old-seal","actor_id":"9007199254740993","created_at":"2026-11-01T00:00:00Z"},"url":"https://untrusted.invalid/never-follow","gate_url":"https://untrusted.invalid/never-follow"})
    }
    #[tokio::test]
    async fn parser_and_complete_file_bounds_preserve_strict_small_request() {
        for args in [vec!["renew", "--request", "renew.json", "--operation-id", "known"], vec!["renewal-operation", "--", "-retry"]] {
            let mut command = vec!["rbit", "plugin", "policy"];
            command.extend(args);
            assert!(crate::cli::Cli::try_parse_from(command).is_ok());
        }
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("request.json");
        let mut bytes = serde_json::to_vec(&request()).unwrap();
        bytes.resize(MAX_POLICY_RENEWAL_REQUEST_BYTES, b' ');
        tokio::fs::write(&file, &bytes).await.unwrap();
        assert_eq!(request_file(&file).await.unwrap(), request());
        bytes.push(b' ');
        tokio::fs::write(&file, &bytes).await.unwrap();
        assert!(request_file(&file).await.unwrap_err().to_string().contains("exceeds"));
        let mut invalid = serde_json::to_value(request()).unwrap();
        invalid["force"] = json!(true);
        tokio::fs::write(&file, serde_json::to_vec(&invalid).unwrap()).await.unwrap();
        assert!(request_file(&file).await.is_err());
    }
    #[test]
    fn applied_historical_and_retention_full_are_distinct() {
        let id = parse_id("known").unwrap();
        let request = request();
        let mut value = success();
        assert_eq!(validate_response(StatusCode::OK, &value, &id, Some(&request)).unwrap(), 0);
        value["outcome"] = json!("historical_receipt");
        value["receipt"]["historical"] = json!(true);
        value["gate"] = json!({"state":"sealed","epoch":"e".repeat(64),"host_policy":"f".repeat(64),"seal":{"operation_id":"later-seal","reason":"history_capacity"}});
        assert_eq!(validate_response(StatusCode::OK, &value, &id, Some(&request)).unwrap(), 0);
        value.as_object_mut().unwrap().remove("gate");
        assert_eq!(validate_response(StatusCode::OK, &value, &id, None).unwrap(), 0);
        value["outcome"] = json!("applied");
        assert!(validate_response(StatusCode::OK, &value, &id, None).is_err());
        value = json!({"schema_version":1,"execution":"unavailable","operation_id":"known","outcome":"retention_full","receipt":null,"gate":{"state":"sealed","epoch":"a".repeat(64),"host_policy":"b".repeat(64),"seal":{"operation_id":"old-seal","reason":"history_capacity"}}});
        assert_eq!(validate_response(StatusCode::CONFLICT, &value, &id, Some(&request)).unwrap(), 1);
        value["gate"]["seal"]["operation_id"] = json!("different");
        assert!(validate_response(StatusCode::CONFLICT, &value, &id, Some(&request)).is_err());
    }
    #[test]
    fn malformed_success_cannot_acknowledge_a_different_renewal() {
        let id = parse_id("known").unwrap();
        let request = request();
        for (field, bad) in [
            ("operation_id", json!("wrong")),
            ("source_host_policy", json!("e".repeat(64))),
            ("expected_seal_operation", json!("other")),
            ("source_epoch", json!("c".repeat(64))),
            ("destination_host_policy", json!("b".repeat(64))),
            ("actor_id", json!("01")),
            ("created_at", json!("bad")),
        ] {
            let mut value = success();
            value["receipt"][field] = bad;
            assert!(validate_response(StatusCode::OK, &value, &id, Some(&request)).is_err(), "{field}");
        }
        for (field, bad) in [("schema_version", json!(2)), ("execution", json!("running")), ("operation_id", json!("wrong"))] {
            let mut value = success();
            value[field] = bad;
            assert!(validate_response(StatusCode::OK, &value, &id, Some(&request)).is_err());
        }
        let mut value = success();
        value["gate"]["epoch"] = json!("e".repeat(64));
        assert!(validate_response(StatusCode::OK, &value, &id, Some(&request)).is_err());
        let mut value = success();
        value["outcome"] = json!("historical_receipt");
        value["receipt"]["historical"] = json!(true);
        value["receipt"]["expected_seal_operation"] = json!("known");
        value.as_object_mut().unwrap().remove("gate");
        // Recovery has no original request to expose this reserved-key collision.
        assert!(validate_response(StatusCode::OK, &value, &id, None).is_err());
    }
    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    #[tokio::test]
    async fn loopback_uses_raw_fixed_destination_and_recovers_without_replay_or_unbounded_response() {
        use axum::{
            body::{Body, to_bytes},
            http::Uri,
            response::IntoResponse,
            routing::any,
        };
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let base = format!("http://{}/deploy", listener.local_addr().unwrap());
        let mode = Arc::new(AtomicUsize::new(0));
        let handler_mode = Arc::clone(&mode);
        let (sender, mut captured) = tokio::sync::mpsc::channel(8);
        let app = axum::Router::new().fallback(any(move |method: Method, uri: Uri, body: Body| {
            let sender = sender.clone();
            let mode = Arc::clone(&handler_mode);
            async move {
                let bytes = to_bytes(body, MAX_POLICY_RENEWAL_REQUEST_BYTES).await.unwrap();
                sender.send((method.clone(), uri.path().to_owned(), bytes)).await.unwrap();
                match mode.load(Ordering::SeqCst) {
                    1 => (StatusCode::TEMPORARY_REDIRECT, [("location", "/poison")], "{}").into_response(),
                    2 => Body::from_stream(tokio_util::io::ReaderStream::new(std::io::Cursor::new(vec![b'x'; LIMIT + 1]))).into_response(),
                    3 => {
                        let mut value = success();
                        value["receipt"]["source_host_policy"] = json!("f".repeat(64));
                        (StatusCode::OK, axum::Json(value)).into_response()
                    }
                    _ => {
                        let mut value = success();
                        if method == Method::GET {
                            value["outcome"] = json!("historical_receipt");
                            value["receipt"]["historical"] = json!(true);
                            value.as_object_mut().unwrap().remove("gate");
                        }
                        (StatusCode::OK, axum::Json(value)).into_response()
                    }
                }
            }
        }));
        let _server = Server(tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        }));
        rbit_services::webhooks::delivery::install_crypto_provider();
        let api = Api::new(&base, Some("private-token".to_owned())).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("renew.json");
        tokio::fs::write(&file, serde_json::to_vec(&request()).unwrap()).await.unwrap();
        assert_eq!(renew(&api, file.clone(), Some("known".to_owned())).await.unwrap(), 0);
        let (method, destination, bytes) = captured.recv().await.unwrap();
        assert_eq!(method, Method::PUT);
        assert_eq!(destination, "/deploy/api/v1/admin/plugin-policy-renewals/known");
        assert_eq!(serde_json::from_slice::<PackagePolicyRenewalRequest>(&bytes).unwrap(), request());
        assert_eq!(operation(&api, "known".to_owned()).await.unwrap(), 0);
        let (method, destination, bytes) = captured.recv().await.unwrap();
        assert_eq!(method, Method::GET);
        assert_eq!(destination, "/deploy/api/v1/admin/plugin-policy-renewals/known");
        assert_eq!(bytes.len(), 0);
        mode.store(1, Ordering::SeqCst);
        assert_eq!(renew(&api, file.clone(), Some("known".to_owned())).await.unwrap(), 1);
        captured.recv().await.unwrap();
        assert!(captured.try_recv().is_err(), "no redirect replay or server URL follow");
        mode.store(2, Ordering::SeqCst);
        assert!(operation(&api, "known".to_owned()).await.unwrap_err().to_string().contains("no automatic retry"));
        captured.recv().await.unwrap();
        mode.store(3, Ordering::SeqCst);
        assert!(renew(&api, file, Some("known".to_owned())).await.is_err());
        captured.recv().await.unwrap();
    }
}
