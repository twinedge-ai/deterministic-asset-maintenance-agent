use asset_agent_core::{
    config::validate_snapshot_design_specs,
    load_model_config,
    replay::{read_trace, select_as_of},
};
use asset_agent_opcua::input_client::{build_input_contract, OpcUaInputConfig};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;
use std::{path::PathBuf, sync::Arc};

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct SnapshotQuery {
    pub asset_id: Option<String>,
    pub as_of: Option<String>,
}

pub async fn status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let trace_path = resolve_project_path(&state.project_root, &state.replay_trace);
    let snapshots = read_trace(&trace_path).map_err(internal_error)?;
    let latest = snapshots.last();
    let contract = build_input_contract(
        &OpcUaInputConfig {
            endpoint: state.opcua_endpoint.clone(),
            namespace_uri: state.opcua_namespace_uri.clone(),
        },
        &state.asset_id,
    );

    Ok(Json(serde_json::json!({
        "connected": false,
        "mode": "replay",
        "endpoint": &state.opcua_endpoint,
        "namespace_uri": &state.opcua_namespace_uri,
        "namespace_resolution": contract.namespace,
        "asset_id": &state.asset_id,
        "latest_as_of_timestamp": latest.map(|snapshot| snapshot.as_of_timestamp.clone()),
        "latest_temperature_source": latest.map(|snapshot| snapshot.liquid_temperature_source.clone()),
        "latest_temperature_c": latest.map(|snapshot| snapshot.liquid_temperature_c),
        "operational_node_count": contract.operational_nodes.len(),
        "design_spec_node_count": contract.design_spec_nodes.len(),
        "operational_nodes": contract.operational_nodes,
        "design_spec_nodes": contract.design_spec_nodes,
        "temperature_node": contract.temperature_node,
        "command_nodes_blocked": contract.command_nodes_blocked,
        "freshness": {
            "mode": "replay",
            "source_timestamp": latest.map(|snapshot| snapshot.timestamp.clone()),
            "as_of_timestamp": latest.map(|snapshot| snapshot.as_of_timestamp.clone()),
            "status": "replay_current"
        },
        "replay_trace": trace_path,
        "note": "Status reports the replay input contract; direct live OPC UA reads are available through the live command."
    })))
}

pub async fn snapshot(
    State(state): State<Arc<AppState>>,
    Query(query): Query<SnapshotQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let requested_asset = query.asset_id.unwrap_or_else(|| state.asset_id.clone());
    if requested_asset != state.asset_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("unsupported asset_id {requested_asset}; this MVP is scoped to {}", state.asset_id)
            })),
        ));
    }

    let config = load_model_config(&state.project_root, &state.asset_id).map_err(internal_error)?;
    let trace_path = resolve_project_path(&state.project_root, &state.replay_trace);
    let snapshots = read_trace(&trace_path).map_err(internal_error)?;
    let selector = query.as_of.unwrap_or_else(|| "trace:last".to_string());
    let snapshot = select_as_of(&snapshots, &selector).map_err(internal_error)?;
    let validation_errors = validate_snapshot_design_specs(snapshot, &config);

    Ok(Json(serde_json::json!({
        "ok": validation_errors.is_empty(),
        "source": "replay",
        "selector": selector,
        "trace": trace_path,
        "snapshot": snapshot,
        "design_spec_validation_errors": validation_errors,
        "temperature": {
            "liquid_temperature_c": snapshot.liquid_temperature_c,
            "source": snapshot.liquid_temperature_source
        }
    })))
}

pub async fn output_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let snapshot = state.output_state.snapshot().map_err(internal_error)?;

    Ok(Json(serde_json::json!({
        "ok": true,
        "connected": true,
        "mode": "opcua-output",
        "endpoint": state.output_runtime.endpoint,
        "namespace_uri": state.output_runtime.namespace_uri,
        "asset_id": state.output_state.asset_id(),
        "initialized": snapshot.initialized,
        "sequence": snapshot.sequence,
        "updated_at": snapshot.updated_at,
        "source_case_id": snapshot.source_case_id,
        "node_count": snapshot.node_count,
        "nodes": snapshot.nodes
    })))
}

fn resolve_project_path(project_root: &std::path::Path, path: &PathBuf) -> PathBuf {
    if path.is_absolute() {
        path.clone()
    } else {
        project_root.join(path)
    }
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "ok": false,
            "error": error.to_string()
        })),
    )
}
