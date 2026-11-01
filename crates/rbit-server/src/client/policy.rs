//! Remote passive policy administration. No response authorizes execution.
use super::{
    Api,
    packages::{fresh_id, parse_id},
};
use anyhow::{Context as _, bail};
use clap::Subcommand;
use rbit_plugin_api::{
    HostPolicyFingerprint, InstalledPackageRef, MAX_POLICY_REQUEST_BYTES, PackageReviewRequest, PackageSelectionClearRequest,
    PackageSelectionRequest, PluginId, PolicyRevision, Sha256Digest,
};
use rbit_services::PackageOperationId;
use reqwest::{Method, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::io::AsyncReadExt as _;

mod renewal;

const ROOT: &str = "admin/plugin-policy";
const LIMIT: usize = 64 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Subcommand)]
pub enum PolicyCommand {
    /// Inspect the gate; never initialize it.
    Status,
    /// Explicitly initialize virgin policy metadata; never reopen a sealed gate.
    Prepare,
    /// Inspect selected/cleared state for a named plugin.
    Inspect { plugin_id: String },
    /// Read authoritative declarations for an exact installed archive.
    InspectPackage { plugin_id: String, full_version: String, archive_digest: String },
    /// Record explicit review intent, never execution authority.
    Review {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        operation_id: Option<String>,
    },
    /// Select an exact reviewed candidate; never enable or run it.
    Select {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        operation_id: Option<String>,
    },
    /// Clear a selection, retaining its revision tombstone; does not revoke review.
    Clear {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        operation_id: Option<String>,
    },
    /// Deliberately renew a sealed epoch; never inherit reviews or selections.
    Renew {
        #[arg(long)]
        request: PathBuf,
        #[arg(long)]
        operation_id: Option<String>,
    },
    /// Recover an immutable renewal receipt; never reopen an epoch.
    RenewalOperation { id: String },
    /// Read an immutable receipt; 404 does not prove rollback or absence of a seal.
    Operation { id: String },
}

