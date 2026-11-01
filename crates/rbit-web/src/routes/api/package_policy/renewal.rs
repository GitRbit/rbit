//! Explicit bounded epoch maintenance; no automatic approval or execution.

use super::{
    ApiResult, ApiRoute, COMMAND_TIMEOUT, Caller, Get, Put, ROOT, administrator, answer, expired, fault, gate_value, located, operation_id,
    read_json_limited, route, url,
};
use crate::AppState;
use axum::{
    body::Body,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Response,
};
use rbit_plugin_api::{MAX_POLICY_RENEWAL_REQUEST_BYTES, PackagePolicyRenewalRequest};
use rbit_services::{PackagePolicyRenewalOutcome, PackagePolicyRenewalReceipt, webhooks::payloads};
use serde_json::{Value, json};
use tokio::time::Instant;

const OPERATIONS: &str = "/admin/plugin-policy-renewals/{operation_id}";

pub(super) fn routes() -> Vec<ApiRoute> {
    vec![route(Put, OPERATIONS, apply), route(Get, OPERATIONS, operation)]
}
async fn apply(
    State(state): State<AppState>,
    caller: Caller,
    Path(raw): Path<String>,
    headers: HeaderMap,
    body: Body,
) -> ApiResult<Response> {
    let actor = administrator(&caller)?;
    let id = operation_id(&raw)?;
    let location = url(&state, &format!("/admin/plugin-policy-renewals/{}", id.as_str()));
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    // Raw body is untouched until both PAT authority and its live database check.
    tokio::time::timeout_at(deadline, state.services.package_policy.gate(actor))
        .await
        .map_err(|_| located(expired(), &location))?
        .map_err(|e| located(fault(&e), &location))?;
    let request: PackagePolicyRenewalRequest =
        read_json_limited(body, &headers, deadline, MAX_POLICY_RENEWAL_REQUEST_BYTES).await.map_err(|e| located(e, &location))?;
    let result = tokio::time::timeout_at(deadline, state.services.package_policy.renew(actor, id, request))
        .await
        .map_err(|_| located(expired(), &location))?
        .map_err(|e| located(fault(&e), &location))?;
    let (status, outcome, receipt, gate) = match result {
        PackagePolicyRenewalOutcome::Applied { receipt, gate } => (StatusCode::OK, "applied", Some(receipt), gate),
        PackagePolicyRenewalOutcome::HistoricalReceipt { receipt, gate } => (StatusCode::OK, "historical_receipt", Some(receipt), gate),
        PackagePolicyRenewalOutcome::RetentionFull { gate } => (StatusCode::CONFLICT, "retention_full", None, gate),
    };
    answer(
        status,
        &json!({
            "schema_version":1,"execution":"unavailable","outcome":outcome,"operation_id":raw,"url":location,
            "receipt":receipt.as_ref().map(|r| receipt_value(r,outcome=="historical_receipt")),
            "gate":gate_value(&gate),"gate_url":url(&state,ROOT),
        }),
        Some(&location),
    )
}
async fn operation(State(state): State<AppState>, caller: Caller, Path(raw): Path<String>) -> ApiResult<Response> {
    let actor = administrator(&caller)?;
    let id = operation_id(&raw)?;
    let location = url(&state, &format!("/admin/plugin-policy-renewals/{}", id.as_str()));
    let receipt =
        state.services.package_policy.renewal_operation(actor, &id).await.map_err(|e| located(fault(&e), &location))?.ok_or_else(|| {
            located(super::ApiError::new(StatusCode::NOT_FOUND, "No renewal receipt; inspect the current gate before retrying"), &location)
        })?;
    // Never combine a second gate read with historical receipt inspection.
    answer(
        StatusCode::OK,
        &json!({
            "schema_version":1,"execution":"unavailable","outcome":"historical_receipt","operation_id":raw,"url":location,
            "receipt":receipt_value(&receipt,true),"gate_url":url(&state,ROOT),
        }),
        None,
    )
}
fn receipt_value(value: &PackagePolicyRenewalReceipt, historical: bool) -> Value {
    json!({"operation_id":value.id.as_str(),"kind":"renew","historical":historical,
        "source_epoch":value.source_epoch,"source_host_policy":value.source_fingerprint,
        "destination_epoch":value.destination_epoch,"destination_host_policy":value.destination_fingerprint,
        "expected_seal_operation":value.request.expected_seal_operation,"actor_id":value.actor.map(|a|a.0.to_string()),
        "created_at":payloads::timestamp(value.created_at),
    })
}

#[cfg(test)]
mod tests;
