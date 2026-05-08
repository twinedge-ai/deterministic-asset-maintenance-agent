use crate::input_client::{resolve_namespace_index, variable_node_id, NamespaceResolution};
use asset_agent_core::schemas::{DesignSpecs, ObservationSnapshot, WindowFeatures};
use chrono::Utc;
use opcua::{
    client::{ClientBuilder, IdentityToken},
    crypto::SecurityPolicy,
    types::{
        AttributeId, DataValue, MessageSecurityMode, NodeId, ReadValueId, StatusCode,
        TimestampsToReturn, VariableId, Variant,
    },
};
use serde::Serialize;
use std::{collections::BTreeMap, path::PathBuf, time::Duration};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct LiveOpcUaConfig {
    pub endpoint: String,
    pub namespace_uri: String,
    pub asset_id: String,
    pub window_seconds: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveSnapshotResult {
    pub snapshot: ObservationSnapshot,
    pub namespace: NamespaceResolution,
    pub nodes_read: Vec<String>,
}

#[derive(Debug, Error)]
pub enum LiveOpcUaError {
    #[error("OPC UA client setup failed: {0}")]
    ClientSetup(String),
    #[error("OPC UA connection failed: {0}")]
    Connection(String),
    #[error("OPC UA read failed: {0}")]
    Read(String),
    #[error("namespace {namespace_uri} was not found in server NamespaceArray")]
    NamespaceNotFound { namespace_uri: String },
    #[error("node {node_id} returned bad status {status:?}")]
    BadNodeStatus { node_id: String, status: StatusCode },
    #[error("node {node_id} returned a missing value")]
    MissingNodeValue { node_id: String },
    #[error("node {node_id} returned an unsupported value type")]
    UnsupportedValue { node_id: String },
    #[error("connection timed out")]
    Timeout,
}

pub async fn read_live_snapshot(
    config: &LiveOpcUaConfig,
) -> Result<LiveSnapshotResult, LiveOpcUaError> {
    let mut client = ClientBuilder::new()
        .application_name("assetpilot-live-client")
        .application_uri("urn:assetpilot:live-client")
        .pki_dir(live_client_pki_dir())
        .create_sample_keypair(true)
        .trust_server_certs(true)
        .session_retry_limit(1)
        .client()
        .map_err(|errors| LiveOpcUaError::ClientSetup(errors.join("; ")))?;

    let (session, event_loop) = client
        .connect_to_matching_endpoint(
            (
                config.endpoint.as_str(),
                SecurityPolicy::None.to_str(),
                MessageSecurityMode::None,
            ),
            IdentityToken::Anonymous,
        )
        .await
        .map_err(|error| LiveOpcUaError::Connection(error.to_string()))?;
    event_loop.spawn();

    let connected = tokio::time::timeout(Duration::from_secs(10), session.wait_for_connection())
        .await
        .map_err(|_| LiveOpcUaError::Timeout)?;
    if !connected {
        return Err(LiveOpcUaError::Connection(
            "session did not reach connected state".to_string(),
        ));
    }

    let namespace_uris = read_namespace_array(&session).await?;
    let namespace = resolve_namespace_index(&namespace_uris, &config.namespace_uri);
    let namespace_index =
        namespace
            .namespace_index
            .ok_or_else(|| LiveOpcUaError::NamespaceNotFound {
                namespace_uri: config.namespace_uri.clone(),
            })?;

    let plan = live_read_plan(&config.asset_id, namespace_index);
    let reads: Vec<ReadValueId> = plan
        .iter()
        .map(|item| read_value_id(&item.node_id))
        .collect();
    let values = session
        .read(&reads, TimestampsToReturn::Both, 0.0)
        .await
        .map_err(|error| LiveOpcUaError::Read(error.to_string()))?;

    let mut by_key = BTreeMap::new();
    for (item, value) in plan.iter().zip(values) {
        ensure_good_status(&item.node_id, &value)?;
        by_key.insert(item.key, (item.node_id.clone(), value));
    }

    let flow_rate_m3h = read_f64(&by_key, "flow_rate")?;
    let suction_pressure_bar = read_f64(&by_key, "suction_pressure")?;
    let discharge_pressure_bar = read_f64(&by_key, "discharge_pressure")?;
    let motor_current_a = read_f64(&by_key, "motor_current")?;
    let motor_voltage_v = read_f64(&by_key, "motor_voltage")?;
    let vibration_rms_mms = read_f64(&by_key, "vibration")?;
    let bearing_temperature_c = read_f64(&by_key, "bearing_temperature")?;
    let liquid_temperature_c = read_f64(&by_key, "raw_temperature")?;
    let speed_rpm = read_f64(&by_key, "speed_rpm")?;
    let status = read_i32(&by_key, "status")?;

    let timestamp = Utc::now().to_rfc3339();
    let snapshot = ObservationSnapshot {
        asset_id: config.asset_id.clone(),
        timestamp: timestamp.clone(),
        as_of_timestamp: timestamp,
        window_seconds: config.window_seconds,
        sample_count: 1,
        flow_rate_m3h,
        suction_pressure_bar,
        discharge_pressure_bar,
        motor_current_a,
        motor_voltage_v,
        vibration_rms_mms,
        bearing_temperature_c,
        liquid_temperature_c,
        liquid_temperature_source: "live_intake_temperature".to_string(),
        speed_rpm,
        status,
        window_features: WindowFeatures {
            flow_rate_std_m3h: 0.0,
            motor_current_std_a: 0.0,
            vibration_max_mms: vibration_rms_mms,
            suction_pressure_min_bar: suction_pressure_bar,
        },
        design_specs: DesignSpecs {
            rated_flow_m3h: read_f64(&by_key, "spec_rated_flow")?,
            rated_head_m: read_f64(&by_key, "spec_rated_head")?,
            rated_speed_rpm: read_f64(&by_key, "spec_rated_speed_rpm")?,
            npshr_m: read_f64(&by_key, "spec_npshr")?,
            efficiency_percent: read_f64(&by_key, "spec_efficiency")?,
            power_kw: read_f64(&by_key, "spec_power_kw")?,
            specific_gravity: read_f64(&by_key, "spec_specific_gravity")?,
            design_suction_pressure_barg: read_f64(&by_key, "spec_design_suction_pressure")?,
            max_discharge_pressure_barg: read_f64(&by_key, "spec_max_discharge_pressure")?,
            min_flow_m3h: read_f64(&by_key, "spec_min_flow")?,
            max_flow_m3h: read_f64(&by_key, "spec_max_flow")?,
            min_speed_rpm: read_f64(&by_key, "spec_min_speed_rpm")?,
            pump_weight_kg: read_f64(&by_key, "spec_pump_weight")?,
            baseplate_weight_kg: read_f64(&by_key, "spec_baseplate_weight")?,
            stages: read_u32(&by_key, "spec_stages")?,
        },
    };

    Ok(LiveSnapshotResult {
        snapshot,
        namespace,
        nodes_read: plan
            .into_iter()
            .map(|item| item.node_id.to_string())
            .collect(),
    })
}

#[derive(Debug, Clone)]
struct ReadPlanItem {
    key: &'static str,
    node_id: NodeId,
}

fn live_read_plan(asset_id: &str, namespace_index: u16) -> Vec<ReadPlanItem> {
    let mut plan = vec![
        asset_node(namespace_index, asset_id, "flow_rate"),
        asset_node(namespace_index, asset_id, "suction_pressure"),
        asset_node(namespace_index, asset_id, "discharge_pressure"),
        asset_node(namespace_index, asset_id, "motor_current"),
        asset_node(namespace_index, asset_id, "motor_voltage"),
        asset_node(namespace_index, asset_id, "vibration"),
        asset_node(namespace_index, asset_id, "bearing_temperature"),
        asset_node(namespace_index, asset_id, "speed_rpm"),
        asset_node(namespace_index, asset_id, "status"),
        ReadPlanItem {
            key: "raw_temperature",
            node_id: NodeId::new(
                namespace_index,
                variable_node_id("intake_basin", "raw_temperature"),
            ),
        },
    ];

    plan.extend([
        asset_node(namespace_index, asset_id, "spec_rated_flow"),
        asset_node(namespace_index, asset_id, "spec_rated_head"),
        asset_node(namespace_index, asset_id, "spec_rated_speed_rpm"),
        asset_node(namespace_index, asset_id, "spec_npshr"),
        asset_node(namespace_index, asset_id, "spec_efficiency"),
        asset_node(namespace_index, asset_id, "spec_power_kw"),
        asset_node(namespace_index, asset_id, "spec_specific_gravity"),
        asset_node(namespace_index, asset_id, "spec_design_suction_pressure"),
        asset_node(namespace_index, asset_id, "spec_max_discharge_pressure"),
        asset_node(namespace_index, asset_id, "spec_min_flow"),
        asset_node(namespace_index, asset_id, "spec_max_flow"),
        asset_node(namespace_index, asset_id, "spec_min_speed_rpm"),
        asset_node(namespace_index, asset_id, "spec_pump_weight"),
        asset_node(namespace_index, asset_id, "spec_baseplate_weight"),
        asset_node(namespace_index, asset_id, "spec_stages"),
    ]);

    plan
}

fn asset_node(namespace_index: u16, asset_id: &str, key: &'static str) -> ReadPlanItem {
    ReadPlanItem {
        key,
        node_id: NodeId::new(namespace_index, variable_node_id(asset_id, key)),
    }
}

async fn read_namespace_array(
    session: &opcua::client::Session,
) -> Result<Vec<String>, LiveOpcUaError> {
    let values = session
        .read(
            &[read_value_id(&VariableId::Server_NamespaceArray.into())],
            TimestampsToReturn::Neither,
            0.0,
        )
        .await
        .map_err(|error| LiveOpcUaError::Read(error.to_string()))?;
    let value = values
        .into_iter()
        .next()
        .ok_or_else(|| LiveOpcUaError::MissingNodeValue {
            node_id: "Server_NamespaceArray".to_string(),
        })?;
    ensure_good_status("Server_NamespaceArray", &value)?;
    match value.value {
        Some(Variant::Array(array)) => array
            .values
            .into_iter()
            .map(|variant| match variant {
                Variant::String(value) => Ok(value.as_ref().to_string()),
                _ => Err(LiveOpcUaError::UnsupportedValue {
                    node_id: "Server_NamespaceArray".to_string(),
                }),
            })
            .collect(),
        _ => Err(LiveOpcUaError::UnsupportedValue {
            node_id: "Server_NamespaceArray".to_string(),
        }),
    }
}

fn read_value_id(node_id: &NodeId) -> ReadValueId {
    ReadValueId {
        node_id: node_id.clone(),
        attribute_id: AttributeId::Value as u32,
        ..Default::default()
    }
}

fn ensure_good_status(node_id: impl ToString, value: &DataValue) -> Result<(), LiveOpcUaError> {
    let status = value.status();
    if status.is_bad() {
        return Err(LiveOpcUaError::BadNodeStatus {
            node_id: node_id.to_string(),
            status,
        });
    }
    Ok(())
}

fn read_f64(
    values: &BTreeMap<&'static str, (NodeId, DataValue)>,
    key: &'static str,
) -> Result<f64, LiveOpcUaError> {
    let (node_id, value) = value_for_key(values, key)?;
    match value.value.as_ref() {
        Some(Variant::Double(value)) => Ok(*value),
        Some(Variant::Float(value)) => Ok((*value).into()),
        Some(Variant::Int16(value)) => Ok((*value).into()),
        Some(Variant::Int32(value)) => Ok((*value).into()),
        Some(Variant::UInt16(value)) => Ok((*value).into()),
        Some(Variant::UInt32(value)) => Ok((*value).into()),
        Some(Variant::UInt64(value)) => Ok(*value as f64),
        _ => Err(LiveOpcUaError::UnsupportedValue {
            node_id: node_id.to_string(),
        }),
    }
}

fn read_i32(
    values: &BTreeMap<&'static str, (NodeId, DataValue)>,
    key: &'static str,
) -> Result<i32, LiveOpcUaError> {
    let (node_id, value) = value_for_key(values, key)?;
    match value.value.as_ref() {
        Some(Variant::Int16(value)) => Ok((*value).into()),
        Some(Variant::Int32(value)) => Ok(*value),
        Some(Variant::UInt16(value)) => Ok((*value).into()),
        Some(Variant::UInt32(value)) => {
            i32::try_from(*value).map_err(|_| LiveOpcUaError::UnsupportedValue {
                node_id: node_id.to_string(),
            })
        }
        _ => Err(LiveOpcUaError::UnsupportedValue {
            node_id: node_id.to_string(),
        }),
    }
}

fn read_u32(
    values: &BTreeMap<&'static str, (NodeId, DataValue)>,
    key: &'static str,
) -> Result<u32, LiveOpcUaError> {
    let (node_id, value) = value_for_key(values, key)?;
    match value.value.as_ref() {
        Some(Variant::Int16(value)) => {
            u32::try_from(*value).map_err(|_| LiveOpcUaError::UnsupportedValue {
                node_id: node_id.to_string(),
            })
        }
        Some(Variant::Int32(value)) => {
            u32::try_from(*value).map_err(|_| LiveOpcUaError::UnsupportedValue {
                node_id: node_id.to_string(),
            })
        }
        Some(Variant::UInt16(value)) => Ok((*value).into()),
        Some(Variant::UInt32(value)) => Ok(*value),
        _ => Err(LiveOpcUaError::UnsupportedValue {
            node_id: node_id.to_string(),
        }),
    }
}

fn value_for_key<'a>(
    values: &'a BTreeMap<&'static str, (NodeId, DataValue)>,
    key: &'static str,
) -> Result<&'a (NodeId, DataValue), LiveOpcUaError> {
    values
        .get(key)
        .ok_or_else(|| LiveOpcUaError::MissingNodeValue {
            node_id: key.to_string(),
        })
}

fn live_client_pki_dir() -> PathBuf {
    std::env::var("ASSET_AGENT_LIVE_CLIENT_PKI_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("asset-agent-opcua-live-client-pki"))
}
