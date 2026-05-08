use crate::sqlite::{MemoryError, MemoryResult, MemoryStore};
use asset_agent_core::schemas::{AnalysisResult, ObservationSnapshot};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize)]
pub struct CaseSummary {
    pub case_id: String,
    pub timestamp: String,
    pub asset_id: String,
    pub risk_level: Value,
    pub recommended_action: String,
    pub feedback_count: usize,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaseRecord {
    pub case_id: String,
    pub timestamp: String,
    pub asset_id: String,
    pub observations: Value,
    pub derived_features: Value,
    pub belief: Value,
    pub prediction: Value,
    pub recommendation: Value,
    pub explanation: Value,
    pub feedback: Value,
    pub post_action: Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SimilarCase {
    pub case_id: String,
    pub asset_id: String,
    pub label: String,
    pub distance: f64,
}

impl MemoryStore {
    pub fn store_analysis_case(
        &self,
        snapshot: &ObservationSnapshot,
        analysis: &AnalysisResult,
    ) -> MemoryResult<String> {
        let case_id = case_id_for_snapshot(snapshot);
        let observations_json = serde_json::to_string(snapshot)?;
        let derived_features_json = serde_json::to_string(&analysis.features)?;
        let belief_json = serde_json::to_string(&analysis.belief)?;
        let prediction_json = serde_json::to_string(&analysis.prediction)?;
        let recommendation_json = serde_json::to_string(&analysis.recommendation)?;
        let explanation_json = serde_json::to_string(&analysis.explanation)?;

        self.conn.execute(
            "INSERT INTO cases(
               case_id, timestamp, asset_id, observations_json, derived_features_json,
               belief_json, prediction_json, recommendation_json, explanation_json,
               feedback_json, post_action_json
             )
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, COALESCE(
               (SELECT feedback_json FROM cases WHERE case_id = ?1), '[]'
             ), COALESCE(
               (SELECT post_action_json FROM cases WHERE case_id = ?1), '{}'
             ))
             ON CONFLICT(case_id) DO UPDATE SET
               timestamp = excluded.timestamp,
               asset_id = excluded.asset_id,
               observations_json = excluded.observations_json,
               derived_features_json = excluded.derived_features_json,
               belief_json = excluded.belief_json,
               prediction_json = excluded.prediction_json,
               recommendation_json = excluded.recommendation_json,
               explanation_json = excluded.explanation_json",
            params![
                case_id,
                snapshot.as_of_timestamp,
                snapshot.asset_id,
                observations_json,
                derived_features_json,
                belief_json,
                prediction_json,
                recommendation_json,
                explanation_json
            ],
        )?;
        self.store_feature_vector(&case_id, &snapshot.asset_id, analysis)?;
        Ok(case_id)
    }

    pub fn list_cases(&self, limit: usize) -> MemoryResult<Vec<CaseSummary>> {
        let mut statement = self.conn.prepare(
            "SELECT c.case_id, c.timestamp, c.asset_id, c.recommendation_json,
                    c.feedback_json, c.created_at, s.label
             FROM cases c
             LEFT JOIN similar_case_index s ON s.case_id = c.case_id
             ORDER BY c.created_at DESC, c.case_id ASC
             LIMIT ?1",
        )?;
        let rows = statement.query_map([limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?;

        rows.map(|row| {
            let (
                case_id,
                timestamp,
                asset_id,
                recommendation_json,
                feedback_json,
                created_at,
                risk_label,
            ) = row?;
            let recommendation: Value = serde_json::from_str(&recommendation_json)?;
            let feedback = parse_json_or(feedback_json.as_deref(), json!([]))?;
            Ok(CaseSummary {
                case_id,
                timestamp,
                asset_id,
                risk_level: risk_label
                    .as_deref()
                    .map(serde_json::from_str)
                    .transpose()?
                    .unwrap_or(Value::Null),
                recommended_action: recommendation
                    .get("recommended_action")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                feedback_count: feedback.as_array().map(Vec::len).unwrap_or(0),
                created_at,
            })
        })
        .collect()
    }

    pub fn get_case(&self, case_id: &str) -> MemoryResult<Option<CaseRecord>> {
        let row = self
            .conn
            .query_row(
                "SELECT
                   case_id, timestamp, asset_id, observations_json, derived_features_json,
                   belief_json, prediction_json, recommendation_json, explanation_json,
                   feedback_json, post_action_json, created_at
                 FROM cases
                 WHERE case_id = ?1",
                [case_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, String>(7)?,
                        row.get::<_, String>(8)?,
                        row.get::<_, Option<String>>(9)?,
                        row.get::<_, Option<String>>(10)?,
                        row.get::<_, String>(11)?,
                    ))
                },
            )
            .optional()?;

        row.map(
            |(
                case_id,
                timestamp,
                asset_id,
                observations,
                derived_features,
                belief,
                prediction,
                recommendation,
                explanation,
                feedback,
                post_action,
                created_at,
            )| {
                Ok(CaseRecord {
                    case_id,
                    timestamp,
                    asset_id,
                    observations: serde_json::from_str(&observations)?,
                    derived_features: serde_json::from_str(&derived_features)?,
                    belief: serde_json::from_str(&belief)?,
                    prediction: serde_json::from_str(&prediction)?,
                    recommendation: serde_json::from_str(&recommendation)?,
                    explanation: serde_json::from_str(&explanation)?,
                    feedback: parse_json_or(feedback.as_deref(), json!([]))?,
                    post_action: parse_json_or(post_action.as_deref(), json!({}))?,
                    created_at,
                })
            },
        )
        .transpose()
    }

