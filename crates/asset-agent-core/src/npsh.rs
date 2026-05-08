pub fn water_vapor_pressure_kpa(temperature_c: f64) -> f64 {
    0.61078 * ((17.27 * temperature_c) / (temperature_c + 237.3)).exp()
}

pub fn gauge_bar_to_abs_kpa(gauge_bar: f64, atmospheric_pressure_bar: f64) -> f64 {
    (gauge_bar + atmospheric_pressure_bar) * 100.0
}

pub fn npsha_m(
    suction_pressure_bar: f64,
    liquid_temperature_c: f64,
    specific_gravity: f64,
    atmospheric_pressure_bar: f64,
) -> f64 {
    let suction_abs_kpa = gauge_bar_to_abs_kpa(suction_pressure_bar, atmospheric_pressure_bar);
    let vapor_kpa = water_vapor_pressure_kpa(liquid_temperature_c);
    ((suction_abs_kpa - vapor_kpa) * 1000.0) / (1000.0 * specific_gravity * 9.80665)
}

pub fn pump_head_m(
    suction_pressure_bar: f64,
    discharge_pressure_bar: f64,
    specific_gravity: f64,
    atmospheric_pressure_bar: f64,
) -> f64 {
    let suction_abs_kpa = gauge_bar_to_abs_kpa(suction_pressure_bar, atmospheric_pressure_bar);
    let discharge_abs_kpa = gauge_bar_to_abs_kpa(discharge_pressure_bar, atmospheric_pressure_bar);
    ((discharge_abs_kpa - suction_abs_kpa) * 1000.0) / (1000.0 * specific_gravity * 9.80665)
}

#[cfg(test)]
mod tests {
    use super::{gauge_bar_to_abs_kpa, npsha_m, pump_head_m, water_vapor_pressure_kpa};

    #[test]
    fn vapor_pressure_at_20c_is_plausible() {
        assert!((water_vapor_pressure_kpa(20.0) - 2.338).abs() < 0.01);
    }

    #[test]
    fn low_suction_npsha_is_below_source_npshr() {
        let npsha = npsha_m(0.7, 20.0, 1.03, 1.01325);
        assert!(npsha < 19.0);
    }

    #[test]
    fn gauge_pressure_converts_to_absolute_kpa() {
        assert!((gauge_bar_to_abs_kpa(0.7, 1.01325) - 171.325).abs() < 1e-9);
    }

    #[test]
    fn pump_head_uses_pressure_delta() {
        let head = pump_head_m(0.7, 63.5, 1.03, 1.01325);
        assert!((head - 621.729).abs() < 0.01);
    }
}
