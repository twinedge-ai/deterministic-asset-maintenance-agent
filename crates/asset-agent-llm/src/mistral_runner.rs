use crate::config::MistralConfig;
use serde::Serialize;
use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use thiserror::Error;

#[derive(Debug, Clone, Serialize)]
pub struct RunnerStatus {
    pub enabled: bool,
    pub available: bool,
    pub reason: String,
    pub provider: String,
    pub binary_path: String,
    pub model_path: String,
    pub seed: u64,
    pub max_tokens: u32,
    pub temperature: f64,
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("{0}")]
    Unavailable(String),
    #[error("failed to spawn mistral.rs runner: {0}")]
    Spawn(std::io::Error),
    #[error("mistral.rs runner timed out after {0} ms")]
    Timeout(u64),
    #[error("failed to wait for mistral.rs runner: {0}")]
    Wait(std::io::Error),
    #[error("mistral.rs runner exited with status {status}: {stderr}")]
    NonZero { status: String, stderr: String },
}

pub fn runner_status(config: &MistralConfig) -> RunnerStatus {
    if !config.enabled {
        return status(config, false, "disabled");
    }
    if !config.model_path.exists() {
        return status(config, false, "model_path_missing");
    }
    if resolve_binary(&config.binary_path).is_none() {
        return status(config, false, "binary_missing");
    }
    status(config, true, "available")
}

pub fn run_mistral(config: &MistralConfig, prompt_text: &str) -> Result<String, RunnerError> {
    let status = runner_status(config);
    if !status.available {
        return Err(RunnerError::Unavailable(status.reason));
    }
    let binary = resolve_binary(&config.binary_path)
        .ok_or_else(|| RunnerError::Unavailable("binary_missing".to_string()))?;
    let model_dir = config.model_path.parent().unwrap_or(Path::new("."));
    let model_file = config
        .model_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let args = config
        .args
        .iter()
        .map(|arg| {
            arg.replace("{model_path}", config.model_path.to_string_lossy().as_ref())
                .replace("{model_dir}", model_dir.to_string_lossy().as_ref())
                .replace("{model_file}", &model_file)
                .replace("{prompt}", prompt_text)
        })
        .collect::<Vec<_>>();
    let mut child = Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(RunnerError::Spawn)?;

    let started = Instant::now();
    loop {
        if child.try_wait().map_err(RunnerError::Wait)?.is_some() {
            let output = child.wait_with_output().map_err(RunnerError::Wait)?;
            if output.status.success() {
                return Ok(clean_stdout(&String::from_utf8_lossy(&output.stdout)));
            }
            return Err(RunnerError::NonZero {
                status: output.status.to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }
        if started.elapsed() >= Duration::from_millis(config.timeout_ms) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(RunnerError::Timeout(config.timeout_ms));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn status(config: &MistralConfig, available: bool, reason: &str) -> RunnerStatus {
    RunnerStatus {
        enabled: config.enabled,
        available,
        reason: reason.to_string(),
        provider: config.provider.clone(),
        binary_path: config.binary_path.clone(),
        model_path: config.model_path.to_string_lossy().to_string(),
        seed: config.seed,
        max_tokens: config.max_tokens,
        temperature: config.temperature,
    }
}

fn resolve_binary(binary_path: &str) -> Option<PathBuf> {
    let path = Path::new(binary_path);
    if path.components().count() > 1 {
        return path.exists().then(|| path.to_path_buf());
    }
    env::var_os("PATH").and_then(|paths| {
        env::split_paths(&paths)
            .map(|dir| dir.join(binary_path))
            .find(|candidate| candidate.exists())
    })
}

fn clean_stdout(stdout: &str) -> String {
    let generated = stdout
        .split("Model loaded, running one-shot mode...\n")
        .last()
        .unwrap_or(stdout);
    generated
        .split("\nStats:")
        .next()
        .unwrap_or(generated)
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::clean_stdout;

    #[test]
    fn clean_stdout_removes_cli_stats() {
        let output = clean_stdout("Generated text.\n\nStats:\nDecode: 10 tokens");
        assert_eq!(output, "Generated text.");
    }

    #[test]
    fn clean_stdout_removes_cli_log_preamble() {
        let output =
            clean_stdout("2026 INFO load\nModel loaded, running one-shot mode...\nGenerated text.");
        assert_eq!(output, "Generated text.");
    }
}
