use asset_agent_core::{
    features::analyze_cavitation, load_model_config, schemas::ObservationSnapshot,
};
use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;

use crate::AppState;

pub async fn analyze(
    State(state): State<Arc<AppState>>,
    Json(snapshot): Json<ObservationSnapshot>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if snapshot.asset_id != state.asset_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("unsupported asset_id {}; this MVP is scoped to {}", snapshot.asset_id, state.asset_id)
            })),
        ));
    }
    let config = load_model_config(&state.project_root, &state.asset_id).map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": error.to_string()
            })),
        )
    })?;
    let result = analyze_cavitation(&snapshot, &config);
    let store = state.open_memory_store().map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "ok": false,
                "error": error.to_string()
            })),
        )
    })?;
    let case_id = store
        .store_analysis_case(&snapshot, &result)
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": error.to_string()
                })),
            )
        })?;
    let agent_output = state
        .output_state
        .publish_analysis(Some(&case_id), &result, &config.versions)
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "ok": false,
                    "error": error.to_string()
                })),
            )
        })?;
    Ok(Json(serde_json::json!({
        "ok": result.features.validation_errors.is_empty() && result.features.missing_inputs.is_empty(),
        "case_id": case_id,
        "result": result,
        "agent_output": agent_output
    })))
}
