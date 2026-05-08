use crate::{belief::BeliefState, config::ModelConfig, forecast::failure_probability_steps};
use std::collections::BTreeMap;

const ACTIONS: &[&str] = &["defer", "inspect", "reduce_load", "repair", "replace"];

#[derive(Debug, Clone)]
pub struct CounterfactualResult {
    pub costs: BTreeMap<String, f64>,
    pub cvar95: f64,
}

pub fn evaluate_counterfactuals(
    belief: &BeliefState,
    config: &ModelConfig,
) -> CounterfactualResult {
    let tick_hours = config_value(&config.belief, "tick_hours", 1.0);
    let horizon_steps = ((24.0 * 7.0) / tick_hours).round().max(1.0);
    let mean_damage = belief.summary.mean_damage;
    let mut costs = BTreeMap::new();

    for action in ACTIONS {
        let (damage_scale, damage_reset, rate_scale, mean_override) =
            action_effect(action, mean_damage);
        let adjusted_grid: Vec<f64> = belief
            .grid
            .iter()
            .map(|damage| damage_reset.unwrap_or((*damage * damage_scale).clamp(0.0, 1.0)))
            .collect();
        let adjusted_rate = belief.damage_rate * rate_scale;
        let adjusted_mean = mean_override.unwrap_or((mean_damage * damage_scale).clamp(0.0, 1.0));
        let pf_7d = failure_probability_steps(
            &adjusted_grid,
            &belief.posterior,
            adjusted_rate,
            horizon_steps,
        );
        let total = maintenance_cost(config, action)
            + downtime_cost(config, action)
            + config_value(&config.costs, "operating_penalty_per_damage", 12_000.0) * adjusted_mean
            + config_value(&config.costs, "failure_penalty", 100_000.0) * pf_7d;
        costs.insert((*action).to_string(), round6(total));
    }

    CounterfactualResult {
        cvar95: round6(cvar95_defer(belief, config, horizon_steps)),
        costs,
    }
}

pub fn default_counterfactual_costs() -> BTreeMap<String, f64> {
    ACTIONS
        .iter()
        .map(|action| ((*action).to_string(), 0.0))
        .collect()
}

fn action_effect(action: &str, mean_damage: f64) -> (f64, Option<f64>, f64, Option<f64>) {
    match action {
        "inspect" => (1.0, None, 1.0, Some(mean_damage)),
        "reduce_load" => (1.0, None, 0.65, Some(mean_damage)),
        "repair" => (0.55, None, 1.0, None),
        "replace" => (1.0, Some(0.05), 1.0, Some(0.05)),
        _ => (1.0, None, 1.0, Some(mean_damage)),
    }
}

fn cvar95_defer(belief: &BeliefState, config: &ModelConfig, horizon_steps: f64) -> f64 {
    let operating_penalty = config_value(&config.costs, "operating_penalty_per_damage", 12_000.0);
    let failure_penalty = config_value(&config.costs, "failure_penalty", 100_000.0);
    let fixed_cost = maintenance_cost(config, "defer") + downtime_cost(config, "defer");
    let mut state_costs: Vec<(f64, f64)> = belief
        .grid
        .iter()
        .zip(&belief.posterior)
        .map(|(damage, probability)| {
            let failed = (*damage + horizon_steps * belief.damage_rate >= 1.0) as u8 as f64;
            (
                fixed_cost + operating_penalty * damage + failure_penalty * failed,
                *probability,
            )
        })
        .collect();
    state_costs.sort_by(|a, b| b.0.total_cmp(&a.0));

    let mut remaining_tail_mass = 0.05;
    let mut weighted_sum = 0.0;
    let mut used_mass = 0.0;
    for (cost, probability) in state_costs {
        if remaining_tail_mass <= 0.0 {
            break;
        }
        let take = probability.min(remaining_tail_mass);
        weighted_sum += cost * take;
        used_mass += take;
        remaining_tail_mass -= take;
    }
    if used_mass > 0.0 {
        weighted_sum / used_mass
    } else {
        0.0
    }
}

fn maintenance_cost(config: &ModelConfig, action: &str) -> f64 {
    nested_config_value(&config.costs, "maintenance_cost", action, 0.0)
}

fn downtime_cost(config: &ModelConfig, action: &str) -> f64 {
    nested_config_value(&config.costs, "downtime_cost", action, 0.0)
}

fn config_value(value: &serde_json::Value, key: &str, default: f64) -> f64 {
    value
        .get(key)
        .and_then(|value| value.as_f64())
        .unwrap_or(default)
}

fn nested_config_value(value: &serde_json::Value, section: &str, key: &str, default: f64) -> f64 {
    value
        .get(section)
        .and_then(|section| section.get(key))
        .and_then(|value| value.as_f64())
        .unwrap_or(default)
}

fn round6(value: f64) -> f64 {
    (value * 1_000_000.0).round() / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::evaluate_counterfactuals;
    use crate::{
        belief::update_belief, config::load_model_config, features::build_features,
        replay::read_trace,
    };
    use std::path::Path;

    #[test]
    fn counterfactual_costs_include_all_actions() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("config loads");
        let snapshots = read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl"))
            .expect("trace loads");
        let features = build_features(&snapshots[0], &config);
        let belief = update_belief(&features, &config);
        let result = evaluate_counterfactuals(&belief, &config);
        for action in ["defer", "inspect", "reduce_load", "repair", "replace"] {
            assert!(result.costs.contains_key(action));
        }
        assert!(result.cvar95 > 0.0);
    }
}
