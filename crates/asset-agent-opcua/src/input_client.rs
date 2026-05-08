#[derive(Debug, Clone)]
pub struct OpcUaInputConfig {
    pub endpoint: String,
    pub namespace_uri: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeBinding {
    pub signal: &'static str,
    pub node_id: String,
    pub required: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct NamespaceResolution {
    pub namespace_uri: String,
    pub namespace_index: Option<u16>,
    pub resolved: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OpcInputContract {
    pub endpoint: String,
    pub namespace: NamespaceResolution,
    pub asset_id: String,
    pub operational_nodes: Vec<NodeBinding>,
    pub design_spec_nodes: Vec<NodeBinding>,
    pub temperature_node: NodeBinding,
    pub command_nodes_blocked: Vec<String>,
}

pub const OPERATIONAL_SIGNALS: &[&str] = &[
    "flow_rate",
    "suction_pressure",
    "discharge_pressure",
    "motor_current",
    "motor_voltage",
    "vibration",
    "bearing_temperature",
    "speed_rpm",
    "status",
];

pub const DESIGN_SPEC_SIGNALS: &[&str] = &[
    "spec_rated_flow",
    "spec_rated_head",
    "spec_rated_speed_rpm",
    "spec_npshr",
    "spec_efficiency",
    "spec_power_kw",
    "spec_specific_gravity",
    "spec_design_suction_pressure",
    "spec_max_discharge_pressure",
    "spec_min_flow",
    "spec_max_flow",
    "spec_min_speed_rpm",
    "spec_pump_weight",
    "spec_baseplate_weight",
    "spec_stages",
];

pub fn variable_node_id(asset_id: &str, signal: &str) -> String {
    format!("node:{asset_id}.{signal}")
}

pub fn build_input_contract(config: &OpcUaInputConfig, asset_id: &str) -> OpcInputContract {
    OpcInputContract {
        endpoint: config.endpoint.clone(),
        namespace: NamespaceResolution {
            namespace_uri: config.namespace_uri.clone(),
            namespace_index: None,
            resolved: false,
        },
        asset_id: asset_id.to_string(),
        operational_nodes: OPERATIONAL_SIGNALS
            .iter()
            .map(|signal| NodeBinding {
                signal,
                node_id: variable_node_id(asset_id, signal),
                required: true,
            })
            .collect(),
        design_spec_nodes: DESIGN_SPEC_SIGNALS
            .iter()
            .map(|signal| NodeBinding {
                signal,
                node_id: variable_node_id(asset_id, signal),
                required: true,
            })
            .collect(),
        temperature_node: NodeBinding {
            signal: "raw_temperature",
            node_id: "node:intake_basin.raw_temperature".to_string(),
            required: false,
        },
        command_nodes_blocked: vec![
            variable_node_id(asset_id, "command_start"),
            variable_node_id(asset_id, "command_stop"),
        ],
    }
}

pub fn resolve_namespace_index(
    namespace_uris: &[String],
    namespace_uri: &str,
) -> NamespaceResolution {
    let namespace_index = namespace_uris
        .iter()
        .position(|candidate| candidate == namespace_uri)
        .map(|index| index as u16);
    NamespaceResolution {
        namespace_uri: namespace_uri.to_string(),
        namespace_index,
        resolved: namespace_index.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_input_contract, resolve_namespace_index, variable_node_id, OpcUaInputConfig,
    };

    #[test]
    fn builds_stable_string_node_ids() {
        assert_eq!(
            variable_node_id("hp_pump_1", "suction_pressure"),
            "node:hp_pump_1.suction_pressure"
        );
    }

    #[test]
    fn input_contract_contains_specs_and_temperature() {
        let contract = build_input_contract(
            &OpcUaInputConfig {
                endpoint: "opc.tcp://127.0.0.1:4840".to_string(),
                namespace_uri: "urn:twinedge:opcua-edge".to_string(),
            },
            "hp_pump_1",
        );
        assert_eq!(contract.operational_nodes.len(), 9);
        assert_eq!(contract.design_spec_nodes.len(), 15);
        assert_eq!(
            contract.temperature_node.node_id,
            "node:intake_basin.raw_temperature"
        );
    }

    #[test]
    fn resolves_namespace_uri_by_position() {
        let namespaces = vec![
            "http://opcfoundation.org/UA/".to_string(),
            "urn:twinedge:opcua-edge".to_string(),
        ];
        let resolved = resolve_namespace_index(&namespaces, "urn:twinedge:opcua-edge");
        assert!(resolved.resolved);
        assert_eq!(resolved.namespace_index, Some(1));
    }
}
