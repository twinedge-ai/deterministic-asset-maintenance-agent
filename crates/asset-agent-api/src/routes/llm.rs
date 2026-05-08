use asset_agent_core::{
    features::analyze_cavitation, load_model_config, schemas::ObservationSnapshot,
};
use asset_agent_llm::{
    config::MistralConfig, explanation_service::explain, mistral_runner::runner_status,
};
use asset_agent_memory::LlmRunInput;
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, time::Instant};

use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ExplainRequest {
    Snapshot(ObservationSnapshot),
    Wrapped {
        snapshot: Option<ObservationSnapshot>,
        case_id: Option<String>,
    },
}

pub async fn status(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let config = MistralConfig::from_env(&state.project_root);
    let status = runner_status(&config);
    Json(json!({
        "llm_available": status.available,
        "source": if status.available { "mistralrs" } else { "template" },
        "fallback_used": !status.available,
        "status": status,
        "message": "mistral.rs is optional, explanation-only, and never safety-critical"
    }))
}

pub async fn explain_route(
    State(state): State<Arc<AppState>>,
    Json(request): Json<ExplainRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let started = Instant::now();
    let store = state.open_memory_store().map_err(internal_error)?;
    let (snapshot, existing_case_id) = match request {
        ExplainRequest::Snapshot(snapshot) => (snapshot, None),
        ExplainRequest::Wrapped { snapshot, case_id } => {
            if let Some(snapshot) = snapshot {
                (snapshot, case_id)
            } else if let Some(case_id) = case_id {
                let Some(case_record) = store.get_case(&case_id).map_err(internal_error)? else {
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(json!({
                            "ok": false,
                            "error": format!("case not found: {case_id}")
                        })),
                    ));
                };
                let snapshot =
                    serde_json::from_value(case_record.observations).map_err(internal_error)?;
                (snapshot, Some(case_id))
            } else {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(json!({
                        "ok": false,
                        "error": "request must include snapshot or case_id"
                    })),
                ));
            }
        }
    };

    if snapshot.asset_id != state.asset_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "ok": false,
                "error": format!("unsupported asset_id {}; this MVP is scoped to {}", snapshot.asset_id, state.asset_id)
            })),
        ));
    }

    let model_config =
        load_model_config(&state.project_root, &state.asset_id).map_err(internal_error)?;
    let analysis = analyze_cavitation(&snapshot, &model_config);
    let case_id = if let Some(case_id) = existing_case_id {
        case_id
    } else {
        store
            .store_analysis_case(&snapshot, &analysis)
            .map_err(internal_error)?
    };
    let llm_config = MistralConfig::from_env(&state.project_root);
    let trace = explain(&analysis, &llm_config);
    let llm_run_id = store
        .store_llm_run(
            Some(&case_id),
            &trace.prompt_hash,
            &LlmRunInput {
                provider: trace.runner.provider.clone(),
                model_path: trace.runner.model_path.clone(),
                binary_path: trace.runner.binary_path.clone(),
                prompt_json: json!({
                    "prompt_hash": &trace.prompt_hash,
                    "source": &trace.source,
                    "guard_reasons": &trace.guard_reasons,
                    "seed": trace.runner.seed,
                    "max_tokens": trace.runner.max_tokens,
                    "temperature": trace.runner.temperature
                }),
                prompt_text: trace.prompt_text.clone(),
                raw_output: trace.raw_output.clone(),
                guarded_output: trace.guarded_output.clone(),
                guard_passed: trace.guard_passed,
                fallback_used: trace.fallback_used,
                latency_ms: started.elapsed().as_secs_f64() * 1000.0,
            },
        )
        .map_err(internal_error)?;

    Ok(Json(json!({
        "ok": true,
        "case_id": case_id,
        "llm_run_id": llm_run_id,
        "risk_level": analysis.risk_level,
        "recommended_action": analysis.recommended_action,
        "llm": trace
    })))
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({
            "ok": false,
            "error": error.to_string()
        })),
    )
}
