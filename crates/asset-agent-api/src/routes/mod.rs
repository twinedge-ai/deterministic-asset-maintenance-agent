use axum::{
    routing::{get, post},
    Router,
};
use std::sync::Arc;

use crate::AppState;

pub mod analysis;
pub mod cases;
pub mod feedback;
pub mod health;
pub mod learning;
pub mod llm;
pub mod opc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/", get(health::root))
        .route("/api/health", get(health::health))
        .route("/api/analyze", post(analysis::analyze))
        .route("/api/cases", get(cases::list_cases))
        .route("/api/cases/:case_id", get(cases::get_case))
        .route(
            "/api/cases/:case_id/feedback",
            post(feedback::submit_feedback),
        )
        .route("/api/learning/summary", get(learning::summary))
        .route("/api/checklist", get(learning::checklist))
        .route("/api/thresholds", get(learning::thresholds))
        .route("/api/opc/status", get(opc::status))
        .route("/api/opc/snapshot", get(opc::snapshot))
        .route("/api/opc/output/status", get(opc::output_status))
        .route("/api/llm/status", get(llm::status))
        .route("/api/llm/explain", post(llm::explain_route))
}
