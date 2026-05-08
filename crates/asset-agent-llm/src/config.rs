use serde::Serialize;
use std::{
    env,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize)]
pub struct MistralConfig {
    pub enabled: bool,
    pub provider: String,
    pub model_path: PathBuf,
    pub binary_path: String,
    pub args: Vec<String>,
    pub timeout_ms: u64,
    pub seed: u64,
    pub max_tokens: u32,
    pub temperature: f64,
}

impl MistralConfig {
    pub fn from_env(project_root: impl AsRef<Path>) -> Self {
        let project_root = project_root.as_ref();
        let model_path = env::var("MISTRALRS_MODEL_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("models/mistral/model.gguf"));
        let model_path = if model_path.is_absolute() {
            model_path
        } else {
            project_root.join(model_path)
        };
        let binary_path = env::var("MISTRALRS_BINARY").unwrap_or_else(|_| "mistralrs".to_string());
        let seed = env_u64("MISTRALRS_SEED", 42);
        let max_tokens = env_u64("MISTRALRS_MAX_TOKENS", 220) as u32;
        let temperature = env_f64("MISTRALRS_TEMPERATURE", 0.0);
        Self {
            enabled: env_bool("ASSET_AGENT_LLM_ENABLED"),
            provider: "mistralrs".to_string(),
            model_path,
            binary_path,
            args: runner_args(seed, max_tokens, temperature),
            timeout_ms: env_u64("MISTRALRS_TIMEOUT_MS", 3_000),
            seed,
            max_tokens,
            temperature,
        }
    }
}

fn runner_args(_seed: u64, _max_tokens: u32, _temperature: f64) -> Vec<String> {
    if let Ok(value) = env::var("MISTRALRS_ARGS") {
        return value
            .split_whitespace()
            .map(|part| part.to_string())
            .collect();
    }
    vec![
        "run".to_string(),
        "-m".to_string(),
        "{model_dir}".to_string(),
        "--format".to_string(),
        "gguf".to_string(),
        "-f".to_string(),
        "{model_file}".to_string(),
        "--cpu".to_string(),
        "-i".to_string(),
        "{prompt}".to_string(),
    ]
}

fn env_bool(name: &str) -> bool {
    env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn env_f64(name: &str, default: f64) -> f64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<f64>().ok())
        .unwrap_or(default)
}
