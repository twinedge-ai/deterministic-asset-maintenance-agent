use asset_agent_core::schemas::AnalysisResult;
use async_trait::async_trait;
use chrono::Utc;
use opcua::{
    crypto::SecurityPolicy,
    server::{
        address_space::{
            add_namespaces, AccessLevel, AddressSpace, ObjectBuilder, VariableBuilder,
        },
        node_manager::{
            memory::{
                InMemoryNodeManagerBuilder, InMemoryNodeManagerImpl, InMemoryNodeManagerImplBuilder,
            },
            NodeManagerBuilder, ParsedReadValueId, RequestContext, ServerContext,
        },
        ServerBuilder, ServerHandle, ANONYMOUS_USER_TOKEN_ID,
    },
    types::{
        AttributeId, DataTypeId, DataValue, DateTime, Identifier, MessageSecurityMode, NodeId,
        ObjectId, ReferenceTypeId, StatusCode, TimestampsToReturn, VariableTypeId, Variant,
    },
};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeMap,
    net::{SocketAddr, ToSocketAddrs},
    path::PathBuf,
    sync::{Arc, RwLock},
};
use thiserror::Error;
use tokio::task::JoinHandle;

const DEFAULT_NAMESPACE_URI: &str = "urn:assetpilot:outputs";

#[derive(Debug, Clone, Serialize)]
pub struct AgentOutputConfig {
    pub endpoint: String,
    pub namespace_uri: String,
}

impl AgentOutputConfig {
    pub fn new(endpoint: impl Into<String>, namespace_uri: impl Into<String>) -> Self {
        Self {
            endpoint: endpoint.into(),
            namespace_uri: namespace_uri.into(),
        }
    }
}

