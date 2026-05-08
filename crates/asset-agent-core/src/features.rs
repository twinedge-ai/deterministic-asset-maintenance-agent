use crate::{
    belief::{default_belief, update_belief},
    checklist::recommended_checklist,
    config::{validate_snapshot_design_specs, ModelConfig},
    counterfactual::{default_counterfactual_costs, evaluate_counterfactuals},
    explanation::template_explanation,
    forecast::{default_prediction, forecast, prediction_summary},
    npsh::{gauge_bar_to_abs_kpa, pump_head_m, water_vapor_pressure_kpa},
    policy::choose_action,
    schemas::{AnalysisResult, DerivedFeatures, ObservationSnapshot, RiskLevel},
};

const EQUATIONS_USED: &[&str] = &[
    "NPSH calculation",
    "Belief state update",
    "RUL threshold forecast",
    "Counterfactual cost model",
    "Safety policy",
];

pub fn analyze_cavitation(snapshot: &ObservationSnapshot, config: &ModelConfig) -> AnalysisResult {
    let features = build_features(snapshot, config);
    let risk_level = risk_level(&features, config);
    let (belief, prediction, counterfactual_costs) = if risk_level == RiskLevel::Unknown {
        (
            default_belief(),
            default_prediction(),
            default_counterfactual_costs(),
        )
    } else {
        let belief_state = update_belief(&features, config);
        let counterfactual = evaluate_counterfactuals(&belief_state, config);
        let prediction = prediction_summary(forecast(&belief_state, config), counterfactual.cvar95);
        (belief_state.summary, prediction, counterfactual.costs)
    };
    let recommendation = choose_action(
        &risk_level,
        &features,
        &belief,
        &prediction,
        &counterfactual_costs,
        config,
    );
    let recommended_action = recommendation.recommended_action.clone();
    let recommended_checklist = recommended_checklist(checklist_limit(&recommended_action));
    let limitations = features.limitations.clone();
    let explanation = template_explanation(
        &snapshot.asset_id,
        &risk_level,
        &features,
        &prediction,
        &recommendation,
        &limitations,
    );
    AnalysisResult {
        asset_id: snapshot.asset_id.clone(),
        risk_level,
        features,
        belief,
        prediction,
        counterfactual_costs,
        recommendation,
        recommended_action,
        recommended_checklist,
        limitations,
        explanation,
        equations_used: EQUATIONS_USED.iter().map(|item| item.to_string()).collect(),
    }
}

