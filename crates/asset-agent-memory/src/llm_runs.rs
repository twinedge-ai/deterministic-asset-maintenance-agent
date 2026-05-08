use crate::sqlite::{MemoryResult, MemoryStore};
use rusqlite::params;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct LlmRunInput {
    pub provider: String,
    pub model_path: String,
    pub binary_path: String,
    pub prompt_json: Value,
    pub prompt_text: String,
    pub raw_output: Option<String>,
    pub guarded_output: String,
    pub guard_passed: bool,
    pub fallback_used: bool,
    pub latency_ms: f64,
}

impl MemoryStore {
    pub fn store_llm_run(
        &self,
        case_id: Option<&str>,
        prompt_hash: &str,
        input: &LlmRunInput,
    ) -> MemoryResult<String> {
        let next_sequence = self.next_llm_sequence(case_id)?;
        let llm_run_id = llm_run_id(case_id, prompt_hash, next_sequence);
        self.conn.execute(
            "INSERT INTO llm_runs(
               llm_run_id, case_id, provider, model_path, binary_path, prompt_json,
               prompt_text, raw_output, guarded_output, guard_passed, fallback_used,
               latency_ms
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                llm_run_id,
                case_id,
                input.provider,
                input.model_path,
                input.binary_path,
                input.prompt_json.to_string(),
                input.prompt_text,
                input.raw_output,
                input.guarded_output,
                i64::from(input.guard_passed),
                i64::from(input.fallback_used),
                input.latency_ms
            ],
        )?;
        Ok(llm_run_id)
    }

    fn next_llm_sequence(&self, case_id: Option<&str>) -> MemoryResult<i64> {
        let count = if let Some(case_id) = case_id {
            self.conn.query_row(
                "SELECT COUNT(*) FROM llm_runs WHERE case_id = ?1",
                [case_id],
                |row| row.get::<_, i64>(0),
            )?
        } else {
            self.conn.query_row(
                "SELECT COUNT(*) FROM llm_runs WHERE case_id IS NULL",
                [],
                |row| row.get::<_, i64>(0),
            )?
        };
        Ok(count + 1)
    }
}

fn llm_run_id(case_id: Option<&str>, prompt_hash: &str, sequence: i64) -> String {
    let hash = prompt_hash
        .strip_prefix("sha256:")
        .unwrap_or(prompt_hash)
        .chars()
        .take(16)
        .collect::<String>();
    sanitize(&format!(
        "llm_{}_{}_{}",
        case_id.unwrap_or("no_case"),
        hash,
        sequence
    ))
}

fn sanitize(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}
