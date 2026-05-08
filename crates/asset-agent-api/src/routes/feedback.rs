use asset_agent_core::load_model_config;
use asset_agent_memory::{FeedbackInput, LearningSettings};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::AppState;

pub async fn submit_feedback(
    State(state): State<Arc<AppState>>,
    Path(case_id): Path<String>,
    Json(feedback): Json<FeedbackInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let config = load_model_config(&state.project_root, &state.asset_id).map_err(internal_error)?;
    let settings = LearningSettings::from_model_config(&config);
    let store = state.open_memory_store().map_err(internal_error)?;
    let result = store
        .submit_feedback(&case_id, feedback, &settings)
        .map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "result": result
    })))
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
