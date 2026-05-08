use asset_agent_memory::MemoryStore;
use asset_agent_opcua::output_server::{
    spawn_agent_output_server, AgentOutputConfig, AgentOutputServerRuntime, AgentOutputState,
};
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderName, HeaderValue, Method, Request, StatusCode},
    middleware::{self, Next},
    response::Response,
    Router,
};
use std::{env, net::SocketAddr, path::PathBuf, sync::Arc};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};

mod routes;

#[derive(Clone)]
pub struct AppState {
    pub project_root: PathBuf,
    pub asset_id: String,
    pub opcua_endpoint: String,
    pub opcua_namespace_uri: String,
    pub replay_trace: PathBuf,
    pub memory_db_path: PathBuf,
    pub output_config: AgentOutputConfig,
    pub output_state: AgentOutputState,
    pub output_runtime: AgentOutputServerRuntime,
}

#[derive(Clone)]
struct SecurityConfig {
    api_token: Option<String>,
}

impl AppState {
    pub fn open_memory_store(&self) -> anyhow::Result<MemoryStore> {
        Ok(MemoryStore::open(resolve_project_path(
            &self.project_root,
            &self.memory_db_path,
        ))?)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let addr: SocketAddr = env::var("ASSET_AGENT_API_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse()?;
    let security = SecurityConfig {
        api_token: env::var("ASSET_AGENT_API_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty()),
    };
    reject_unsafe_public_bind(addr, &security)?;

    let project_root = env::var("ASSET_AGENT_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    let asset_id = env::var("ASSET_AGENT_ASSET_ID").unwrap_or_else(|_| "hp_pump_1".to_string());
    let output_config = AgentOutputConfig::new(
        env::var("ASSET_AGENT_OUTPUT_ENDPOINT")
            .unwrap_or_else(|_| "opc.tcp://127.0.0.1:4841".to_string()),
        env::var("ASSET_AGENT_OUTPUT_NAMESPACE_URI")
            .unwrap_or_else(|_| "urn:assetpilot:outputs".to_string()),
    );
    let output_state = AgentOutputState::new(output_config.clone(), asset_id.clone());
    let output_runtime = spawn_agent_output_server(&output_config, output_state.clone()).await?;

    let state = Arc::new(AppState {
        project_root,
        asset_id,
        opcua_endpoint: env::var("OPCUA_ENDPOINT")
            .unwrap_or_else(|_| "opc.tcp://127.0.0.1:4840".to_string()),
        opcua_namespace_uri: env::var("OPCUA_NAMESPACE_URI")
            .unwrap_or_else(|_| "urn:twinedge:opcua-edge".to_string()),
        replay_trace: env::var("ASSET_AGENT_REPLAY_TRACE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("traces/hp_pump_1_low_suction_pressure.jsonl")),
        memory_db_path: env::var("ASSET_AGENT_DB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/asset_agent_memory.sqlite3")),
        output_config,
        output_state,
        output_runtime,
    });

    let app = Router::new()
        .merge(routes::router())
        .layer(middleware::from_fn_with_state(security, require_api_token))
        .layer(cors_layer()?)
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("asset-agent-api listening on {addr}");
    tracing::info!(
        "agent OPC UA output listening on {}",
        state.output_config.endpoint
    );
    axum::serve(listener, app).await?;
    Ok(())
}

async fn require_api_token(
    State(security): State<SecurityConfig>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let Some(expected) = security.api_token.as_deref() else {
        return Ok(next.run(request).await);
    };
    let authorized = bearer_token_matches(request.headers(), expected)
        || header_token_matches(request.headers(), expected);
    if authorized {
        Ok(next.run(request).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn bearer_token_matches(headers: &axum::http::HeaderMap, expected: &str) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .is_some_and(|token| token == expected)
}

fn header_token_matches(headers: &axum::http::HeaderMap, expected: &str) -> bool {
    headers
        .get("x-asset-agent-token")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|token| token == expected)
}

fn reject_unsafe_public_bind(addr: SocketAddr, security: &SecurityConfig) -> anyhow::Result<()> {
    if is_public_bind(addr)
        && security.api_token.is_none()
        && !env_bool("ASSET_AGENT_ALLOW_UNAUTHENTICATED_PUBLIC")
    {
        anyhow::bail!(
            "refusing to bind {addr} without ASSET_AGENT_API_TOKEN; bind localhost or set ASSET_AGENT_ALLOW_UNAUTHENTICATED_PUBLIC=1 for a lab"
        );
    }
    Ok(())
}

fn is_public_bind(addr: SocketAddr) -> bool {
    let ip = addr.ip();
    ip.is_unspecified() || !ip.is_loopback()
}

fn env_bool(name: &str) -> bool {
    env::var(name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false)
}

fn cors_layer() -> anyhow::Result<CorsLayer> {
    let origins = env::var("ASSET_AGENT_CORS_ORIGINS")
        .unwrap_or_else(|_| "http://127.0.0.1:3000,http://localhost:3000".to_string());
    let allow_origin = if origins.trim() == "*" {
        AllowOrigin::any()
    } else {
        let parsed = origins
            .split(',')
            .map(str::trim)
            .filter(|origin| !origin.is_empty())
            .map(HeaderValue::from_str)
            .collect::<Result<Vec<_>, _>>()?;
        AllowOrigin::list(parsed)
    };

    Ok(CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            HeaderName::from_static("x-asset-agent-token"),
        ]))
}

fn resolve_project_path(project_root: &std::path::Path, path: &PathBuf) -> PathBuf {
    if path.is_absolute() {
        path.clone()
    } else {
        project_root.join(path)
    }
}