    pub fn similar_cases(&self, case_id: &str, limit: usize) -> MemoryResult<Vec<SimilarCase>> {
        let Some(target) = self.feature_vector(case_id)? else {
            return Ok(Vec::new());
        };
        let target_asset = target.asset_id.clone();
        let mut statement = self.conn.prepare(
            "SELECT case_id, asset_id, feature_vector_json, label
             FROM similar_case_index
             WHERE asset_id = ?1 AND case_id != ?2",
        )?;
        let rows = statement.query_map(params![target_asset, case_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut cases = Vec::new();
        for row in rows {
            let (case_id, asset_id, feature_vector_json, label) = row?;
            let vector: FeatureVector = serde_json::from_str(&feature_vector_json)?;
            cases.push(SimilarCase {
                case_id,
                asset_id,
                label,
                distance: round6(target.distance(&vector)),
            });
        }
        cases.sort_by(|left, right| {
            left.distance
                .total_cmp(&right.distance)
                .then_with(|| left.case_id.cmp(&right.case_id))
        });
        cases.truncate(limit);
        Ok(cases)
    }

    pub(crate) fn feedback_entries(&self, case_id: &str) -> MemoryResult<Vec<Value>> {
        let feedback_json = self
            .conn
            .query_row(
                "SELECT feedback_json FROM cases WHERE case_id = ?1",
                [case_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .ok_or_else(|| MemoryError::CaseNotFound(case_id.to_string()))?;
        let feedback = parse_json_or(feedback_json.as_deref(), json!([]))?;
        Ok(feedback.as_array().cloned().unwrap_or_default())
    }

    pub(crate) fn case_features(&self, case_id: &str) -> MemoryResult<Value> {
        self.conn
            .query_row(
                "SELECT derived_features_json FROM cases WHERE case_id = ?1",
                [case_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or_else(|| MemoryError::CaseNotFound(case_id.to_string()))
            .and_then(|json| serde_json::from_str(&json).map_err(MemoryError::from))
    }

    pub(crate) fn case_asset_id(&self, case_id: &str) -> MemoryResult<String> {
        self.conn
            .query_row(
                "SELECT asset_id FROM cases WHERE case_id = ?1",
                [case_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| MemoryError::CaseNotFound(case_id.to_string()))
    }

    fn store_feature_vector(
        &self,
        case_id: &str,
        asset_id: &str,
        analysis: &AnalysisResult,
    ) -> MemoryResult<()> {
        let vector = FeatureVector {
            asset_id: asset_id.to_string(),
            npsh_margin_m: analysis.features.npsh_margin_m.unwrap_or(0.0),
            cavitation_evidence_score: analysis.features.cavitation_evidence_score,
            vibration_score: analysis.features.vibration_score,
            current_score: analysis.features.current_score,
            thermal_score: analysis.features.thermal_score,
        };
        self.conn.execute(
            "INSERT INTO similar_case_index(case_id, asset_id, feature_vector_json, label)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(case_id) DO UPDATE SET
               asset_id = excluded.asset_id,
               feature_vector_json = excluded.feature_vector_json,
               label = excluded.label",
            params![
                case_id,
                asset_id,
                serde_json::to_string(&vector)?,
                serde_json::to_string(&analysis.risk_level)?
            ],
        )?;
        Ok(())
    }

    fn feature_vector(&self, case_id: &str) -> MemoryResult<Option<FeatureVector>> {
        self.conn
            .query_row(
                "SELECT asset_id, feature_vector_json FROM similar_case_index WHERE case_id = ?1",
                [case_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .map(|(asset_id, vector_json)| {
                let mut vector: FeatureVector = serde_json::from_str(&vector_json)?;
                vector.asset_id = asset_id;
                Ok(vector)
            })
            .transpose()
    }
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
struct FeatureVector {
    asset_id: String,
    npsh_margin_m: f64,
    cavitation_evidence_score: f64,
    vibration_score: f64,
    current_score: f64,
    thermal_score: f64,
}

impl FeatureVector {
    fn distance(&self, other: &Self) -> f64 {
        let npsh = (self.npsh_margin_m - other.npsh_margin_m).abs() / 30.0;
        let evidence = (self.cavitation_evidence_score - other.cavitation_evidence_score).abs();
        let vibration = (self.vibration_score - other.vibration_score).abs();
        let current = (self.current_score - other.current_score).abs();
        let thermal = (self.thermal_score - other.thermal_score).abs();
        npsh + evidence + 0.5 * vibration + 0.5 * current + 0.25 * thermal
    }
}

fn case_id_for_snapshot(snapshot: &ObservationSnapshot) -> String {
    let raw = format!("{}_{}", snapshot.asset_id, snapshot.as_of_timestamp);
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn parse_json_or(input: Option<&str>, default: Value) -> MemoryResult<Value> {
    input
        .filter(|value| !value.trim().is_empty())
        .map(serde_json::from_str)
        .transpose()
        .map(|value| value.unwrap_or(default))
        .map_err(MemoryError::from)
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}
