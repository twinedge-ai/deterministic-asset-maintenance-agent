use crate::{
    config::MistralConfig,
    fallback_templates::template_text,
    mistral_runner::{run_mistral, runner_status, RunnerStatus},
    output_guard::{guard_output, GuardResult},
    prompt_builder::{build_prompt, PromptPackage},
};
use asset_agent_core::schemas::AnalysisResult;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ExplanationTrace {
    pub source: String,
    pub llm_available: bool,
    pub prompt_hash: String,
    pub prompt_text: String,
    pub raw_output: Option<String>,
    pub guarded_output: String,
    pub guard_passed: bool,
    pub fallback_used: bool,
    pub guard_reasons: Vec<String>,
    pub runner: RunnerStatus,
}

pub fn explain(analysis: &AnalysisResult, config: &MistralConfig) -> ExplanationTrace {
    let prompt = build_prompt(analysis);
    let runner = runner_status(config);
    if !runner.available {
        return fallback_trace(analysis, prompt, runner, vec![runner_status_reason(config)]);
    }

    match run_mistral(config, &prompt.prompt_text) {
        Ok(raw_output) => explain_from_raw_output(analysis, config, prompt, runner, raw_output),
        Err(error) => fallback_trace(
            analysis,
            prompt,
            runner,
            vec![format!("runner_error:{error}")],
        ),
    }
}

pub fn explain_from_raw_output(
    analysis: &AnalysisResult,
    _config: &MistralConfig,
    prompt: PromptPackage,
    runner: RunnerStatus,
    raw_output: String,
) -> ExplanationTrace {
    let guard = guard_output(&raw_output, analysis);
    if guard.passed {
        ExplanationTrace {
            source: "mistralrs".to_string(),
            llm_available: runner.available,
            prompt_hash: prompt.prompt_hash,
            prompt_text: prompt.prompt_text,
            raw_output: Some(raw_output),
            guarded_output: guard.guarded_output.unwrap_or_default(),
            guard_passed: true,
            fallback_used: false,
            guard_reasons: Vec::new(),
            runner,
        }
    } else {
        rejected_trace(analysis, prompt, runner, raw_output, guard)
    }
}

fn fallback_trace(
    analysis: &AnalysisResult,
    prompt: PromptPackage,
    runner: RunnerStatus,
    guard_reasons: Vec<String>,
) -> ExplanationTrace {
    ExplanationTrace {
        source: "template".to_string(),
        llm_available: runner.available,
        prompt_hash: prompt.prompt_hash,
        prompt_text: prompt.prompt_text,
        raw_output: None,
        guarded_output: template_text(analysis),
        guard_passed: false,
        fallback_used: true,
        guard_reasons,
        runner,
    }
}

fn rejected_trace(
    analysis: &AnalysisResult,
    prompt: PromptPackage,
    runner: RunnerStatus,
    raw_output: String,
    guard: GuardResult,
) -> ExplanationTrace {
    ExplanationTrace {
        source: "template".to_string(),
        llm_available: runner.available,
        prompt_hash: prompt.prompt_hash,
        prompt_text: prompt.prompt_text,
        raw_output: Some(raw_output),
        guarded_output: template_text(analysis),
        guard_passed: false,
        fallback_used: true,
        guard_reasons: guard.reasons,
        runner,
    }
}

fn runner_status_reason(config: &MistralConfig) -> String {
    runner_status(config).reason
}

#[cfg(test)]
mod tests {
    use super::{explain, explain_from_raw_output};
    use crate::{
        config::MistralConfig, mistral_runner::runner_status, prompt_builder::build_prompt,
    };
    use asset_agent_core::{
        config::load_model_config, features::analyze_cavitation, replay::read_trace,
    };
    use std::{path::Path, path::PathBuf};

    fn analysis() -> asset_agent_core::schemas::AnalysisResult {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("config loads");
        let snapshots =
            read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl")).unwrap();
        analyze_cavitation(&snapshots[0], &config)
    }

    fn disabled_config() -> MistralConfig {
        MistralConfig {
            enabled: false,
            provider: "mistralrs".to_string(),
            model_path: PathBuf::from("models/mistral/model.gguf"),
            binary_path: "mistralrs".to_string(),
            args: Vec::new(),
            timeout_ms: 1,
            seed: 42,
            max_tokens: 220,
            temperature: 0.0,
        }
    }

    #[test]
    fn unavailable_runner_returns_template_fallback() {
        let analysis = analysis();
        let trace = explain(&analysis, &disabled_config());
        assert_eq!(trace.source, "template");
        assert!(!trace.llm_available);
        assert!(trace.fallback_used);
        assert_eq!(trace.guarded_output, analysis.explanation.text);
        assert_eq!(trace.guard_reasons, vec!["disabled"]);
    }

    #[test]
    fn rejected_raw_output_returns_template_fallback() {
        let analysis = analysis();
        let config = disabled_config();
        let mut runner = runner_status(&config);
        runner.available = true;
        let prompt = build_prompt(&analysis);
        let trace = explain_from_raw_output(
            &analysis,
            &config,
            prompt,
            runner,
            "hp_pump_1 has LOW risk and confirmed cavitation. Automatically stop the pump."
                .to_string(),
        );

        assert_eq!(trace.source, "template");
        assert!(trace.fallback_used);
        assert!(!trace.guard_passed);
        assert!(trace
            .guard_reasons
            .iter()
            .any(|reason| reason.starts_with("changes_risk_level")));
        assert_eq!(trace.guarded_output, analysis.explanation.text);
    }
}