pub fn build_features(snapshot: &ObservationSnapshot, config: &ModelConfig) -> DerivedFeatures {
    let validation_errors = validate_snapshot_design_specs(snapshot, config);
    let missing_inputs = missing_inputs(snapshot);
    let mut limitations = temperature_limitations(snapshot, config);
    let npshr_method = config
        .npsh
        .get("npshr_method")
        .and_then(|value| value.as_str())
        .unwrap_or("source_design_point")
        .to_string();

    if !validation_errors.is_empty() || !missing_inputs.is_empty() {
        limitations.push(
            "Risk is UNKNOWN because required inputs or source-backed design specs are missing or invalid."
                .to_string(),
        );
        return empty_features(missing_inputs, validation_errors, npshr_method, limitations);
    }

    let atmospheric_pressure_bar = config_value(&config.npsh, "atmospheric_pressure_bar", 1.01325);
    let specific_gravity = snapshot.design_specs.specific_gravity;
    let suction_abs_kpa =
        gauge_bar_to_abs_kpa(snapshot.suction_pressure_bar, atmospheric_pressure_bar);
    let discharge_abs_kpa =
        gauge_bar_to_abs_kpa(snapshot.discharge_pressure_bar, atmospheric_pressure_bar);
    let vapor_pressure_kpa = water_vapor_pressure_kpa(snapshot.liquid_temperature_c);
    let npsha_m =
        ((suction_abs_kpa - vapor_pressure_kpa) * 1000.0) / (1000.0 * specific_gravity * 9.80665);
    let npshr_m = snapshot.design_specs.npshr_m;
    let npsh_margin_m = npsha_m - npshr_m;
    let npsh_margin_ratio = if npshr_m.abs() > f64::EPSILON {
        npsh_margin_m / npshr_m
    } else {
        0.0
    };
    let pump_head_m = pump_head_m(
        snapshot.suction_pressure_bar,
        snapshot.discharge_pressure_bar,
        specific_gravity,
        atmospheric_pressure_bar,
    );
    let pressure_delta_kpa = discharge_abs_kpa - suction_abs_kpa;

    let npsh_score = clamp01(
        (policy_value(config, "npsh_warning_margin_m", 3.0) - npsh_margin_m)
            / policy_value(config, "npsh_warning_margin_m", 3.0),
    );
    let suction_score = clamp01(
        (policy_value(config, "suction_low_bar", 1.0) - snapshot.suction_pressure_bar)
            / policy_value(config, "suction_low_bar", 1.0),
    );
    let vibration_score = score_between(
        snapshot.vibration_rms_mms,
        policy_value(config, "vibration_normal_mms", 4.0),
        policy_value(config, "vibration_high_mms", 8.5),
    );
    let current_score = score_between(
        snapshot.motor_current_a,
        policy_value(config, "current_normal_a", 700.0),
        policy_value(config, "current_high_a", 850.0),
    );
    let thermal_score = score_between(
        snapshot.bearing_temperature_c,
        policy_value(config, "bearing_temp_normal_c", 70.0),
        policy_value(config, "bearing_temp_high_c", 85.0),
    );
    let flow_instability_score = clamp01(
        snapshot.window_features.flow_rate_std_m3h
            / policy_value(config, "flow_instability_warning_m3h", 35.0),
    );
    let current_instability_score = clamp01(
        snapshot.window_features.motor_current_std_a
            / policy_value(config, "current_instability_warning_a", 20.0),
    );
    let cavitation_evidence_score = 0.45 * npsh_score
        + 0.20 * suction_score
        + 0.20 * vibration_score
        + 0.07 * current_score
        + 0.03 * flow_instability_score.max(current_instability_score)
        + 0.05 * thermal_score;

    maybe_add_curve_limitation(snapshot, config, &mut limitations);

    DerivedFeatures {
        missing_inputs,
        validation_errors,
        suction_pressure_abs_kpa: Some(round6(suction_abs_kpa)),
        discharge_pressure_abs_kpa: Some(round6(discharge_abs_kpa)),
        vapor_pressure_kpa: Some(round6(vapor_pressure_kpa)),
        npsha_m: Some(round6(npsha_m)),
        npshr_m: Some(round6(npshr_m)),
        npsh_margin_m: Some(round6(npsh_margin_m)),
        npsh_margin_ratio: Some(round6(npsh_margin_ratio)),
        npshr_method,
        pump_head_m: Some(round6(pump_head_m)),
        pressure_delta_kpa: Some(round6(pressure_delta_kpa)),
        npsh_score: round6(npsh_score),
        suction_score: round6(suction_score),
        vibration_score: round6(vibration_score),
        current_score: round6(current_score),
        thermal_score: round6(thermal_score),
        flow_instability_score: round6(flow_instability_score),
        current_instability_score: round6(current_instability_score),
        cavitation_evidence_score: round6(cavitation_evidence_score),
        confidence: round6(clamp01(0.55 + 0.40 * cavitation_evidence_score)),
        limitations,
    }
}

