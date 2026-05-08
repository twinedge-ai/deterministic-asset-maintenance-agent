use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;

use crate::AppState;

pub async fn list_cases(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let store = state.open_memory_store().map_err(internal_error)?;
    let cases = store.list_cases(50).map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "cases": cases
    })))
}

pub async fn get_case(
    State(state): State<Arc<AppState>>,
    Path(case_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let store = state.open_memory_store().map_err(internal_error)?;
    let Some(case_record) = store.get_case(&case_id).map_err(internal_error)? else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "ok": false,
                "error": format!("case not found: {case_id}")
            })),
        ));
    };
    let similar_cases = store.similar_cases(&case_id, 5).map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "case": case_record,
        "similar_cases": similar_cases
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