impl Default for AgentOutputConfig {
    fn default() -> Self {
        Self {
            endpoint: "opc.tcp://127.0.0.1:4841".to_string(),
            namespace_uri: DEFAULT_NAMESPACE_URI.to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOutputDataType {
    Float,
    Text,
    UInt,
    Bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AgentOutputValue {
    Float(f64),
    Text(String),
    UInt(u64),
    Bool(bool),
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentOutputNode {
    pub signal: &'static str,
    pub node_id: String,
    pub data_type: AgentOutputDataType,
    pub value: AgentOutputValue,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentOutputSnapshot {
    pub endpoint: String,
    pub namespace_uri: String,
    pub asset_id: String,
    pub initialized: bool,
    pub sequence: u64,
    pub updated_at: Option<String>,
    pub source_case_id: Option<String>,
    pub node_count: usize,
    pub nodes: BTreeMap<String, AgentOutputNode>,
}

#[derive(Debug, Clone)]
pub struct AgentOutputState {
    config: AgentOutputConfig,
    asset_id: String,
    inner: Arc<RwLock<AgentOutputSnapshot>>,
}

#[derive(Clone)]
pub struct AgentOutputServerRuntime {
    pub endpoint: String,
    pub namespace_uri: String,
    pub handle: ServerHandle,
}

#[derive(Debug, Error)]
pub enum AgentOutputError {
    #[error("invalid OPC UA output endpoint {endpoint}")]
    InvalidEndpoint { endpoint: String },
    #[error("failed to bind OPC UA output endpoint {endpoint}: {source}")]
    Bind {
        endpoint: String,
        source: std::io::Error,
    },
    #[error("failed to build OPC UA output server: {0}")]
    Build(String),
    #[error("failed to read output state")]
    StatePoisoned,
}

#[derive(Debug, Clone, Copy)]
struct AgentOutputNodeSpec {
    signal: &'static str,
    suffix: &'static str,
    data_type: AgentOutputDataType,
}

const OUTPUT_NODE_SPECS: &[AgentOutputNodeSpec] = &[
    AgentOutputNodeSpec {
        signal: "risk_level",
        suffix: "health.risk_level",
        data_type: AgentOutputDataType::Text,
    },
    AgentOutputNodeSpec {
        signal: "cavitation_evidence_score",
        suffix: "health.cavitation_evidence_score",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "npsh_margin_m",
        suffix: "health.npsh_margin_m",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "npsha_m",
        suffix: "health.npsha_m",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "npshr_m",
        suffix: "health.npshr_m",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "mean_damage",
        suffix: "belief.mean_damage",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "damage_variance",
        suffix: "belief.damage_variance",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "rul_hours",
        suffix: "prediction.rul_hours",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "failure_probability_24h",
        suffix: "risk.failure_probability_24h",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "failure_probability_7d",
        suffix: "risk.failure_probability_7d",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "failure_probability_30d",
        suffix: "risk.failure_probability_30d",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "cvar95",
        suffix: "risk.cvar95",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "defer_cost",
        suffix: "counterfactual.defer_cost",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "inspect_cost",
        suffix: "counterfactual.inspect_cost",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "reduce_load_cost",
        suffix: "counterfactual.reduce_load_cost",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "repair_cost",
        suffix: "counterfactual.repair_cost",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "replace_cost",
        suffix: "counterfactual.replace_cost",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "recommended_action",
        suffix: "decision.recommended_action",
        data_type: AgentOutputDataType::Text,
    },
    AgentOutputNodeSpec {
        signal: "action_confidence",
        suffix: "decision.action_confidence",
        data_type: AgentOutputDataType::Float,
    },
    AgentOutputNodeSpec {
        signal: "decision_reason",
        suffix: "decision.decision_reason",
        data_type: AgentOutputDataType::Text,
    },
    AgentOutputNodeSpec {
        signal: "policy_version",
        suffix: "model.policy_version",
        data_type: AgentOutputDataType::Text,
    },
    AgentOutputNodeSpec {
        signal: "residual_version",
        suffix: "model.residual_version",
        data_type: AgentOutputDataType::Text,
    },
    AgentOutputNodeSpec {
        signal: "publish_sequence",
        suffix: "output.sequence",
        data_type: AgentOutputDataType::UInt,
    },
    AgentOutputNodeSpec {
        signal: "output_initialized",
        suffix: "output.initialized",
        data_type: AgentOutputDataType::Bool,
    },
];

impl AgentOutputState {
    pub fn new(config: AgentOutputConfig, asset_id: impl Into<String>) -> Self {
        let asset_id = asset_id.into();
        let snapshot = empty_snapshot(&config, &asset_id);
        Self {
            config,
            asset_id,
            inner: Arc::new(RwLock::new(snapshot)),
        }
    }

    pub fn config(&self) -> &AgentOutputConfig {
        &self.config
    }

    pub fn asset_id(&self) -> &str {
        &self.asset_id
    }

    pub fn snapshot(&self) -> Result<AgentOutputSnapshot, AgentOutputError> {
        self.inner
            .read()
            .map(|guard| guard.clone())
            .map_err(|_| AgentOutputError::StatePoisoned)
    }

    pub fn publish_analysis(
        &self,
        case_id: Option<&str>,
        analysis: &AnalysisResult,
        versions: &Value,
    ) -> Result<AgentOutputSnapshot, AgentOutputError> {
        let mut next = self
            .inner
            .write()
            .map_err(|_| AgentOutputError::StatePoisoned)?;
        let sequence = next.sequence.saturating_add(1);
        let mut nodes = build_default_nodes(&self.asset_id);

        set_node(
            &mut nodes,
            &self.asset_id,
            "health.risk_level",
            AgentOutputValue::Text(format!("{:?}", analysis.risk_level).to_uppercase()),
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "health.cavitation_evidence_score",
            analysis.features.cavitation_evidence_score,
        );
        set_float_option(
            &mut nodes,
            &self.asset_id,
            "health.npsh_margin_m",
            analysis.features.npsh_margin_m,
        );
        set_float_option(
            &mut nodes,
            &self.asset_id,
            "health.npsha_m",
            analysis.features.npsha_m,
        );
        set_float_option(
            &mut nodes,
            &self.asset_id,
            "health.npshr_m",
            analysis.features.npshr_m,
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "belief.mean_damage",
            analysis.belief.mean_damage,
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "belief.damage_variance",
            analysis.belief.damage_variance,
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "prediction.rul_hours",
            analysis.prediction.rul_hours,
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "risk.failure_probability_24h",
            analysis.prediction.failure_probability_24h,
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "risk.failure_probability_7d",
            analysis.prediction.failure_probability_7d,
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "risk.failure_probability_30d",
            analysis.prediction.failure_probability_30d,
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "risk.cvar95",
            analysis.prediction.cvar95,
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "counterfactual.defer_cost",
            counterfactual_cost(analysis, "defer"),
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "counterfactual.inspect_cost",
            counterfactual_cost(analysis, "inspect"),
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "counterfactual.reduce_load_cost",
            counterfactual_cost(analysis, "reduce_load"),
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "counterfactual.repair_cost",
            counterfactual_cost(analysis, "repair"),
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "counterfactual.replace_cost",
            counterfactual_cost(analysis, "replace"),
        );
        set_node(
            &mut nodes,
            &self.asset_id,
            "decision.recommended_action",
            AgentOutputValue::Text(analysis.recommendation.recommended_action.clone()),
        );
        set_float(
            &mut nodes,
            &self.asset_id,
            "decision.action_confidence",
            analysis.recommendation.action_confidence,
        );
        set_node(
            &mut nodes,
            &self.asset_id,
            "decision.decision_reason",
            AgentOutputValue::Text(analysis.recommendation.decision_reason.clone()),
        );
        set_node(
            &mut nodes,
            &self.asset_id,
            "model.policy_version",
            AgentOutputValue::Text(version_string(versions, "policy_version")),
        );
        set_node(
            &mut nodes,
            &self.asset_id,
            "model.residual_version",
            AgentOutputValue::Text(version_string(versions, "residual_version")),
        );
        set_node(
            &mut nodes,
            &self.asset_id,
            "output.sequence",
            AgentOutputValue::UInt(sequence),
        );
        set_node(
            &mut nodes,
            &self.asset_id,
            "output.initialized",
            AgentOutputValue::Bool(true),
        );

        *next = AgentOutputSnapshot {
            endpoint: self.config.endpoint.clone(),
            namespace_uri: self.config.namespace_uri.clone(),
            asset_id: self.asset_id.clone(),
            initialized: true,
            sequence,
            updated_at: Some(Utc::now().to_rfc3339()),
            source_case_id: case_id.map(str::to_string),
            node_count: nodes.len(),
            nodes,
        };
        Ok(next.clone())
    }

    fn read_node_value(&self, node_id: &NodeId) -> Option<AgentOutputValue> {
        let id = match &node_id.identifier {
            Identifier::String(value) => value.as_ref().to_string(),
            _ => return None,
        };
        self.inner
            .read()
            .ok()
            .and_then(|snapshot| snapshot.nodes.get(&id).map(|node| node.value.clone()))
    }
}

pub async fn spawn_agent_output_server(
    config: &AgentOutputConfig,
    state: AgentOutputState,
) -> Result<AgentOutputServerRuntime, AgentOutputError> {
    let socket_addr = parse_endpoint_socket_addr(&config.endpoint)?;
    let listener = tokio::net::TcpListener::bind(socket_addr)
        .await
        .map_err(|source| AgentOutputError::Bind {
            endpoint: config.endpoint.clone(),
            source,
        })?;
    let host = socket_addr.ip().to_string();
    let port = socket_addr.port();
    let discovery_url = format!("opc.tcp://{host}:{port}/");

    let builder = ServerBuilder::new()
        .application_name("AssetPilot Deterministic Output")
        .application_uri("urn:assetpilot:output-server")
        .product_uri("urn:assetpilot")
        .create_sample_keypair(true)
        .certificate_path("own/cert.der")
        .private_key_path("private/private.pem")
        .host(host)
        .port(port)
        .pki_dir(output_pki_dir())
        .discovery_urls(vec![discovery_url])
        .add_endpoint(
            "none",
            (
                "/",
                SecurityPolicy::None,
                MessageSecurityMode::None,
                &[ANONYMOUS_USER_TOKEN_ID] as &[&str],
            ),
        )
        .with_node_manager(agent_output_node_manager(
            config.namespace_uri.clone(),
            state,
        ));

    let (server, handle) = builder.build().map_err(AgentOutputError::Build)?;
    spawn_server_task(server.run_with(listener));
    Ok(AgentOutputServerRuntime {
        endpoint: config.endpoint.clone(),
        namespace_uri: config.namespace_uri.clone(),
        handle,
    })
}

pub fn output_node_id(asset_id: &str, suffix: &str) -> String {
    format!("agent:{asset_id}.{suffix}")
}

fn agent_output_node_manager(
    namespace_uri: String,
    state: AgentOutputState,
) -> impl NodeManagerBuilder {
    InMemoryNodeManagerBuilder::new(AgentOutputNodeManagerBuilder {
        namespace_uri,
        state,
    })
}

struct AgentOutputNodeManagerBuilder {
    namespace_uri: String,
    state: AgentOutputState,
}

impl InMemoryNodeManagerImplBuilder for AgentOutputNodeManagerBuilder {
    type Impl = AgentOutputNodeManager;

    fn build(self, context: ServerContext, address_space: &mut AddressSpace) -> Self::Impl {
        let namespace_index =
            add_namespaces(&context, address_space, &[self.namespace_uri.as_str()])[0];
        add_output_nodes(address_space, namespace_index, self.state.asset_id());
        AgentOutputNodeManager {
            namespace_uri: self.namespace_uri,
            namespace_index,
            state: self.state,
        }
    }
}

struct AgentOutputNodeManager {
    namespace_uri: String,
    namespace_index: u16,
    state: AgentOutputState,
}

#[async_trait]
impl InMemoryNodeManagerImpl for AgentOutputNodeManager {
    async fn init(&self, _address_space: &mut AddressSpace, _context: ServerContext) {}

    fn name(&self) -> &str {
        "agent_output"
    }

    fn namespaces(&self) -> Vec<opcua::server::diagnostics::NamespaceMetadata> {
        vec![opcua::server::diagnostics::NamespaceMetadata {
            is_namespace_subset: Some(false),
            namespace_uri: self.namespace_uri.clone(),
            namespace_index: self.namespace_index,
            ..Default::default()
        }]
    }

    async fn read_values(
        &self,
        context: &RequestContext,
        address_space: &opcua::sync::RwLock<AddressSpace>,
        nodes: &[&ParsedReadValueId],
        max_age: f64,
        timestamps_to_return: TimestampsToReturn,
    ) -> Vec<DataValue> {
        let address_space = address_space.read();
        nodes
            .iter()
            .map(|node| {
                if node.attribute_id != AttributeId::Value {
                    return address_space.read(context, node, max_age, timestamps_to_return);
                }
                match self.state.read_node_value(&node.node_id) {
                    Some(value) => data_value(value),
                    None => address_space.read(context, node, max_age, timestamps_to_return),
                }
            })
            .collect()
    }
}

fn add_output_nodes(address_space: &mut AddressSpace, namespace_index: u16, asset_id: &str) {
    let folder_id = NodeId::new(namespace_index, format!("agent:{asset_id}"));
    ObjectBuilder::new(&folder_id, asset_id, asset_id)
        .organized_by(ObjectId::ObjectsFolder)
        .insert(address_space);

    for spec in OUTPUT_NODE_SPECS {
        let node_id = NodeId::new(namespace_index, output_node_id(asset_id, spec.suffix));
        let variable = VariableBuilder::new(&node_id, spec.signal, spec.signal)
            .value(default_variant(spec.data_type))
            .data_type(opcua_data_type(spec.data_type))
            .access_level(AccessLevel::CURRENT_READ)
            .user_access_level(AccessLevel::CURRENT_READ)
            .build();
        address_space.insert::<_, NodeId>(variable, None);
        address_space.insert_reference(&folder_id, &node_id, ReferenceTypeId::HasComponent);
        address_space.insert_reference(
            &node_id,
            &VariableTypeId::BaseDataVariableType.into(),
            ReferenceTypeId::HasTypeDefinition,
        );
    }
}

fn empty_snapshot(config: &AgentOutputConfig, asset_id: &str) -> AgentOutputSnapshot {
    let nodes = build_default_nodes(asset_id);
    AgentOutputSnapshot {
        endpoint: config.endpoint.clone(),
        namespace_uri: config.namespace_uri.clone(),
        asset_id: asset_id.to_string(),
        initialized: false,
        sequence: 0,
        updated_at: None,
        source_case_id: None,
        node_count: nodes.len(),
        nodes,
    }
}

fn build_default_nodes(asset_id: &str) -> BTreeMap<String, AgentOutputNode> {
    OUTPUT_NODE_SPECS
        .iter()
        .map(|spec| {
            let node_id = output_node_id(asset_id, spec.suffix);
            (
                node_id.clone(),
                AgentOutputNode {
                    signal: spec.signal,
                    node_id,
                    data_type: spec.data_type,
                    value: default_value(spec.data_type),
                },
            )
        })
        .collect()
}

fn set_float(
    nodes: &mut BTreeMap<String, AgentOutputNode>,
    asset_id: &str,
    suffix: &str,
    value: f64,
) {
    set_node(
        nodes,
        asset_id,
        suffix,
        AgentOutputValue::Float(sanitize_float(value)),
    );
}

fn set_float_option(
    nodes: &mut BTreeMap<String, AgentOutputNode>,
    asset_id: &str,
    suffix: &str,
    value: Option<f64>,
) {
    set_float(nodes, asset_id, suffix, value.unwrap_or(f64::NAN));
}

fn set_node(
    nodes: &mut BTreeMap<String, AgentOutputNode>,
    asset_id: &str,
    suffix: &str,
    value: AgentOutputValue,
) {
    let node_id = output_node_id(asset_id, suffix);
    if let Some(node) = nodes.get_mut(&node_id) {
        node.value = value;
    }
}

fn counterfactual_cost(analysis: &AnalysisResult, action: &str) -> f64 {
    analysis
        .counterfactual_costs
        .get(action)
        .copied()
        .unwrap_or(0.0)
}

fn version_string(versions: &Value, key: &str) -> String {
    versions
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string()
}

fn default_value(data_type: AgentOutputDataType) -> AgentOutputValue {
    match data_type {
        AgentOutputDataType::Float => AgentOutputValue::Float(0.0),
        AgentOutputDataType::Text => AgentOutputValue::Text(String::new()),
        AgentOutputDataType::UInt => AgentOutputValue::UInt(0),
        AgentOutputDataType::Bool => AgentOutputValue::Bool(false),
    }
}

fn default_variant(data_type: AgentOutputDataType) -> Variant {
    match default_value(data_type) {
        AgentOutputValue::Float(value) => Variant::Double(value),
        AgentOutputValue::Text(value) => Variant::String(value.into()),
        AgentOutputValue::UInt(value) => Variant::UInt64(value),
        AgentOutputValue::Bool(value) => Variant::Boolean(value),
    }
}

fn opcua_data_type(data_type: AgentOutputDataType) -> DataTypeId {
    match data_type {
        AgentOutputDataType::Float => DataTypeId::Double,
        AgentOutputDataType::Text => DataTypeId::String,
        AgentOutputDataType::UInt => DataTypeId::UInt64,
        AgentOutputDataType::Bool => DataTypeId::Boolean,
    }
}

fn data_value(value: AgentOutputValue) -> DataValue {
    let now = DateTime::now();
    DataValue {
        value: Some(match value {
            AgentOutputValue::Float(value) => Variant::Double(sanitize_float(value)),
            AgentOutputValue::Text(value) => Variant::String(value.into()),
            AgentOutputValue::UInt(value) => Variant::UInt64(value),
            AgentOutputValue::Bool(value) => Variant::Boolean(value),
        }),
        status: Some(StatusCode::Good),
        source_timestamp: Some(now),
        server_timestamp: Some(now),
        ..Default::default()
    }
}

fn sanitize_float(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        f64::NAN
    }
}

fn parse_endpoint_socket_addr(endpoint: &str) -> Result<SocketAddr, AgentOutputError> {
    let raw =
        endpoint
            .strip_prefix("opc.tcp://")
            .ok_or_else(|| AgentOutputError::InvalidEndpoint {
                endpoint: endpoint.to_string(),
            })?;
    let authority = raw
        .split('/')
        .next()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| AgentOutputError::InvalidEndpoint {
            endpoint: endpoint.to_string(),
        })?;
    authority
        .to_socket_addrs()
        .map_err(|_| AgentOutputError::InvalidEndpoint {
            endpoint: endpoint.to_string(),
        })?
        .next()
        .ok_or_else(|| AgentOutputError::InvalidEndpoint {
            endpoint: endpoint.to_string(),
        })
}

fn output_pki_dir() -> PathBuf {
    std::env::var("ASSET_AGENT_OUTPUT_PKI_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join("asset-agent-opcua-output-pki"))
}

fn spawn_server_task(task: impl std::future::Future<Output = Result<(), String>> + Send + 'static) {
    let _handle: JoinHandle<()> = tokio::spawn(async move {
        if let Err(error) = task.await {
            tracing::error!(%error, "OPC UA output server stopped");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        output_node_id, AgentOutputConfig, AgentOutputState, AgentOutputValue, OUTPUT_NODE_SPECS,
    };
    use asset_agent_core::{
        features::analyze_cavitation,
        load_model_config,
        replay::{read_trace, select_as_of},
    };
    use std::path::PathBuf;

    #[test]
    fn builds_stable_output_node_ids() {
        assert_eq!(
            output_node_id("hp_pump_1", "health.risk_level"),
            "agent:hp_pump_1.health.risk_level"
        );
        assert_eq!(OUTPUT_NODE_SPECS.len(), 24);
    }

    #[test]
    fn output_nodes_update_after_analysis() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let config = load_model_config(&root, "hp_pump_1").expect("model config");
        let trace =
            read_trace(root.join("traces/hp_pump_1_low_suction_pressure.jsonl")).expect("trace");
        let snapshot = select_as_of(&trace, "trace:last").expect("snapshot");
        let analysis = analyze_cavitation(snapshot, &config);
        let output = AgentOutputState::new(AgentOutputConfig::default(), "hp_pump_1");

        assert_eq!(output.snapshot().expect("initial").sequence, 0);
        let published = output
            .publish_analysis(Some("case-smoke"), &analysis, &config.versions)
            .expect("publish");

        assert!(published.initialized);
        assert_eq!(published.sequence, 1);
        assert_eq!(published.source_case_id.as_deref(), Some("case-smoke"));
        assert_eq!(
            published
                .nodes
                .get("agent:hp_pump_1.output.sequence")
                .map(|node| &node.value),
            Some(&AgentOutputValue::UInt(1))
        );
        assert_eq!(
            published
                .nodes
                .get("agent:hp_pump_1.decision.recommended_action")
                .map(|node| &node.value),
            Some(&AgentOutputValue::Text(
                analysis.recommendation.recommended_action
            ))
        );
    }
}
