use crate::prompt_builder::risk_label;
use asset_agent_core::schemas::AnalysisResult;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct GuardResult {
    pub passed: bool,
    pub guarded_output: Option<String>,
    pub reasons: Vec<String>,
}

pub fn guard_output(raw_output: &str, analysis: &AnalysisResult) -> GuardResult {
    let mut reasons = Vec::new();
    let output = raw_output.trim();
    let lower = output.to_ascii_lowercase();

    if output.is_empty() {
        reasons.push("empty_output".to_string());
    }
    if output.len() > 2_500 {
        reasons.push("output_too_long".to_string());
    }

    guard_risk_level(&lower, analysis, &mut reasons);
    guard_no_certainty(&lower, &mut reasons);
    guard_no_autonomous_control(&lower, &mut reasons);
    guard_human_limitations(&lower, &mut reasons);
    guard_numbers(output, analysis, &mut reasons);

    if reasons.is_empty() {
        GuardResult {
            passed: true,
            guarded_output: Some(output.to_string()),
            reasons,
        }
    } else {
        GuardResult {
            passed: false,
            guarded_output: None,
            reasons,
        }
    }
}

fn guard_risk_level(lower: &str, analysis: &AnalysisResult, reasons: &mut Vec<String>) {
    let actual = risk_label(&analysis.risk_level);
    let actual_lower = actual.to_ascii_lowercase();
    if !lower.contains(&actual_lower) {
        reasons.push("omits_deterministic_risk_level".to_string());
    }
    for candidate in ["LOW", "MEDIUM", "HIGH", "UNKNOWN"] {
        if candidate == actual {
            continue;
        }
        let candidate_lower = candidate.to_ascii_lowercase();
        let changed = [
            format!("{candidate_lower} risk"),
            format!("risk level {candidate_lower}"),
            format!("risk is {candidate_lower}"),
            format!("risk: {candidate_lower}"),
        ]
        .into_iter()
        .any(|pattern| lower.contains(&pattern));
        if changed {
            reasons.push(format!("changes_risk_level_to_{candidate_lower}"));
        }
    }
}

fn guard_no_certainty(lower: &str, reasons: &mut Vec<String>) {
    let blocked = [
        "guaranteed cavitation",
        "confirmed cavitation",
        "definite cavitation",
        "certain cavitation",
        "certainly cavitating",
        "will fail",
        "will cavitate",
    ];
    if blocked.iter().any(|phrase| lower.contains(phrase)) {
        reasons.push("claims_certainty".to_string());
    }
}

fn guard_no_autonomous_control(lower: &str, reasons: &mut Vec<String>) {
    let blocked = [
        "autonomously",
        "automatically open",
        "automatically close",
        "open the valve",
        "close the valve",
        "start the pump",
        "stop the pump",
        "write to plc",
        "send command",
        "issue command",
    ];
    if blocked.iter().any(|phrase| lower.contains(phrase)) {
        reasons.push("recommends_autonomous_plant_control".to_string());
    }
}

fn guard_human_limitations(lower: &str, reasons: &mut Vec<String>) {
    let has_human = lower.contains("human");
    let has_inspection = lower.contains("inspection")
        || lower.contains("inspect")
        || lower.contains("verify")
        || lower.contains("review");
    let has_limitation =
        lower.contains("limitation") || lower.contains("sensor") || lower.contains("pump curve");
    if !(has_human && has_inspection && has_limitation) {
        reasons.push("omits_human_inspection_limitations".to_string());
    }
}

fn guard_numbers(output: &str, analysis: &AnalysisResult, reasons: &mut Vec<String>) {
    let allowed = allowed_numbers(analysis);
    for number in extract_numbers(output) {
        if number.integer_token && number.value.fract().abs() <= f64::EPSILON {
            let integer = number.value as i64;
            if (0..=100).contains(&integer) {
                continue;
            }
        }
        if !allowed
            .iter()
            .any(|allowed| (number.value - allowed).abs() <= 0.000_5)
        {
            reasons.push(format!("invents_numeric_value_{:.6}", number.value));
            return;
        }
    }
}

fn allowed_numbers(analysis: &AnalysisResult) -> Vec<f64> {
    let mut values = vec![
        analysis.features.cavitation_evidence_score,
        analysis.features.confidence,
        analysis.belief.mean_damage,
        analysis.belief.damage_variance,
        analysis.recommendation.action_confidence,
        analysis.prediction.rul_hours,
        analysis.prediction.failure_probability_24h,
        analysis.prediction.failure_probability_7d,
        analysis.prediction.failure_probability_30d,
        analysis.prediction.cvar95,
    ];
    values.extend(
        [
            analysis.features.npsha_m,
            analysis.features.npshr_m,
            analysis.features.npsh_margin_m,
            analysis.features.npsh_margin_ratio,
            analysis.features.pump_head_m,
            analysis.features.pressure_delta_kpa,
        ]
        .into_iter()
        .flatten(),
    );
    values.extend(analysis.counterfactual_costs.values().copied());
    values
}

#[derive(Debug)]
struct NumberToken {
    value: f64,
    integer_token: bool,
}

fn extract_numbers(input: &str) -> Vec<NumberToken> {
    let mut numbers = Vec::new();
    let mut token = String::new();
    for ch in input.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_digit() || (ch == '-' && token.is_empty()) || ch == '.' {
            token.push(ch);
            continue;
        }
        if token.chars().any(|ch| ch.is_ascii_digit()) {
            if let Ok(value) = token.parse::<f64>() {
                numbers.push(NumberToken {
                    value,
                    integer_token: !token.contains('.'),
                });
            }
        }
        token.clear();
    }
    numbers
}

#[cfg(test)]
mod tests {
    use super::guard_output;
    use asset_agent_core::{
        config::load_model_config, features::analyze_cavitation, replay::read_trace,
    };
    use std::path::Path;

    fn analysis() -> asset_agent_core::schemas::AnalysisResult {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("config loads");
        let snapshots =
            read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl")).unwrap();
        analyze_cavitation(&snapshots[0], &config)
    }

    #[test]
    fn guard_accepts_safe_explanation() {
        let analysis = analysis();
        let output = "hp_pump_1 has HIGH suspected cavitation risk. Recommended action is replace, while the lowest-cost action is defer. Human inspection should verify sensor health and pump curve limitations before any maintenance action.";
        let guard = guard_output(output, &analysis);
        assert!(guard.passed, "{:?}", guard.reasons);
    }

    #[test]
    fn guard_rejects_certainty_and_commands() {
        let analysis = analysis();
        let output =
            "hp_pump_1 has HIGH confirmed cavitation and will fail. Automatically open the valve.";
        let guard = guard_output(output, &analysis);
        assert!(!guard.passed);
        assert!(guard.reasons.contains(&"claims_certainty".to_string()));
        assert!(guard
            .reasons
            .contains(&"recommends_autonomous_plant_control".to_string()));
    }

    #[test]
    fn guard_rejects_changed_risk_and_invented_values() {
        let analysis = analysis();
        let output = "hp_pump_1 has LOW risk with suction pressure 9.9 bar. Human inspection should verify sensor limitations.";
        let guard = guard_output(output, &analysis);
        assert!(!guard.passed);
        assert!(guard
            .reasons
            .iter()
            .any(|reason| reason.starts_with("changes_risk_level")));
        assert!(guard
            .reasons
            .iter()
            .any(|reason| reason.starts_with("invents_numeric_value")));
    }
}
