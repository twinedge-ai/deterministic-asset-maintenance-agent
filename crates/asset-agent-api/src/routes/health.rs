use asset_agent_core::schemas::HealthSummary;
use asset_agent_core::{load_model_config, validate_source_bundle};
use axum::{extract::State, http::StatusCode, Json};
use std::{env, sync::Arc};

use crate::AppState;

pub async fn root(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "service": "deterministic-asset-maintenance-agent-api",
        "runtime": "rust",
        "asset_id": &state.asset_id,
        "dashboard": env::var("ASSET_AGENT_DASHBOARD_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_string()),
        "endpoints": {
            "health": "/api/health",
            "analyze": "/api/analyze",
            "cases": "/api/cases",
            "learning": "/api/learning/summary",
            "opc_input_status": "/api/opc/status",
            "opc_output_status": "/api/opc/output/status",
            "llm_status": "/api/llm/status"
        },
        "note": "Open the dashboard for the operator UI; this service is the Rust API."
    }))
}

pub async fn health(
    State(state): State<Arc<AppState>>,
) -> Result<Json<HealthSummary>, (StatusCode, Json<serde_json::Value>)> {
    let config = load_model_config(&state.project_root, &state.asset_id).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": error.to_string()
            })),
        )
    })?;

    Ok(Json(HealthSummary {
        ok: true,
        asset_id: config.asset.asset_id.clone(),
        source_bundle_issues: validate_source_bundle(&config),
        runtime: "rust".to_string(),
    }))
}
