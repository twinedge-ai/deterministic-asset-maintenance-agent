use crate::sqlite::{MemoryError, MemoryResult, MemoryStore};
use asset_agent_core::{
    checklist::{checklist_item_order, checklist_item_text},
    config::ModelConfig,
};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct LearningSettings {
    pub checklist_alpha: f64,
    pub checklist_beta: f64,
    pub threshold_proposal_min_confirmed_cases: usize,
    pub threshold_proposal_status: String,
}

impl LearningSettings {
    pub fn from_model_config(config: &ModelConfig) -> Self {
        Self {
            checklist_alpha: config_value(&config.learning, "checklist_alpha", 1.0),
            checklist_beta: config_value(&config.learning, "checklist_beta", 2.0),
            threshold_proposal_min_confirmed_cases: config
                .learning
                .get("threshold_proposal_min_confirmed_cases")
                .and_then(Value::as_u64)
                .unwrap_or(3) as usize,
            threshold_proposal_status: config
                .learning
                .get("threshold_proposal_status")
                .and_then(Value::as_str)
                .unwrap_or("requires_review")
                .to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeedbackInput {
    #[serde(default)]
    pub confirmed_cavitation: bool,
    #[serde(default)]
    pub solved_item_id: Option<String>,
    #[serde(default)]
    pub false_alarm: bool,
    #[serde(default)]
    pub missed_cavitation: bool,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub post_action: Option<Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChecklistRankingItem {
    pub item_id: String,
    pub item_text: String,
    pub fault_type: String,
    pub successes: i64,
    pub attempts: i64,
    pub expected_success: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThresholdProposal {
    pub asset_id: String,
    pub threshold_name: String,
    pub default_value: f64,
    pub current_value: f64,
    pub proposed_value: Option<f64>,
    pub evidence_count: i64,
    pub status: String,
    pub last_updated: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalibrationSummary {
    pub false_alarm_count: i64,
    pub missed_cavitation_count: i64,
    pub review_required_count: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeedbackResult {
    pub case_id: String,
    pub feedback_sequence: usize,
    pub top_checklist_item_id: Option<String>,
    pub threshold_proposals: Vec<ThresholdProposal>,
    pub calibration_summary: CalibrationSummary,
}

#[derive(Debug, Clone, Serialize)]
pub struct LearningSummary {
    pub checklist: Vec<ChecklistRankingItem>,
    pub thresholds: Vec<ThresholdProposal>,
    pub calibration: CalibrationSummary,
}

impl MemoryStore {
    pub fn submit_feedback(
        &self,
        case_id: &str,
        feedback: FeedbackInput,
        settings: &LearningSettings,
    ) -> MemoryResult<FeedbackResult> {
        let asset_id = self.case_asset_id(case_id)?;
        let solved_item_id = feedback.solved_item_id.clone();
        let post_action_json = feedback.post_action.as_ref().map(Value::to_string);
        if let Some(item_id) = solved_item_id.as_deref() {
            if checklist_item_text(item_id).is_none() {
                return Err(MemoryError::InvalidFeedback(format!(
                    "unknown checklist item_id {item_id}"
                )));
            }
        }

        let mut entries = self.feedback_entries(case_id)?;
        let sequence = entries.len() + 1;
        let entry = json!({
            "sequence": sequence,
            "confirmed_cavitation": feedback.confirmed_cavitation,
            "solved_item_id": solved_item_id,
            "false_alarm": feedback.false_alarm,
            "missed_cavitation": feedback.missed_cavitation,
            "notes": feedback.notes.clone()
        });
        entries.push(entry);
        self.conn.execute(
            "UPDATE cases SET feedback_json = ?1, post_action_json = COALESCE(?2, post_action_json)
             WHERE case_id = ?3",
            params![serde_json::to_string(&entries)?, post_action_json, case_id],
        )?;

        if let Some(item_id) = entries
            .last()
            .and_then(|entry| entry.get("solved_item_id"))
            .and_then(Value::as_str)
        {
            let success_increment = i64::from(feedback.confirmed_cavitation);
            self.conn.execute(
                "UPDATE checklist_stats
                 SET attempts = attempts + 1,
                     successes = successes + ?1,
                     last_updated = CURRENT_TIMESTAMP
                 WHERE item_id = ?2",
                params![success_increment, item_id],
            )?;
        }

        self.update_threshold_proposals(&asset_id, settings)?;
        self.record_calibration_event(case_id, &asset_id, &feedback)?;

        let checklist = self.checklist_ranking(settings)?;
        let top_checklist_item_id = checklist.first().map(|item| item.item_id.clone());
        Ok(FeedbackResult {
            case_id: case_id.to_string(),
            feedback_sequence: sequence,
            top_checklist_item_id,
            threshold_proposals: self.thresholds()?,
            calibration_summary: self.calibration_summary()?,
        })
    }

    pub fn checklist_ranking(
        &self,
        settings: &LearningSettings,
    ) -> MemoryResult<Vec<ChecklistRankingItem>> {
        let mut statement = self.conn.prepare(
            "SELECT item_id, item_text, fault_type, successes, attempts FROM checklist_stats",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ChecklistRankingItem {
                item_id: row.get(0)?,
                item_text: row.get(1)?,
                fault_type: row.get(2)?,
                successes: row.get(3)?,
                attempts: row.get(4)?,
                expected_success: 0.0,
            })
        })?;

        let mut items = rows
            .map(|row| {
                let mut item = row?;
                item.expected_success = expected_success(
                    item.successes,
                    item.attempts,
                    settings.checklist_alpha,
                    settings.checklist_beta,
                );
                Ok(item)
            })
            .collect::<MemoryResult<Vec<_>>>()?;

        items.sort_by(|left, right| {
            right
                .expected_success
                .total_cmp(&left.expected_success)
                .then_with(|| right.successes.cmp(&left.successes))
                .then_with(|| right.attempts.cmp(&left.attempts))
                .then_with(|| {
                    checklist_item_order(&left.item_id).cmp(&checklist_item_order(&right.item_id))
                })
                .then_with(|| left.item_id.cmp(&right.item_id))
        });
        Ok(items)
    }

    pub fn thresholds(&self) -> MemoryResult<Vec<ThresholdProposal>> {
        let mut statement = self.conn.prepare(
            "SELECT asset_id, threshold_name, default_value, current_value, proposed_value,
                    evidence_count, status, last_updated
             FROM pump_thresholds
             ORDER BY asset_id, threshold_name",
        )?;
        let rows = statement.query_map([], |row| {
            Ok(ThresholdProposal {
                asset_id: row.get(0)?,
                threshold_name: row.get(1)?,
                default_value: row.get(2)?,
                current_value: row.get(3)?,
                proposed_value: row.get(4)?,
                evidence_count: row.get(5)?,
                status: row.get(6)?,
                last_updated: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(MemoryError::from)
    }

    pub fn calibration_summary(&self) -> MemoryResult<CalibrationSummary> {
        let false_alarm_count = self.count_calibration_events("false_alarm")?;
        let missed_cavitation_count = self.count_calibration_events("missed_cavitation")?;
        let review_required_count = self.conn.query_row(
            "SELECT COUNT(*) FROM calibration_events WHERE requires_review = 1",
            [],
            |row| row.get(0),
        )?;
        Ok(CalibrationSummary {
            false_alarm_count,
            missed_cavitation_count,
            review_required_count,
        })
    }

    pub fn learning_summary(&self, settings: &LearningSettings) -> MemoryResult<LearningSummary> {
        Ok(LearningSummary {
            checklist: self.checklist_ranking(settings)?,
            thresholds: self.thresholds()?,
            calibration: self.calibration_summary()?,
        })
    }

    fn update_threshold_proposals(
        &self,
        asset_id: &str,
        settings: &LearningSettings,
    ) -> MemoryResult<()> {
        let Some((current_value, default_value)) = self
            .conn
            .query_row(
                "SELECT current_value, default_value FROM pump_thresholds
                 WHERE asset_id = ?1 AND threshold_name = 'npsh_margin_high_risk_m'",
                [asset_id],
                |row| Ok((row.get::<_, f64>(0)?, row.get::<_, f64>(1)?)),
            )
            .optional()?
        else {
            return Ok(());
        };

        let evidence = self.threshold_evidence(asset_id, current_value)?;
        let evidence_count = evidence.len() as i64;
        let proposed_value = if evidence.len() >= settings.threshold_proposal_min_confirmed_cases {
            evidence.into_iter().reduce(f64::max).map(round6)
        } else {
            None
        };
        let status = if proposed_value.is_some() {
            settings.threshold_proposal_status.as_str()
        } else {
            "collecting_evidence"
        };

        self.conn.execute(
            "UPDATE pump_thresholds
             SET default_value = ?1,
                 current_value = ?2,
                 proposed_value = ?3,
                 evidence_count = ?4,
                 status = ?5,
                 last_updated = CURRENT_TIMESTAMP
             WHERE asset_id = ?6 AND threshold_name = 'npsh_margin_high_risk_m'",
            params![
                default_value,
                current_value,
                proposed_value,
                evidence_count,
                status,
                asset_id
            ],
        )?;
        Ok(())
    }

    fn threshold_evidence(&self, asset_id: &str, current_value: f64) -> MemoryResult<Vec<f64>> {
        let mut statement = self.conn.prepare(
            "SELECT case_id, derived_features_json, feedback_json FROM cases WHERE asset_id = ?1",
        )?;
        let rows = statement.query_map([asset_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;

        let mut evidence = Vec::new();
        for row in rows {
            let (_case_id, features_json, feedback_json) = row?;
            let features: Value = serde_json::from_str(&features_json)?;
            let Some(npsh_margin) = features.get("npsh_margin_m").and_then(Value::as_f64) else {
                continue;
            };
            if npsh_margin < current_value {
                continue;
            }
            let feedback = parse_feedback_array(feedback_json.as_deref())?;
            let confirmed_count = feedback
                .iter()
                .filter(|entry| {
                    entry
                        .get("confirmed_cavitation")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                })
                .count();
            evidence.extend(std::iter::repeat_n(npsh_margin, confirmed_count));
        }
        Ok(evidence)
    }

    fn record_calibration_event(
        &self,
        case_id: &str,
        asset_id: &str,
        feedback: &FeedbackInput,
    ) -> MemoryResult<()> {
        if !feedback.false_alarm && !feedback.missed_cavitation {
            return Ok(());
        }

        let features = self.case_features(case_id)?;
        let feature_confidence = features
            .get("confidence")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let events = [
            ("false_alarm", feedback.false_alarm, -0.05),
            ("missed_cavitation", feedback.missed_cavitation, 0.05),
        ];
        for (event_type, enabled, confidence_delta) in events {
            if !enabled {
                continue;
            }
            let event_id = format!(
                "{}_{}_{}",
                case_id,
                event_type,
                self.count_calibration_events_for_case(case_id, event_type)? + 1
            );
            self.conn.execute(
                "INSERT OR IGNORE INTO calibration_events(
                   event_id, asset_id, event_type, before_json, after_json,
                   reason, requires_review
                 )
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6, 1)",
                params![
                    event_id,
                    asset_id,
                    event_type,
                    json!({ "feature_confidence": feature_confidence }).to_string(),
                    json!({
                        "confidence_delta": confidence_delta,
                        "applied_to_diagnostic_truth": false
                    })
                    .to_string(),
                    format!("{event_type} feedback requires confidence calibration review")
                ],
            )?;
        }
        Ok(())
    }

    fn count_calibration_events(&self, event_type: &str) -> MemoryResult<i64> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM calibration_events WHERE event_type = ?1",
                [event_type],
                |row| row.get(0),
            )
            .map_err(MemoryError::from)
    }

    fn count_calibration_events_for_case(
        &self,
        case_id: &str,
        event_type: &str,
    ) -> MemoryResult<i64> {
        let prefix = format!("{case_id}_{event_type}_%");
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM calibration_events
                 WHERE event_id LIKE ?1 AND event_type = ?2",
                params![prefix, event_type],
                |row| row.get(0),
            )
            .map_err(MemoryError::from)
    }
}

fn expected_success(successes: i64, attempts: i64, alpha: f64, beta: f64) -> f64 {
    round6((successes as f64 + alpha) / (attempts as f64 + alpha + beta))
}

fn parse_feedback_array(input: Option<&str>) -> MemoryResult<Vec<Value>> {
    let value = input
        .filter(|value| !value.trim().is_empty())
        .map(serde_json::from_str)
        .transpose()?
        .unwrap_or_else(|| json!([]));
    Ok(value.as_array().cloned().unwrap_or_default())
}

fn config_value(value: &Value, key: &str, default: f64) -> f64 {
    value.get(key).and_then(Value::as_f64).unwrap_or(default)
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::{FeedbackInput, LearningSettings};
    use crate::sqlite::MemoryStore;
    use asset_agent_core::{
        config::load_model_config, features::analyze_cavitation, replay::read_trace,
    };
    use std::path::Path;

    #[test]
    fn confirmed_feedback_updates_checklist_and_threshold_proposal() {
        let store = MemoryStore::open_in_memory().expect("memory db opens");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("config loads");
        let settings = LearningSettings::from_model_config(&config);
        let snapshots =
            read_trace(root.join("traces/hp_pump_1_high_vibration.jsonl")).expect("trace loads");
        let analysis = analyze_cavitation(&snapshots[0], &config);
        let case_id = store
            .store_analysis_case(&snapshots[0], &analysis)
            .expect("case stores");

        for _ in 0..3 {
            store
                .submit_feedback(
                    &case_id,
                    FeedbackInput {
                        confirmed_cavitation: true,
                        solved_item_id: Some("check_suction_strainer".to_string()),
                        false_alarm: false,
                        missed_cavitation: false,
                        notes: None,
                        post_action: None,
                    },
                    &settings,
                )
                .expect("feedback stores");
        }

        let checklist = store.checklist_ranking(&settings).expect("checklist ranks");
        assert_eq!(checklist[0].item_id, "check_suction_strainer");
        assert_eq!(checklist[0].attempts, 3);
        assert_eq!(checklist[0].successes, 3);

        let threshold = store.thresholds().expect("thresholds load").remove(0);
        assert_eq!(threshold.status, "requires_review");
        assert_eq!(threshold.evidence_count, 3);
        assert_eq!(threshold.current_value, 0.20);
        assert!(threshold.proposed_value.unwrap() > threshold.current_value);
    }
}