pub fn risk_level(features: &DerivedFeatures, config: &ModelConfig) -> RiskLevel {
    if !features.validation_errors.is_empty() || !features.missing_inputs.is_empty() {
        return RiskLevel::Unknown;
    }
    let score = features.cavitation_evidence_score;
    let margin = features.npsh_margin_m.unwrap_or(f64::INFINITY);
    if score >= policy_value(config, "high_score", 0.70)
        || margin < policy_value(config, "high_npsh_margin_m", 0.20)
    {
        RiskLevel::High
    } else if score >= policy_value(config, "medium_score", 0.40)
        || margin < policy_value(config, "medium_npsh_margin_m", 0.80)
    {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

fn empty_features(
    missing_inputs: Vec<String>,
    validation_errors: Vec<String>,
    npshr_method: String,
    limitations: Vec<String>,
) -> DerivedFeatures {
    DerivedFeatures {
        missing_inputs,
        validation_errors,
        suction_pressure_abs_kpa: None,
        discharge_pressure_abs_kpa: None,
        vapor_pressure_kpa: None,
        npsha_m: None,
        npshr_m: None,
        npsh_margin_m: None,
        npsh_margin_ratio: None,
        npshr_method,
        pump_head_m: None,
        pressure_delta_kpa: None,
        npsh_score: 0.0,
        suction_score: 0.0,
        vibration_score: 0.0,
        current_score: 0.0,
        thermal_score: 0.0,
        flow_instability_score: 0.0,
        current_instability_score: 0.0,
        cavitation_evidence_score: 0.0,
        confidence: 0.0,
        limitations,
    }
}

fn missing_inputs(snapshot: &ObservationSnapshot) -> Vec<String> {
    let checks = [
        ("flow_rate_m3h", snapshot.flow_rate_m3h),
        ("suction_pressure_bar", snapshot.suction_pressure_bar),
        ("discharge_pressure_bar", snapshot.discharge_pressure_bar),
        ("motor_current_a", snapshot.motor_current_a),
        ("vibration_rms_mms", snapshot.vibration_rms_mms),
        ("bearing_temperature_c", snapshot.bearing_temperature_c),
        ("liquid_temperature_c", snapshot.liquid_temperature_c),
        ("speed_rpm", snapshot.speed_rpm),
    ];
    checks
        .into_iter()
        .filter(|(_, value)| !value.is_finite())
        .map(|(name, _)| name.to_string())
        .collect()
}

fn temperature_limitations(snapshot: &ObservationSnapshot, config: &ModelConfig) -> Vec<String> {
    let mut limitations = Vec::new();
    let plausible = config.asset.temperature_plausible_range_c.as_slice();
    if plausible.len() == 2
        && (snapshot.liquid_temperature_c < plausible[0]
            || snapshot.liquid_temperature_c > plausible[1])
    {
        limitations.push(
            "Liquid temperature was outside the configured plausible range; validate pump inlet temperature."
                .to_string(),
        );
    } else if snapshot.liquid_temperature_source != "live_intake_temperature" {
        limitations.push(
            "Liquid temperature uses the documented high-end fallback rather than a live pump-inlet measurement."
                .to_string(),
        );
    } else {
        limitations.push(
            "Liquid temperature uses live intake temperature as a proxy, not a certified HP pump inlet measurement."
                .to_string(),
        );
    }
    limitations
}

fn maybe_add_curve_limitation(
    snapshot: &ObservationSnapshot,
    config: &ModelConfig,
    limitations: &mut Vec<String>,
) {
    let rated = snapshot.design_specs.rated_flow_m3h;
    if rated.abs() <= f64::EPSILON {
        return;
    }
    let material_fraction = config_value(&config.npsh, "material_flow_deviation_fraction", 0.12);
    if ((snapshot.flow_rate_m3h - rated).abs() / rated) > material_fraction {
        limitations.push(
            "NPSHr uses the source design point because no certified full pump curve is available for interpolation."
                .to_string(),
        );
    }
}

fn score_between(value: f64, normal: f64, high: f64) -> f64 {
    clamp01((value - normal) / (high - normal))
}

fn policy_value(config: &ModelConfig, key: &str, default: f64) -> f64 {
    config_value(&config.policy, key, default)
}

fn config_value(value: &serde_json::Value, key: &str, default: f64) -> f64 {
    value
        .get(key)
        .and_then(|value| value.as_f64())
        .unwrap_or(default)
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn checklist_limit(recommended_action: &str) -> usize {
    match recommended_action {
        "explain_normal_operation" | "ask_for_missing_data" | "ask_for_more_measurements" => 0,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::analyze_cavitation;
    use crate::{config::load_model_config, replay::read_trace, schemas::RiskLevel};
    use std::{fs, path::Path};

    fn analyze_trace(name: &str) -> crate::schemas::AnalysisResult {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("config loads");
        let snapshots = read_trace(root.join("traces").join(name)).expect("trace loads");
        analyze_cavitation(&snapshots[0], &config)
    }

    #[test]
    fn normal_trace_is_low_risk() {
        let result = analyze_trace("hp_pump_1_normal.jsonl");
        assert_eq!(result.risk_level, RiskLevel::Low);
        assert!(result.features.npsh_margin_m.unwrap() > 0.0);
    }

    #[test]
    fn low_suction_trace_is_high_risk() {
        let result = analyze_trace("hp_pump_1_low_suction_pressure.jsonl");
        assert_eq!(result.risk_level, RiskLevel::High);
        assert!(result.features.npsh_margin_m.unwrap() < 0.8);
        assert!(result.features.cavitation_evidence_score >= 0.70);
        assert!(result.belief.mean_damage > 0.0);
        assert!(result.prediction.rul_hours > 0.0);
        assert!(result.counterfactual_costs.contains_key("defer"));
        assert_eq!(result.recommended_action, "replace");
        assert_eq!(result.recommendation.lowest_cost_action, "defer");
        assert_eq!(result.recommended_checklist.len(), 3);
    }

    #[test]
    fn low_suction_phase4_summary_matches_golden_output() {
        let result = analyze_trace("hp_pump_1_low_suction_pressure.jsonl");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let expected_path = root.join("tests/golden/phase4_low_suction_summary.json");
        let expected: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(expected_path).expect("golden output reads"))
                .expect("golden output parses");
        let actual = serde_json::json!({
            "action_confidence": result.recommendation.action_confidence,
            "explanation_fallback_used": result.explanation.fallback_used,
            "explanation_guard_passed": result.explanation.guard_passed,
            "explanation_source": &result.explanation.source,
            "explanation_text": &result.explanation.text,
            "failure_probability_7d": result.prediction.failure_probability_7d,
            "lowest_cost_action": &result.recommendation.lowest_cost_action,
            "npsh_margin_m": result.features.npsh_margin_m,
            "recommended_action": &result.recommended_action,
            "recommended_checklist": &result.recommended_checklist,
            "risk_level": serde_json::to_value(&result.risk_level).expect("risk level serializes"),
            "rul_hours": result.prediction.rul_hours
        });

        assert_eq!(actual, expected);
        assert!(result
            .explanation
            .text
            .contains("suspected low-NPSH cavitation risk"));
        assert!(!result.explanation.text.contains("guaranteed cavitation"));
    }

    #[test]
    fn phase9_replay_profiles_match_golden_outputs() {
        let traces = [
            "hp_pump_1_normal.jsonl",
            "hp_pump_1_low_suction_pressure.jsonl",
            "hp_pump_1_high_vibration.jsonl",
            "hp_pump_1_overload_current.jsonl",
        ];
        let actual: Vec<serde_json::Value> = traces
            .iter()
            .map(|trace| {
                let result = analyze_trace(trace);
                serde_json::json!({
                    "action_confidence": result.recommendation.action_confidence,
                    "cavitation_evidence_score": result.features.cavitation_evidence_score,
                    "checklist_count": result.recommended_checklist.len(),
                    "failure_probability_7d": result.prediction.failure_probability_7d,
                    "lowest_cost_action": &result.recommendation.lowest_cost_action,
                    "npsh_margin_m": result.features.npsh_margin_m,
                    "recommended_action": &result.recommended_action,
                    "risk_level": serde_json::to_value(&result.risk_level)
                        .expect("risk level serializes"),
                    "rul_hours": result.prediction.rul_hours,
                    "trace": trace
                })
            })
            .collect();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let expected_path = root.join("tests/golden/phase9_replay_profiles_summary.json");
        let expected: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(expected_path).expect("golden output reads"))
                .expect("golden output parses");

        assert_eq!(serde_json::Value::Array(actual), expected);
    }

    #[test]
    fn high_vibration_trace_is_not_unknown() {
        let result = analyze_trace("hp_pump_1_high_vibration.jsonl");
        assert_ne!(result.risk_level, RiskLevel::Unknown);
        assert!(result.features.vibration_score > 0.9);
    }

    #[test]
    fn overload_trace_is_not_unknown() {
        let result = analyze_trace("hp_pump_1_overload_current.jsonl");
        assert_ne!(result.risk_level, RiskLevel::Unknown);
        assert!(result.features.current_score > 0.9);
    }

    #[test]
    fn bad_design_spec_fails_closed_unknown() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("config loads");
        let mut snapshots = read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl"))
            .expect("trace loads");
        snapshots[0].design_specs.npshr_m = 12.0;
        let result = analyze_cavitation(&snapshots[0], &config);
        assert_eq!(result.risk_level, RiskLevel::Unknown);
        assert!(!result.features.validation_errors.is_empty());
        assert_eq!(result.belief.mean_damage, 0.0);
        assert_eq!(result.prediction.failure_probability_7d, 0.0);
    }
}
