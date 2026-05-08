use crate::{
    config::ModelConfig,
    schemas::{BeliefSummary, DerivedFeatures, PredictionSummary, Recommendation, RiskLevel},
};
use std::collections::BTreeMap;

pub fn choose_action(
    risk_level: &RiskLevel,
    features: &DerivedFeatures,
    belief: &BeliefSummary,
    prediction: &PredictionSummary,
    counterfactual_costs: &BTreeMap<String, f64>,
    config: &ModelConfig,
) -> Recommendation {
    let confidence = if *risk_level == RiskLevel::Unknown {
        0.0
    } else {
        features.confidence.min(belief.confidence)
    };
    let confidence_min = policy_value(config, "confidence_min", 0.05);
    let rho_low = policy_value(config, "rho_low", 0.20);
    let rho_high = policy_value(config, "rho_high", 0.55);
    let rho_critical = policy_value(config, "rho_critical", 0.85);
    let lowest_cost_action = lowest_cost_action(counterfactual_costs);

    let (recommended_action, decision_reason) = if *risk_level == RiskLevel::Unknown {
        (
            "ask_for_missing_data",
            "risk is UNKNOWN because required inputs or source-backed specs are missing or invalid",
        )
    } else if confidence < confidence_min {
        (
            "ask_for_more_measurements",
            "combined feature and belief confidence is below the configured minimum",
        )
    } else if prediction.failure_probability_7d >= rho_critical {
        (
            "replace",
            "7-day failure probability meets or exceeds the critical threshold",
        )
    } else if prediction.failure_probability_7d >= rho_high {
        (
            "repair",
            "7-day failure probability meets or exceeds the high threshold",
        )
    } else if prediction.failure_probability_7d >= rho_low || *risk_level == RiskLevel::High {
        (
            "recommend_cavitation_checklist",
            "failure probability or HIGH risk level warrants the cavitation checklist",
        )
    } else if *risk_level == RiskLevel::Medium {
        (
            "explain_npsh_lesson_and_monitor",
            "MEDIUM risk warrants monitoring and an NPSH explanation",
        )
    } else {
        (
            "explain_normal_operation",
            "risk is LOW and failure probability is below configured action thresholds",
        )
    };

    Recommendation {
        recommended_action: recommended_action.to_string(),
        action_confidence: round6(confidence),
        decision_reason: decision_reason.to_string(),
        lowest_cost_action,
    }
}

fn lowest_cost_action(counterfactual_costs: &BTreeMap<String, f64>) -> String {
    counterfactual_costs
        .iter()
        .min_by(|left, right| left.1.total_cmp(right.1))
        .map(|(action, _)| action.clone())
        .unwrap_or_else(|| "none".to_string())
}

fn policy_value(config: &ModelConfig, key: &str, default: f64) -> f64 {
    config
        .policy
        .get(key)
        .and_then(|value| value.as_f64())
        .unwrap_or(default)
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}
