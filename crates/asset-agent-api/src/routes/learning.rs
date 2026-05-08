use asset_agent_core::load_model_config;
use asset_agent_memory::LearningSettings;
use axum::{extract::State, http::StatusCode, Json};
use std::sync::Arc;

use crate::AppState;

pub async fn summary(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let settings = settings(&state)?;
    let store = state.open_memory_store().map_err(internal_error)?;
    let summary = store.learning_summary(&settings).map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "summary": summary
    })))
}

pub async fn checklist(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let settings = settings(&state)?;
    let store = state.open_memory_store().map_err(internal_error)?;
    let items = store.checklist_ranking(&settings).map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "items": items
    })))
}

pub async fn thresholds(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let store = state.open_memory_store().map_err(internal_error)?;
    let thresholds = store.thresholds().map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "thresholds": thresholds
    })))
}

fn settings(state: &AppState) -> Result<LearningSettings, (StatusCode, Json<serde_json::Value>)> {
    let config = load_model_config(&state.project_root, &state.asset_id).map_err(internal_error)?;
    Ok(LearningSettings::from_model_config(&config))
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
