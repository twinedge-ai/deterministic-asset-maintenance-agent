use asset_agent_core::schemas::{AnalysisResult, RiskLevel};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize)]
pub struct PromptPackage {
    pub prompt_hash: String,
    pub prompt_text: String,
}

pub fn build_prompt(analysis: &AnalysisResult) -> PromptPackage {
    let prompt_text = prompt_text(analysis);
    let prompt_hash = format!("sha256:{}", sha256_hex(prompt_text.as_bytes()));
    PromptPackage {
        prompt_hash,
        prompt_text,
    }
}

fn prompt_text(analysis: &AnalysisResult) -> String {
    let recommendation = &analysis.recommendation;

    format!(
        "Output exactly: {asset_id} has {risk_level} suspected low-NPSH cavitation risk; {recommended_action} is recommended; {lowest_cost_action} is lowest-cost; Human inspection must verify sensor and pump curve limitations; no autonomous plant commands.",
        asset_id = analysis.asset_id,
        risk_level = risk_label(&analysis.risk_level),
        recommended_action = recommendation.recommended_action,
        lowest_cost_action = recommendation.lowest_cost_action,
    )
}

pub fn risk_label(risk_level: &RiskLevel) -> &'static str {
    match risk_level {
        RiskLevel::Low => "LOW",
        RiskLevel::Medium => "MEDIUM",
        RiskLevel::High => "HIGH",
        RiskLevel::Unknown => "UNKNOWN",
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::build_prompt;
    use asset_agent_core::{
        config::load_model_config, features::analyze_cavitation, replay::read_trace,
    };
    use std::path::Path;

    #[test]
    fn prompt_hash_is_stable_sha256() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("config loads");
        let snapshots =
            read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl")).unwrap();
        let analysis = analyze_cavitation(&snapshots[0], &config);

        let first = build_prompt(&analysis);
        let second = build_prompt(&analysis);

        assert_eq!(first.prompt_hash, second.prompt_hash);
        assert!(first.prompt_hash.starts_with("sha256:"));
        assert!(first.prompt_text.contains("HIGH suspected"));
        assert!(first.prompt_text.contains("Human inspection"));
    }
}
