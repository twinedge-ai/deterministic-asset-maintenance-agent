use crate::{belief::BeliefState, config::ModelConfig, schemas::PredictionSummary};

#[derive(Debug, Clone)]
pub struct ForecastMetrics {
    pub rul_hours: f64,
    pub failure_probability_24h: f64,
    pub failure_probability_7d: f64,
    pub failure_probability_30d: f64,
}

pub fn forecast(belief: &BeliefState, config: &ModelConfig) -> ForecastMetrics {
    let tick_hours = config_value(&config.belief, "tick_hours", 1.0);
    ForecastMetrics {
        rul_hours: round6(expected_rul_hours(
            &belief.grid,
            &belief.posterior,
            belief.damage_rate,
            tick_hours,
        )),
        failure_probability_24h: round6(failure_probability_hours(
            &belief.grid,
            &belief.posterior,
            belief.damage_rate,
            24.0,
            tick_hours,
        )),
        failure_probability_7d: round6(failure_probability_hours(
            &belief.grid,
            &belief.posterior,
            belief.damage_rate,
            24.0 * 7.0,
            tick_hours,
        )),
        failure_probability_30d: round6(failure_probability_hours(
            &belief.grid,
            &belief.posterior,
            belief.damage_rate,
            24.0 * 30.0,
            tick_hours,
        )),
    }
}

pub fn prediction_summary(metrics: ForecastMetrics, cvar95: f64) -> PredictionSummary {
    PredictionSummary {
        rul_hours: metrics.rul_hours,
        rul_basis: "deterministic_model_threshold".to_string(),
        failure_probability_24h: metrics.failure_probability_24h,
        failure_probability_7d: metrics.failure_probability_7d,
        failure_probability_30d: metrics.failure_probability_30d,
        cvar95: round6(cvar95),
        cvar_basis: "configured_demo_cost_distribution".to_string(),
    }
}

pub fn default_prediction() -> PredictionSummary {
    PredictionSummary {
        rul_hours: 0.0,
        rul_basis: "deterministic_model_threshold".to_string(),
        failure_probability_24h: 0.0,
        failure_probability_7d: 0.0,
        failure_probability_30d: 0.0,
        cvar95: 0.0,
        cvar_basis: "configured_demo_cost_distribution".to_string(),
    }
}

pub fn failure_probability_steps(
    grid: &[f64],
    posterior: &[f64],
    damage_rate: f64,
    steps: f64,
) -> f64 {
    grid.iter()
        .zip(posterior)
        .filter(|(damage, _)| **damage + steps * damage_rate >= 1.0)
        .map(|(_, probability)| probability)
        .sum()
}

fn failure_probability_hours(
    grid: &[f64],
    posterior: &[f64],
    damage_rate: f64,
    horizon_hours: f64,
    tick_hours: f64,
) -> f64 {
    let steps = (horizon_hours / tick_hours).round().max(1.0);
    failure_probability_steps(grid, posterior, damage_rate, steps)
}

fn expected_rul_hours(grid: &[f64], posterior: &[f64], damage_rate: f64, tick_hours: f64) -> f64 {
    if damage_rate <= 0.0 {
        return 1_000_000.0;
    }
    grid.iter()
        .zip(posterior)
        .map(|(damage, probability)| {
            let steps = ((1.0 - damage).max(0.0) / damage_rate).ceil();
            probability * steps * tick_hours
        })
        .sum()
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

#[cfg(test)]
mod tests {
    use super::forecast;
    use crate::{
        belief::update_belief, config::load_model_config, features::build_features,
        replay::read_trace,
    };
    use std::path::Path;

    #[test]
    fn forecast_returns_ordered_horizon_probabilities() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("config loads");
        let snapshots = read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl"))
            .expect("trace loads");
        let features = build_features(&snapshots[0], &config);
        let belief = update_belief(&features, &config);
        let metrics = forecast(&belief, &config);
        assert!(metrics.rul_hours > 0.0);
        assert!(metrics.failure_probability_24h <= metrics.failure_probability_7d);
        assert!(metrics.failure_probability_7d <= metrics.failure_probability_30d);
    }
}