enum Expected {
    Gate,
    Prepared,
    Candidate(PluginId),
    Package(InstalledPackageRef),
    Mutation(PackageOperationId, &'static str, Value),
    Receipt(PackageOperationId),
}

pub(super) async fn run(api: &Api, command: PolicyCommand) -> anyhow::Result<i32> {
    let (method, path, body, expected) = match command {
        PolicyCommand::Renew { request, operation_id } => return renewal::renew(api, request, operation_id).await,
        PolicyCommand::RenewalOperation { id } => return renewal::operation(api, id).await,
        PolicyCommand::Status => (Method::GET, ROOT.to_owned(), None, Expected::Gate),
        PolicyCommand::Prepare => {
            eprintln!("Prepare has no receipt. Recover by running rbit plugin policy status with the same instance (--hostname).");
            (Method::POST, format!("{ROOT}/prepare"), Some(json!({"schema_version":1})), Expected::Prepared)
        }
        PolicyCommand::Inspect { plugin_id } => {
            let id: rbit_plugin_api::PluginId = plugin_id.parse().context("invalid plugin identity")?;
            (Method::GET, format!("{ROOT}/candidates/{}", segment(id.as_str())), None, Expected::Candidate(id))
        }
        PolicyCommand::InspectPackage { plugin_id, full_version, archive_digest } => {
            let package: InstalledPackageRef =
                serde_json::from_value(json!({"plugin_id":plugin_id,"version":full_version,"archive_digest":archive_digest}))
                    .context("invalid exact package identity")?;
            (Method::GET, package_path(&package), None, Expected::Package(package))
        }
        PolicyCommand::Operation { id } => {
            let id = parse_id(&id)?;
            (Method::GET, operation_path(&id), None, Expected::Receipt(id))
        }
        PolicyCommand::Review { request, operation_id } => {
            mutation("review", request_file::<PackageReviewRequest>(&request).await?, operation_id)?
        }
        PolicyCommand::Select { request, operation_id } => {
            mutation("select", request_file::<PackageSelectionRequest>(&request).await?, operation_id)?
        }
        PolicyCommand::Clear { request, operation_id } => {
            mutation("clear", request_file::<PackageSelectionClearRequest>(&request).await?, operation_id)?
        }
    };
    let mut request = api.request(method, &path)?.timeout(TIMEOUT);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let result: anyhow::Result<i32> = async {
        let response = request.send().await.context("policy response unavailable")?;
        let (status, value) = read_response(response).await?;
        let code = validate_response(status, &value, &expected)?;
        // Preserve committed-seal evidence even when the requested mutation failed.
        println!("{}", serde_json::to_string_pretty(&value)?);
        if code != 0 {
            if value["outcome"].as_str() == Some("capacity_sealed") {
                eprintln!("Requested mutation rejected; persistent policy seal committed. Execution remains unavailable.");
            } else {
                eprintln!("Policy request returned non-success. Inspect policy status and the retained key; do not infer rollback.");
            }
        }
        Ok(code)
    }
    .await;
    result.context("no automatic retry was made; inspect the same instance's gate and retained operation key before retrying")
}
fn mutation<T: Serialize>(
    kind: &'static str,
    request: T,
    supplied: Option<String>,
) -> anyhow::Result<(Method, String, Option<Value>, Expected)> {
    let body = json!({"command":kind,"request":request});
    if serde_json::to_vec(&body)?.len() > LIMIT {
        bail!("policy command envelope exceeds 64 KiB");
    }
    let id = match supplied {
        Some(id) => parse_id(&id)?,
        None => fresh_id()?,
    };
    eprintln!("Policy operation: {}", id.as_str());
    eprintln!("Recover with: rbit plugin policy operation -- {}", id.as_str());
    eprintln!("Use the same instance (--hostname); also inspect rbit plugin policy status. Missing receipts do not prove rollback.");
    let expected_request = body["request"].clone();
    Ok((Method::PUT, operation_path(&id), Some(body), Expected::Mutation(id, kind, expected_request)))
}
async fn request_file<T: DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let source = tokio::fs::File::open(path).await.context("cannot open policy request file")?;
    let mut bytes = Vec::new();
    source.take(MAX_POLICY_REQUEST_BYTES as u64 + 1).read_to_end(&mut bytes).await.context("cannot read policy request file")?;
    if bytes.len() > MAX_POLICY_REQUEST_BYTES {
        bail!("policy request file exceeds 64 KiB");
    }
    serde_json::from_slice(&bytes).context("invalid strict policy request file")
}
// Encode each validated segment independently, including '+' in full SemVer.
fn segment(value: &str) -> String {
    let mut result = String::new();
    let hex = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(char::from(byte));
        } else {
            result.push('%');
            result.push(char::from(hex[usize::from(byte >> 4)]));
            result.push(char::from(hex[usize::from(byte & 15)]));
        }
    }
    result
}
fn package_path(package: &InstalledPackageRef) -> String {
    format!(
        "{ROOT}/packages/{}/versions/{}/archives/{}",
        segment(package.plugin_id().as_str()),
        segment(&package.version().to_string()),
        segment(package.archive_digest().as_str())
    )
}
fn operation_path(id: &PackageOperationId) -> String {
    format!("admin/plugin-policy-operations/{}", id.as_str())
}
async fn read_response(mut response: reqwest::Response) -> anyhow::Result<(StatusCode, Value)> {
    let status = response.status();
    if response.content_length().is_some_and(|n| n > LIMIT as u64) {
        bail!("policy response exceeds 64 KiB");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.context("cannot read policy response")? {
        if chunk.len() > LIMIT - bytes.len() {
            bail!("policy response exceeds 64 KiB");
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok((status, serde_json::from_slice(&bytes).context("policy response is not JSON")?))
}
fn check_gate(value: &Value) -> anyhow::Result<()> {
    match value["state"].as_str() {
        Some("uninitialized") => {
            if value.get("epoch") != Some(&Value::Null)
                || value.get("host_policy") != Some(&Value::Null)
                || value.get("seal") != Some(&Value::Null)
            {
                bail!("invalid uninitialized gate");
            }
        }
        Some("open" | "sealed") => {
            let _: Sha256Digest = serde_json::from_value(value["epoch"].clone()).context("invalid gate epoch")?;
            let _: HostPolicyFingerprint = serde_json::from_value(value["host_policy"].clone()).context("invalid gate fingerprint")?;
            if value["state"] == "open" {
                if value.get("seal") != Some(&Value::Null) {
                    bail!("open gate contains seal");
                }
            } else {
                if !matches!(
                    value["seal"]["reason"].as_str(),
                    Some(
                        "history_capacity" | "receipt_capacity" | "review_head_capacity" | "selection_head_capacity" | "revision_exhausted"
                    )
                ) {
                    bail!("unrecognized seal reason");
                }
                parse_id(value["seal"]["operation_id"].as_str().context("missing seal operation")?)?;
            }
        }
        _ => bail!("unrecognized policy gate"),
    }
    Ok(())
}
fn check_receipt(value: &Value, id: &PackageOperationId, historical: bool) -> anyhow::Result<()> {
    if value["operation_id"].as_str() != Some(id.as_str()) || value["historical"].as_bool() != Some(historical) {
        bail!("unexpected policy receipt identity or outcome");
    }
    let plugin: PluginId = value["plugin_id"].as_str().context("missing receipt plugin")?.parse().context("invalid receipt plugin")?;
    let _: PolicyRevision = serde_json::from_value(value["result_revision"].clone()).context("invalid receipt revision")?;
    let _: Sha256Digest = serde_json::from_value(value["epoch"].clone()).context("invalid receipt epoch")?;
    let _: HostPolicyFingerprint = serde_json::from_value(value["host_policy"].clone()).context("invalid receipt fingerprint")?;
    let summary = &value["summary"];
    match value["kind"].as_str() {
        Some("review" | "select") => {
            let package: InstalledPackageRef = serde_json::from_value(value["package"].clone()).context("invalid receipt exact package")?;
            if package.plugin_id() != &plugin {
                bail!("receipt package/plugin mismatch");
            }
            if value["kind"] == "review" {
                if !matches!(
                    summary["intent"].as_str(),
                    Some("deny" | "data_only" | "native_full_service_user_trust" | "wasm_sandbox_required")
                ) || summary.get("expected_review_revision") != Some(&Value::Null)
                {
                    bail!("invalid review receipt summary");
                }
            } else {
                let _: PolicyRevision =
                    serde_json::from_value(summary["expected_review_revision"].clone()).context("invalid selected review revision")?;
                if summary.get("intent") != Some(&Value::Null) {
                    bail!("invalid selection receipt summary");
                }
            }
        }
        Some("clear") => {
            if value.get("package") != Some(&Value::Null)
                || summary.get("intent") != Some(&Value::Null)
                || summary.get("expected_review_revision") != Some(&Value::Null)
            {
                bail!("invalid clear receipt summary");
            }
        }
        _ => bail!("unrecognized policy receipt command"),
    }
    Ok(())
}
fn check_mutation(status: StatusCode, value: &Value, id: &PackageOperationId, kind: &str, request: &Value) -> anyhow::Result<i32> {
    if value["operation_id"].as_str() != Some(id.as_str()) {
        bail!("policy operation identity mismatch");
    }
    check_gate(&value["gate"])?;
    match (status, value["outcome"].as_str()) {
        (StatusCode::OK, Some("applied" | "historical_receipt")) => {
            let historical = value["outcome"] == "historical_receipt";
            let receipt = &value["receipt"];
            check_receipt(receipt, id, historical)?;
            if !historical
                && (value["gate"]["state"] != "open"
                    || receipt["epoch"] != value["gate"]["epoch"]
                    || receipt["host_policy"] != value["gate"]["host_policy"])
            {
                bail!("applied receipt does not match its open committing gate");
            }
            if receipt["kind"].as_str() != Some(kind) || receipt["host_policy"] != request["expected_host_policy"] {
                bail!("policy receipt command or fingerprint mismatch");
            }
            match kind {
                "review" | "select" => {
                    if receipt["package"] != request["package"] {
                        bail!("policy receipt exact package mismatch");
                    }
                }
                "clear" => {
                    if receipt["plugin_id"] != request["plugin_id"] {
                        bail!("policy receipt plugin mismatch");
                    }
                }
                _ => bail!("unknown requested policy command"),
            }
            if kind == "review" && receipt["summary"]["intent"] != request["intent"]["kind"]
                || kind == "select" && receipt["summary"]["expected_review_revision"] != request["expected_review_revision"]
            {
                bail!("policy receipt decision mismatch");
            }
            if historical {
                eprintln!("Historical receipt only; no new approval or selection was applied.");
            }
            Ok(0)
        }
        (StatusCode::CONFLICT, Some("gate_sealed" | "capacity_sealed"))
            if value["gate"]["state"] == "sealed" && value.get("receipt") == Some(&Value::Null) =>
        {
            if value["outcome"] == "capacity_sealed" && value["gate"]["seal"]["operation_id"].as_str() != Some(id.as_str()) {
                bail!("committed seal identity mismatch");
            }
            Ok(1)
        }
        _ => bail!("unexpected policy mutation outcome; inspect gate and retained key"),
    }
}
fn validate_response(status: StatusCode, value: &Value, expected: &Expected) -> anyhow::Result<i32> {
    // Ordinary errors have no success envelope. Keep them structured and nonzero,
    // never follow their Location/url, and never interpret an error as rollback.
    let seal = matches!(value["outcome"].as_str(), Some("gate_sealed" | "capacity_sealed"));
    if !status.is_success() && !seal {
        return Ok(1);
    }
    if value["schema_version"].as_u64() != Some(1) || value["execution"].as_str() != Some("unavailable") {
        bail!("unrecognized passive policy response schema or execution state");
    }
    match expected {
        Expected::Gate => {
            if status != StatusCode::OK {
                bail!("unexpected policy status");
            }
            check_gate(&value["gate"])?;
        }
        Expected::Candidate(id) => {
            if status != StatusCode::OK || value["plugin_id"].as_str() != Some(id.as_str()) {
                bail!("candidate plugin identity mismatch");
            }
            check_gate(&value["gate"])?;
        }
        Expected::Prepared => {
            if status != StatusCode::OK || value["outcome"].as_str() != Some("prepared") || value["gate"]["state"].as_str() != Some("open")
            {
                bail!("unexpected policy prepare result; inspect gate");
            }
            check_gate(&value["gate"])?;
        }
        Expected::Package(package) => {
            if status != StatusCode::OK
                || serde_json::from_value::<InstalledPackageRef>(value["package"].clone()).ok().as_ref() != Some(package)
            {
                bail!("policy proposal identity mismatch");
            }
            check_gate(&value["gate"])?;
        }
        Expected::Receipt(id) => {
            if status != StatusCode::OK || value["operation_id"].as_str() != Some(id.as_str()) {
                bail!("policy operation identity mismatch");
            }
            check_receipt(&value["receipt"], id, true)?;
        }
        Expected::Mutation(id, kind, request) => return check_mutation(status, value, id, kind, request),
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser as _;
    fn request() -> Value {
        json!({"schema_version":1,"package":{"plugin_id":"example.independent","version":"1.0.0+outside.build","archive_digest":"a".repeat(64)},"expected_review":{"kind":"absent"},"expected_host_policy":"b".repeat(64),"intent":{"kind":"native_full_service_user_trust"},"grants":[]})
    }
    fn open_gate() -> Value {
        json!({"state":"open","epoch":"c".repeat(64),"host_policy":"b".repeat(64),"seal":null})
    }
    fn success(id: &str) -> Value {
        json!({"schema_version":1,"execution":"unavailable","operation_id":id,"outcome":"applied","gate":open_gate(),"receipt":{"operation_id":id,"kind":"review","plugin_id":"example.independent","package":request()["package"],"result_revision":"1","epoch":"c".repeat(64),"host_policy":"b".repeat(64),"historical":false,"summary":{"intent":"native_full_service_user_trust","expected_review_revision":null}},"url":"https://untrusted.invalid/never-follow"})
    }
    #[test]
    fn namespace_parser_and_independent_segments_preserve_full_identity() {
        for args in [
            vec!["status"],
            vec!["prepare"],
            vec!["inspect", "example.independent"],
            vec![
                "inspect-package",
                "example.independent",
                "1.0.0+outside.build",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ],
            vec!["operation", "--", "-retry"],
            vec!["review", "--request", "review.json", "--operation-id", "known"],
            vec!["select", "--request", "select.json"],
            vec!["clear", "--request", "clear.json"],
        ] {
            let mut command = vec!["rbit", "plugin", "policy"];
            command.extend(args);
            assert!(crate::cli::Cli::try_parse_from(command).is_ok());
        }
        assert_eq!(segment("1.0.0+outside.build"), "1.0.0%2Boutside.build");
        assert_eq!(segment("a/b?x#y%"), "a%2Fb%3Fx%23y%25");
    }
    #[tokio::test]
    async fn files_are_bounded_and_strict_sdk_requests_remain_explicit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("review.json");
        let mut bytes = serde_json::to_vec(&request()).unwrap();
        bytes.resize(LIMIT, b' ');
        tokio::fs::write(&path, &bytes).await.unwrap();
        let review = request_file::<PackageReviewRequest>(&path).await.unwrap();
        assert_eq!(review.package.version().to_string(), "1.0.0+outside.build");
        bytes.push(b' ');
        tokio::fs::write(&path, bytes).await.unwrap();
        assert!(request_file::<PackageReviewRequest>(&path).await.unwrap_err().to_string().contains("exceeds"));
        let mut invalid = request();
        invalid["enabled"] = json!(true);
        tokio::fs::write(&path, serde_json::to_vec(&invalid).unwrap()).await.unwrap();
        assert!(request_file::<PackageReviewRequest>(&path).await.is_err());
    }
    #[test]
    fn response_protocol_preserves_historical_and_committed_seal_distinctions() {
        let expected = Expected::Mutation(parse_id("known").unwrap(), "review", request());
        let mut value = success("known");
        assert_eq!(validate_response(StatusCode::OK, &value, &expected).unwrap(), 0);
        value["gate"] = json!({"state":"uninitialized","epoch":null,"host_policy":null,"seal":null});
        assert!(validate_response(StatusCode::OK, &value, &expected).is_err());
        value = success("known");
        value["gate"]["epoch"] = json!("d".repeat(64));
        assert!(validate_response(StatusCode::OK, &value, &expected).is_err());
        value = success("known");
        value["gate"]["host_policy"] = json!("d".repeat(64));
        assert!(validate_response(StatusCode::OK, &value, &expected).is_err());
        value["outcome"] = json!("historical_receipt");
        value["receipt"]["historical"] = json!(true);
        // Historical receipt is separate from current gate, which may change.
        assert_eq!(validate_response(StatusCode::OK, &value, &expected).unwrap(), 0);
        value["execution"] = json!("running");
        assert!(validate_response(StatusCode::OK, &value, &expected).is_err());
        value = success("wrong");
        assert!(validate_response(StatusCode::OK, &value, &expected).is_err());
        value = success("known");
        value["outcome"] = json!("approved_and_running");
        assert!(validate_response(StatusCode::OK, &value, &expected).is_err());
        value = json!({"schema_version":1,"execution":"unavailable","operation_id":"known","outcome":"capacity_sealed","receipt":null,"gate":{"state":"sealed","epoch":"c".repeat(64),"host_policy":"b".repeat(64),"seal":{"operation_id":"known","reason":"receipt_capacity"}}});
        assert_eq!(validate_response(StatusCode::CONFLICT, &value, &expected).unwrap(), 1);
        value["gate"]["state"] = json!("open");
        assert!(validate_response(StatusCode::CONFLICT, &value, &expected).is_err());
    }
    #[test]
    fn malformed_gate_candidate_and_receipt_metadata_cannot_report_success() {
        let mut gate = open_gate();
        gate["epoch"] = json!("not-a-digest");
        assert!(check_gate(&gate).is_err());
        gate = open_gate();
        gate["host_policy"] = Value::Null;
        assert!(check_gate(&gate).is_err());
        gate = json!({"state":"uninitialized","epoch":null,"host_policy":null,"seal":null});
        assert!(check_gate(&gate).is_ok());
        gate["epoch"] = json!("c".repeat(64));
        assert!(check_gate(&gate).is_err());
        gate = open_gate();
        gate["state"] = json!("sealed");
        gate["seal"] = json!({"operation_id":"known","reason":"invented"});
        assert!(check_gate(&gate).is_err());
        gate["seal"]["reason"] = json!("receipt_capacity");
        gate["seal"]["operation_id"] = json!("../escape");
        assert!(check_gate(&gate).is_err());
        let candidate = Expected::Candidate("example.independent".parse().unwrap());
        let mut value =
            json!({"schema_version":1,"execution":"unavailable","plugin_id":"example.other","gate":open_gate(),"selection":null});
        assert!(validate_response(StatusCode::OK, &value, &candidate).is_err());
        value["plugin_id"] = json!("example.independent");
        assert_eq!(validate_response(StatusCode::OK, &value, &candidate).unwrap(), 0);
        let expected = Expected::Mutation(parse_id("known").unwrap(), "review", request());
        for (field, bad) in [
            ("epoch", json!("invalid")),
            ("host_policy", json!("invalid")),
            ("plugin_id", json!("example.other")),
            ("result_revision", json!("01")),
        ] {
            value = success("known");
            value["receipt"][field] = bad;
            assert!(validate_response(StatusCode::OK, &value, &expected).is_err());
        }
        value = success("known");
        value["receipt"]["package"]["version"] = json!("1.0.0+different.build");
        assert!(validate_response(StatusCode::OK, &value, &expected).is_err());
        value = success("known");
        value["receipt"]["summary"]["intent"] = json!("deny");
        assert!(validate_response(StatusCode::OK, &value, &expected).is_err());
        value = success("known");
        value["receipt"]["kind"] = json!("clear");
        assert!(validate_response(StatusCode::OK, &value, &expected).is_err());
    }
    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    #[tokio::test]
    async fn real_loopback_write_is_single_fixed_destination_and_response_is_bounded() {
        use axum::{
            body::{Body, to_bytes},
            http::{HeaderMap, Uri},
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
        let mode_handler = Arc::clone(&mode);
        let (sender, mut captured) = tokio::sync::mpsc::channel(8);
        let app = axum::Router::new().fallback(any(move |method: Method, uri: Uri, headers: HeaderMap, body: Body| {
            let sender = sender.clone();
            let mode = Arc::clone(&mode_handler);
            async move {
                let bytes = to_bytes(body, LIMIT).await.unwrap();
                sender.send((method.clone(), uri.path().to_owned(), headers, bytes)).await.unwrap();
                match mode.load(Ordering::SeqCst) {
                    1 => (StatusCode::OK, axum::Json(success("wrong-key"))).into_response(),
                    2 => (StatusCode::TEMPORARY_REDIRECT, [("location", "/poison")], "{}").into_response(),
                    3 => Body::from_stream(tokio_util::io::ReaderStream::new(std::io::Cursor::new(vec![b'x'; LIMIT + 1]))).into_response(),
                    _ => {
                        let mut value = success("known");
                        if method == Method::GET {
                            value["receipt"]["historical"] = json!(true);
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
        let path = directory.path().join("review.json");
        tokio::fs::write(&path, serde_json::to_vec(&request()).unwrap()).await.unwrap();
        assert_eq!(run(&api, PolicyCommand::Review { request: path.clone(), operation_id: Some("known".to_owned()) }).await.unwrap(), 0);
        let (method, path_seen, headers, body) = captured.recv().await.unwrap();
        assert_eq!(method, Method::PUT);
        assert_eq!(path_seen, "/deploy/api/v1/admin/plugin-policy-operations/known");
        assert_eq!(headers["authorization"], "Bearer private-token");
        assert_eq!(serde_json::from_slice::<Value>(&body).unwrap()["request"], request());
        assert_eq!(run(&api, PolicyCommand::Operation { id: "known".to_owned() }).await.unwrap(), 0);
        assert_eq!(captured.recv().await.unwrap().0, Method::GET);
        mode.store(1, Ordering::SeqCst);
        assert!(run(&api, PolicyCommand::Review { request: path.clone(), operation_id: Some("known".to_owned()) }).await.is_err());
        captured.recv().await.unwrap();
        mode.store(2, Ordering::SeqCst);
        assert_eq!(run(&api, PolicyCommand::Review { request: path, operation_id: Some("known".to_owned()) }).await.unwrap(), 1);
        assert_eq!(captured.recv().await.unwrap().1, "/deploy/api/v1/admin/plugin-policy-operations/known");
        assert!(captured.try_recv().is_err(), "no redirect replay or server-URL follow");
        mode.store(3, Ordering::SeqCst);
        let response = api.request(Method::GET, "admin/plugin-policy").unwrap().send().await.unwrap();
        assert_eq!(response.content_length(), None);
        assert!(read_response(response).await.unwrap_err().to_string().contains("exceeds"));
        captured.recv().await.unwrap();
    }
}
