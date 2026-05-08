pub fn unavailable_message() -> &'static str {
    "Template explanations are active; mistral.rs is optional and not required for deterministic decisions."
}

pub fn template_text(analysis: &asset_agent_core::schemas::AnalysisResult) -> String {
    analysis.explanation.text.clone()
}
