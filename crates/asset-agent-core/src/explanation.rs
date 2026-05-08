use crate::schemas::{DerivedFeatures, Explanation, PredictionSummary, Recommendation, RiskLevel};

pub fn template_explanation(
    asset_id: &str,
    risk_level: &RiskLevel,
    features: &DerivedFeatures,
    prediction: &PredictionSummary,
    recommendation: &Recommendation,
    limitations: &[String],
) -> Explanation {
    Explanation {
        source: "template".to_string(),
        text: explanation_text(
            asset_id,
            risk_level,
            features,
            prediction,
            recommendation,
            limitations,
        ),
        guard_passed: true,
        fallback_used: true,
        prompt_hash: String::new(),
        guard_reasons: Vec::new(),
    }
}

fn explanation_text(
    asset_id: &str,
    risk_level: &RiskLevel,
    features: &DerivedFeatures,
    prediction: &PredictionSummary,
    recommendation: &Recommendation,
    limitations: &[String],
) -> String {
    let limitations_text = if limitations.is_empty() {
        "No configured limitations were reported.".to_string()
    } else {
        format!("Limitations: {}", limitations.join(" "))
    };

    if *risk_level == RiskLevel::Unknown {
        return format!(
            "{asset_id} risk is UNKNOWN because required inputs or source-backed design specs are missing or invalid. Recommended action: {action}. Lowest configured counterfactual action: {lowest_cost_action}. This is a maintenance recommendation for human review only; the agent does not write plant commands. {limitations_text}",
            action = recommendation.recommended_action,
            lowest_cost_action = recommendation.lowest_cost_action,
        );
    }

    format!(
        "{asset_id} has {risk_label} suspected low-NPSH cavitation risk. The deterministic engine estimated NPSH margin {npsh_margin_m}, cavitation evidence score {evidence_score:.6}, 7-day failure probability {pf7d:.6}, and RUL {rul_hours:.6} h. Recommended action: {action} because {reason}. Lowest configured counterfactual action: {lowest_cost_action}; include it in review even when the safety policy is more conservative. This is not a guaranteed diagnosis and does not authorize autonomous plant commands. Human inspection should verify suction conditions, liquid temperature, pump curve assumptions, and sensor health. {limitations_text}",
        risk_label = risk_label(risk_level),
        npsh_margin_m = optional_meters(features.npsh_margin_m),
        evidence_score = features.cavitation_evidence_score,
        pf7d = prediction.failure_probability_7d,
        rul_hours = prediction.rul_hours,
        action = recommendation.recommended_action,
        reason = recommendation.decision_reason,
        lowest_cost_action = recommendation.lowest_cost_action,
    )
}

fn risk_label(risk_level: &RiskLevel) -> &'static str {
    match risk_level {
        RiskLevel::Low => "LOW",
        RiskLevel::Medium => "MEDIUM",
        RiskLevel::High => "HIGH",
        RiskLevel::Unknown => "UNKNOWN",
    }
}

fn optional_meters(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.6} m"))
        .unwrap_or_else(|| "unavailable".to_string())
}
