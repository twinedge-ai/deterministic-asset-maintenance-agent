use crate::{
    config::ModelConfig,
    schemas::{BeliefSummary, DerivedFeatures},
};

#[derive(Debug, Clone)]
pub struct BeliefState {
    pub summary: BeliefSummary,
    pub damage_rate: f64,
    pub grid: Vec<f64>,
    pub posterior: Vec<f64>,
}

pub fn update_belief(features: &DerivedFeatures, config: &ModelConfig) -> BeliefState {
    let step = config_value(&config.belief, "grid_step", 0.01);
    let sigma = config_value(&config.belief, "observation_sigma", 0.18);
    let damage_rate = damage_rate(features, config);
    let grid = damage_grid(step);
    let prior = 1.0 / grid.len() as f64;
    let mut weights = Vec::with_capacity(grid.len());
    let mut sum = 0.0;

    for damage in &grid {
        let predicted = (damage + damage_rate).clamp(0.0, 1.0);
        let z = (features.cavitation_evidence_score - predicted) / sigma;
        let likelihood = (-0.5 * z * z).exp();
        let weight = likelihood * prior;
        weights.push(weight);
        sum += weight;
    }

    let posterior = if sum > 0.0 {
        weights.into_iter().map(|weight| weight / sum).collect()
    } else {
        vec![prior; grid.len()]
    };

    let mean = weighted_mean(&grid, &posterior);
    let variance = grid
        .iter()
        .zip(&posterior)
        .map(|(damage, probability)| probability * (damage - mean).powi(2))
        .sum::<f64>();
    let confidence = 1.0 - normalized_entropy(&posterior);

    BeliefState {
        summary: BeliefSummary {
            mean_damage: round6(mean),
            damage_variance: round6(variance),
            confidence: round6(confidence),
            belief_method: "fixed_grid".to_string(),
        },
        damage_rate: round12(damage_rate),
        grid,
        posterior,
    }
}

pub fn default_belief() -> BeliefSummary {
    BeliefSummary {
        mean_damage: 0.0,
        damage_variance: 0.0,
        confidence: 0.0,
        belief_method: "fixed_grid".to_string(),
    }
}

pub fn damage_rate(features: &DerivedFeatures, config: &ModelConfig) -> f64 {
    config_value(&config.belief, "base_damage_rate", 0.001)
        + config_value(&config.belief, "cavitation_damage_gain", 0.010)
            * features.cavitation_evidence_score
        + config_value(&config.belief, "vibration_damage_gain", 0.004) * features.vibration_score
        + config_value(&config.belief, "thermal_damage_gain", 0.002) * features.thermal_score
        + config_value(&config.belief, "learned_residual_bias", 0.0)
}

fn damage_grid(step: f64) -> Vec<f64> {
    let mut grid = Vec::new();
    let mut value = 0.0;
    while value < 1.0 {
        grid.push(round12(value));
        value += step;
    }
    grid.push(1.0);
    grid
}

fn weighted_mean(values: &[f64], weights: &[f64]) -> f64 {
    values
        .iter()
        .zip(weights)
        .map(|(value, weight)| value * weight)
        .sum()
}

fn normalized_entropy(probabilities: &[f64]) -> f64 {
    let n = probabilities.len();
    if n <= 1 {
        return 0.0;
    }
    let entropy = probabilities
        .iter()
        .filter(|probability| **probability > 0.0)
        .map(|probability| -probability * probability.ln())
        .sum::<f64>();
    entropy / (n as f64).ln()
}

fn config_value(value: &serde_json::Value, key: &str, default: f64) -> f64 {
    value
        .get(key)
        .and_then(|value| value.as_f64())
        .unwrap_or(default)
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

fn round12(value: f64) -> f64 {
    (value * 1_000_000_000_000.0).round() / 1_000_000_000_000.0
}

#[cfg(test)]
mod tests {
    use super::update_belief;
    use crate::{config::load_model_config, features::build_features, replay::read_trace};
    use std::path::Path;

    #[test]
    fn belief_grid_is_fixed_and_normalized() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("config loads");
        let snapshots = read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl"))
            .expect("trace loads");
        let features = build_features(&snapshots[0], &config);
        let belief = update_belief(&features, &config);
        assert_eq!(belief.grid.len(), 101);
        assert!((belief.posterior.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        assert!(belief.summary.mean_damage > 0.0);
    }
}
