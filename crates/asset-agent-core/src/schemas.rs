use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowFeatures {
    pub flow_rate_std_m3h: f64,
    pub motor_current_std_a: f64,
    pub vibration_max_mms: f64,
    pub suction_pressure_min_bar: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesignSpecs {
    pub rated_flow_m3h: f64,
    pub rated_head_m: f64,
    pub rated_speed_rpm: f64,
    pub npshr_m: f64,
    pub efficiency_percent: f64,
    pub power_kw: f64,
    pub specific_gravity: f64,
    pub design_suction_pressure_barg: f64,
    pub max_discharge_pressure_barg: f64,
    pub min_flow_m3h: f64,
    pub max_flow_m3h: f64,
    pub min_speed_rpm: f64,
    pub pump_weight_kg: f64,
    pub baseplate_weight_kg: f64,
    pub stages: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationSnapshot {
    pub asset_id: String,
    pub timestamp: String,
    pub as_of_timestamp: String,
    pub window_seconds: u32,
    pub sample_count: u32,
    pub flow_rate_m3h: f64,
    pub suction_pressure_bar: f64,
    pub discharge_pressure_bar: f64,
    pub motor_current_a: f64,
    pub motor_voltage_v: f64,
    pub vibration_rms_mms: f64,
    pub bearing_temperature_c: f64,
    pub liquid_temperature_c: f64,
    pub liquid_temperature_source: String,
    pub speed_rpm: f64,
    pub status: i32,
    pub window_features: WindowFeatures,
    pub design_specs: DesignSpecs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthSummary {
    pub ok: bool,
    pub asset_id: String,
    pub source_bundle_issues: Vec<String>,
    pub runtime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DerivedFeatures {
    pub missing_inputs: Vec<String>,
    pub validation_errors: Vec<String>,
    pub suction_pressure_abs_kpa: Option<f64>,
    pub discharge_pressure_abs_kpa: Option<f64>,
    pub vapor_pressure_kpa: Option<f64>,
    pub npsha_m: Option<f64>,
    pub npshr_m: Option<f64>,
    pub npsh_margin_m: Option<f64>,
    pub npsh_margin_ratio: Option<f64>,
    pub npshr_method: String,
    pub pump_head_m: Option<f64>,
    pub pressure_delta_kpa: Option<f64>,
    pub npsh_score: f64,
    pub suction_score: f64,
    pub vibration_score: f64,
    pub current_score: f64,
    pub thermal_score: f64,
    pub flow_instability_score: f64,
    pub current_instability_score: f64,
    pub cavitation_evidence_score: f64,
    pub confidence: f64,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub asset_id: String,
    pub risk_level: RiskLevel,
    pub features: DerivedFeatures,
    pub belief: BeliefSummary,
    pub prediction: PredictionSummary,
    pub counterfactual_costs: BTreeMap<String, f64>,
    pub recommendation: Recommendation,
    pub recommended_action: String,
    pub recommended_checklist: Vec<String>,
    pub limitations: Vec<String>,
    pub explanation: Explanation,
    pub equations_used: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    pub recommended_action: String,
    pub action_confidence: f64,
    pub decision_reason: String,
    pub lowest_cost_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    pub source: String,
    pub text: String,
    pub guard_passed: bool,
    pub fallback_used: bool,
    pub prompt_hash: String,
    pub guard_reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeliefSummary {
    pub mean_damage: f64,
    pub damage_variance: f64,
    pub confidence: f64,
    pub belief_method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionSummary {
    pub rul_hours: f64,
    pub rul_basis: String,
    pub failure_probability_24h: f64,
    pub failure_probability_7d: f64,
    pub failure_probability_30d: f64,
    pub cvar95: f64,
    pub cvar_basis: String,
}
