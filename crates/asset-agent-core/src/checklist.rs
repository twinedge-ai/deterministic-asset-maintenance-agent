#[derive(Debug, Clone, Copy)]
pub struct ChecklistDefinition {
    pub item_id: &'static str,
    pub item_text: &'static str,
}

pub const DEFAULT_CHECKLIST_ITEMS: &[ChecklistDefinition] = &[
    ChecklistDefinition {
        item_id: "check_suction_strainer",
        item_text: "Check suction strainer for blockage.",
    },
    ChecklistDefinition {
        item_id: "check_inlet_valve_position",
        item_text: "Check inlet valve position.",
    },
    ChecklistDefinition {
        item_id: "check_suction_pipe_restriction",
        item_text: "Check suction pipe restriction or blockage.",
    },
    ChecklistDefinition {
        item_id: "check_air_ingress",
        item_text: "Check for air ingress on the suction side.",
    },
    ChecklistDefinition {
        item_id: "check_liquid_temperature",
        item_text: "Check liquid temperature.",
    },
    ChecklistDefinition {
        item_id: "verify_npsha_gt_npshr",
        item_text: "Verify NPSHA is greater than NPSHR from the pump curve.",
    },
    ChecklistDefinition {
        item_id: "check_flow_range",
        item_text: "Check whether the pump is operating too far from its intended flow range.",
    },
    ChecklistDefinition {
        item_id: "check_upstream_filters",
        item_text: "Check clogged filters upstream.",
    },
    ChecklistDefinition {
        item_id: "check_suction_tank_level",
        item_text: "Check suction tank level.",
    },
    ChecklistDefinition {
        item_id: "check_pump_speed_change",
        item_text: "Check whether pump speed changed.",
    },
];

pub const DEFAULT_CHECKLIST: &[&str] = &[
    "Check suction strainer for blockage.",
    "Check inlet valve position.",
    "Check suction pipe restriction or blockage.",
    "Check for air ingress on the suction side.",
    "Check liquid temperature.",
    "Verify NPSHA is greater than NPSHR from the pump curve.",
    "Check whether the pump is operating too far from its intended flow range.",
    "Check clogged filters upstream.",
    "Check suction tank level.",
    "Check whether pump speed changed.",
];

pub fn recommended_checklist(limit: usize) -> Vec<String> {
    DEFAULT_CHECKLIST
        .iter()
        .take(limit)
        .map(|item| item.to_string())
        .collect()
}

pub fn checklist_item_text(item_id: &str) -> Option<&'static str> {
    DEFAULT_CHECKLIST_ITEMS
        .iter()
        .find(|item| item.item_id == item_id)
        .map(|item| item.item_text)
}

pub fn checklist_item_order(item_id: &str) -> usize {
    DEFAULT_CHECKLIST_ITEMS
        .iter()
        .position(|item| item.item_id == item_id)
        .unwrap_or(usize::MAX)
}
