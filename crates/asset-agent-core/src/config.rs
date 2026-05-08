use crate::schemas::{DesignSpecs, ObservationSnapshot};
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse TOML {path}: {source}")]
    Toml {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("failed to parse JSON {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetConfig {
    pub asset_id: String,
    pub display_name: String,
    pub asset_type: String,
    pub parent_area: String,
    pub fault_focus: String,
    pub source_specs_path: String,
    pub pump_curve_path: String,
    pub manufacturer: String,
    pub pump_model: String,
    pub service: String,
    pub source_document: String,
    pub source_url: String,
    pub preferred_liquid_temperature_node: String,
    pub fallback_liquid_temperature_c: f64,
    pub temperature_plausible_range_c: Vec<f64>,
    pub temperature_policy: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManufacturerSpecConfig {
    pub expected_design_specs: ExpectedDesignSpecs,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExpectedDesignSpecs {
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

#[derive(Debug, Clone)]
pub struct ModelConfig {
    pub project_root: PathBuf,
    pub asset: AssetConfig,
    pub manufacturer_spec: ManufacturerSpecConfig,
    pub npsh: Value,
    pub policy: Value,
    pub costs: Value,
    pub belief: Value,
    pub learning: Value,
    pub versions: Value,
    pub source_specs: Value,
    pub pump_curve: Value,
}

pub fn load_model_config(
    project_root: impl AsRef<Path>,
    asset_id: &str,
) -> Result<ModelConfig, ConfigError> {
    let root = project_root.as_ref().to_path_buf();
    let model_dir = root.join("models").join(asset_id);
    let asset: AssetConfig = read_toml(&model_dir.join("asset.toml"))?;
    let manufacturer_spec = read_toml(&model_dir.join("manufacturer_spec.toml"))?;
    let source_specs = read_json(&root.join(&asset.source_specs_path))?;
    let pump_curve = read_json(&root.join(&asset.pump_curve_path))?;

    Ok(ModelConfig {
        project_root: root,
        asset,
        manufacturer_spec,
        npsh: read_toml_value(&model_dir.join("npsh.toml"))?,
        policy: read_toml_value(&model_dir.join("policy.toml"))?,
        costs: read_toml_value(&model_dir.join("costs.toml"))?,
        belief: read_toml_value(&model_dir.join("belief.toml"))?,
        learning: read_toml_value(&model_dir.join("learning.toml"))?,
        versions: read_toml_value(&model_dir.join("versions.toml"))?,
        source_specs,
        pump_curve,
    })
}

pub fn validate_source_bundle(config: &ModelConfig) -> Vec<String> {
    let mut issues = Vec::new();
    if config.asset.asset_id != "hp_pump_1" {
        issues.push(format!("unexpected asset id {}", config.asset.asset_id));
    }
    if config.source_specs.get("asset_id").and_then(Value::as_str)
        != Some(config.asset.asset_id.as_str())
    {
        issues.push("source spec asset_id does not match asset config".to_string());
    }
    if config.pump_curve.get("asset_id").and_then(Value::as_str)
        != Some(config.asset.asset_id.as_str())
    {
        issues.push("pump curve asset_id does not match asset config".to_string());
    }
    if config
        .pump_curve
        .get("curve_policy")
        .and_then(Value::as_str)
        != Some("single_source_design_point_no_fabricated_interpolation")
    {
        issues.push(
            "pump curve policy is not the expected single-source design-point policy".to_string(),
        );
    }
    if config
        .source_specs
        .get("demo_data_policy")
        .and_then(Value::as_str)
        != Some("synthetic_demo_profile_not_for_operational_use")
    {
        issues.push(
            "source spec policy must mark the public profile as synthetic demo data".to_string(),
        );
    }
    let specs = &config.manufacturer_spec.expected_design_specs;
    if (specs.npshr_m - 19.0).abs() > f64::EPSILON {
        issues.push("expected NPSHr must be 19.0 m for this demo asset".to_string());
    }
    if (specs.specific_gravity - 1.03).abs() > f64::EPSILON {
        issues.push("expected specific gravity must be 1.03 for this demo asset".to_string());
    }
    issues
}

pub fn validate_snapshot_design_specs(
    snapshot: &ObservationSnapshot,
    config: &ModelConfig,
) -> Vec<String> {
    compare_design_specs(
        &snapshot.design_specs,
        &config.manufacturer_spec.expected_design_specs,
    )
}

fn compare_design_specs(observed: &DesignSpecs, expected: &ExpectedDesignSpecs) -> Vec<String> {
    let mut errors = Vec::new();
    compare_f64(
        &mut errors,
        "rated_flow_m3h",
        observed.rated_flow_m3h,
        expected.rated_flow_m3h,
    );
    compare_f64(
        &mut errors,
        "rated_head_m",
        observed.rated_head_m,
        expected.rated_head_m,
    );
    compare_f64(
        &mut errors,
        "rated_speed_rpm",
        observed.rated_speed_rpm,
        expected.rated_speed_rpm,
    );
    compare_f64(&mut errors, "npshr_m", observed.npshr_m, expected.npshr_m);
    compare_f64(
        &mut errors,
        "efficiency_percent",
        observed.efficiency_percent,
        expected.efficiency_percent,
    );
    compare_f64(
        &mut errors,
        "power_kw",
        observed.power_kw,
        expected.power_kw,
    );
    compare_f64(
        &mut errors,
        "specific_gravity",
        observed.specific_gravity,
        expected.specific_gravity,
    );
    compare_f64(
        &mut errors,
        "design_suction_pressure_barg",
        observed.design_suction_pressure_barg,
        expected.design_suction_pressure_barg,
    );
    compare_f64(
        &mut errors,
        "max_discharge_pressure_barg",
        observed.max_discharge_pressure_barg,
        expected.max_discharge_pressure_barg,
    );
    compare_f64(
        &mut errors,
        "min_flow_m3h",
        observed.min_flow_m3h,
        expected.min_flow_m3h,
    );
    compare_f64(
        &mut errors,
        "max_flow_m3h",
        observed.max_flow_m3h,
        expected.max_flow_m3h,
    );
    compare_f64(
        &mut errors,
        "min_speed_rpm",
        observed.min_speed_rpm,
        expected.min_speed_rpm,
    );
    compare_f64(
        &mut errors,
        "pump_weight_kg",
        observed.pump_weight_kg,
        expected.pump_weight_kg,
    );
    compare_f64(
        &mut errors,
        "baseplate_weight_kg",
        observed.baseplate_weight_kg,
        expected.baseplate_weight_kg,
    );
    if observed.stages != expected.stages {
        errors.push(format!(
            "design spec stages expected {}, got {}",
            expected.stages, observed.stages
        ));
    }
    errors
}

fn compare_f64(errors: &mut Vec<String>, name: &str, actual: f64, expected: f64) {
    if (actual - expected).abs() > 1e-6 {
        errors.push(format!(
            "design spec {name} expected {expected}, got {actual}"
        ));
    }
}

fn read_to_string(path: &Path) -> Result<String, ConfigError> {
    fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, ConfigError> {
    let text = read_to_string(path)?;
    toml::from_str(&text).map_err(|source| ConfigError::Toml {
        path: path.to_path_buf(),
        source,
    })
}

fn read_toml_value(path: &Path) -> Result<Value, ConfigError> {
    let text = read_to_string(path)?;
    let toml_value: toml::Value = toml::from_str(&text).map_err(|source| ConfigError::Toml {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::to_value(toml_value).map_err(|source| ConfigError::Json {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json(path: &Path) -> Result<Value, ConfigError> {
    let text = read_to_string(path)?;
    serde_json::from_str(&text).map_err(|source| ConfigError::Json {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::{load_model_config, validate_snapshot_design_specs, validate_source_bundle};
    use crate::replay::read_trace;
    use std::path::Path;

    #[test]
    fn loads_hp_pump_1_config() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(root, "hp_pump_1").expect("config should load");
        assert_eq!(config.asset.asset_id, "hp_pump_1");
        assert_eq!(config.manufacturer_spec.expected_design_specs.npshr_m, 19.0);
        assert!(validate_source_bundle(&config).is_empty());
    }

    #[test]
    fn validates_replay_snapshot_design_specs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("config should load");
        let snapshots = read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl"))
            .expect("trace should load");
        assert!(validate_snapshot_design_specs(&snapshots[0], &config).is_empty());
    }
}
