use axum::{
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use bytes::Bytes;
use chrono::{Duration, Utc};
use futures_util::StreamExt;
use reqwest::Proxy;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    env, fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicI64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};
use thiserror::Error;
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

const APP_DIR: &str = ".codexswitch";
const DB_FILE: &str = "codexswitch.db";
const CIRCUIT_BREAKER_FAILURE_THRESHOLD: i64 = 3;
const CIRCUIT_BREAKER_OPEN_SECONDS: i64 = 60;
static TELEMETRY_LAST_PRUNE_EPOCH: AtomicI64 = AtomicI64::new(0);

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("invalid config: {0}")]
    Config(String),
    #[error("{0}")]
    Message(String),
}

impl CoreError {
    fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }
}

impl IntoResponse for CoreError {
    fn into_response(self) -> Response {
        let status = match self {
            CoreError::Database(_) | CoreError::Io { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            CoreError::Http(_) => StatusCode::BAD_GATEWAY,
            CoreError::Config(_) | CoreError::Message(_) => StatusCode::BAD_REQUEST,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

type CoreResult<T> = Result<T, CoreError>;

#[derive(Clone)]
pub struct SharedState {
    pub database: Arc<Database>,
}

impl SharedState {
    pub async fn new() -> CoreResult<Self> {
        let database = Arc::new(Database::open_default()?);
        database.initialize()?;
        Ok(Self { database })
    }

    #[cfg(test)]
    pub(crate) fn memory_for_test() -> CoreResult<Self> {
        let database = Arc::new(Database::memory()?);
        database.initialize()?;
        Ok(Self { database })
    }
}

pub async fn serve(state: SharedState, addr: &str) -> CoreResult<()> {
    let socket: SocketAddr = addr
        .parse()
        .map_err(|err| CoreError::Config(format!("invalid listen address {addr}: {err}")))?;
    let listener = tokio::net::TcpListener::bind(socket)
        .await
        .map_err(|err| CoreError::io(addr, err))?;
    axum::serve(listener, router(state))
        .await
        .map_err(|err| CoreError::Config(format!("server error: {err}")))?;
    Ok(())
}

fn router(state: SharedState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/state", get(api_state))
        .route("/api/providers", get(list_providers).post(add_provider))
        .route("/api/providers/test-all", post(test_all_providers))
        .route("/api/providers/:id/models", get(fetch_provider_models))
        .route("/api/providers/:id/models/sync", post(sync_provider_models))
        .route(
            "/api/providers/:id",
            put(update_provider).delete(delete_provider),
        )
        .route("/api/providers/:id/test", post(test_provider))
        .route("/api/providers/:id/switch", post(switch_provider))
        .route("/api/providers/reorder", post(reorder_providers))
        .route(
            "/api/proxy/config",
            get(get_proxy_config).put(update_proxy_config),
        )
        .route("/api/proxy/start", post(start_proxy))
        .route("/api/proxy/stop", post(stop_proxy))
        .route("/api/proxy/status", get(proxy_status))
        .route("/api/gateway/toggle", post(toggle_gateway))
        .route("/api/gateway/restart", post(restart_gateway))
        .route("/api/gateway/quick-task", post(quick_task))
        .route("/api/database/reset", post(reset_database))
        .route("/api/logs", get(list_logs))
        .route("/api/logs/clear", post(clear_logs))
        .route(
            "/api/admin/users",
            get(list_gateway_users).post(create_gateway_user),
        )
        .route("/api/admin/users/:id", put(update_gateway_user))
        .route(
            "/api/admin/projects",
            get(list_gateway_projects).post(create_gateway_project),
        )
        .route("/api/admin/projects/:id", put(update_gateway_project))
        .route(
            "/api/admin/keys",
            get(list_gateway_keys).post(create_gateway_key),
        )
        .route("/api/admin/keys/:id", put(update_gateway_key))
        .route("/api/admin/keys/:id/revoke", post(revoke_gateway_key))
        .route("/api/admin/usage/summary", get(gateway_usage_summary))
        .route(
            "/api/admin/distillation/jobs",
            get(list_distillation_jobs).post(create_distillation_job),
        )
        .route(
            "/api/admin/distillation/jobs/:id",
            get(get_distillation_job),
        )
        .route(
            "/api/telemetry/config",
            get(get_telemetry_config).put(update_telemetry_config),
        )
        .route("/api/telemetry/summary", get(telemetry_summary))
        .route(
            "/api/telemetry/traces",
            get(list_telemetry_traces).delete(clear_telemetry_traces),
        )
        .route("/api/chat/send", post(chat_send))
        .route("/api/models", get(list_models).post(add_model))
        .route("/api/models/:id", put(update_model).delete(delete_model))
        .route("/api/agents/:id", put(update_agent))
        .route("/api/agents/:id/invoke", post(invoke_agent))
        .route("/v1/messages/count_tokens", post(proxy_count_tokens))
        .route("/v1/messages", post(proxy_messages))
        .route("/v1/chat/completions", post(proxy_chat))
        .route("/v1/responses", post(proxy_responses))
        .route("/v1/models", get(proxy_models))
        .route("/v1beta/models/*path", post(proxy_gemini))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
                .allow_headers(Any),
        )
        .with_state(state)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    #[serde(default = "default_app_type")]
    pub app_type: String,
    pub name: String,
    #[serde(rename = "type", default = "default_provider_type")]
    pub provider_type: String,
    #[serde(default)]
    pub service_type: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub base_urls: Vec<String>,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub api_keys: Vec<String>,
    #[serde(default = "default_status")]
    pub status: String,
    pub latency: Option<String>,
    pub description: Option<String>,
    #[serde(default)]
    pub skip_tls: bool,
    #[serde(default)]
    pub low_quality: bool,
    #[serde(default)]
    pub auto_blacklist: bool,
    #[serde(default)]
    pub codex_passthrough: bool,
    #[serde(default)]
    pub codex_tools_compat: bool,
    #[serde(default)]
    pub normalize_user_id: bool,
    #[serde(default)]
    pub normalize_chat_roles: bool,
    pub reasoning_style: Option<String>,
    #[serde(default)]
    pub custom_headers: Vec<HeaderPair>,
    pub proxy_url: Option<String>,
    pub route_prefix: Option<String>,
    #[serde(default)]
    pub model_mapping: BTreeMap<String, String>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub is_current: bool,
    #[serde(default)]
    pub failover_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HeaderPair {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub is_default: bool,
    #[serde(default)]
    pub context_window: String,
    #[serde(default)]
    pub latency: String,
    #[serde(default)]
    pub max_output: String,
    #[serde(default)]
    pub supports_images: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpstreamModel {
    pub id: String,
    pub name: String,
    pub owned_by: Option<String>,
    pub context_window: Option<String>,
    pub max_output: Option<String>,
    #[serde(default)]
    pub supports_images: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub provider_id: String,
    pub model_id: String,
    pub status: String,
    pub last_run: Option<String>,
    pub metrics: Value,
    #[serde(default)]
    pub allowed_providers: Vec<String>,
    pub context_length_limit: Option<u64>,
    pub max_output_limit: Option<u64>,
    pub temperature: Option<f64>,
    #[serde(default)]
    pub auto_format_convert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    pub provider_id: String,
    pub is_healthy: bool,
    pub consecutive_failures: i64,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
    pub last_error: Option<String>,
    pub circuit_open_until: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayState {
    pub status: String,
    #[serde(rename = "uptimeSeconds")]
    pub uptime_seconds: u64,
    #[serde(rename = "cpuUsage")]
    pub cpu_usage: u64,
    #[serde(rename = "memoryUsage")]
    pub memory_usage: u64,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyConfig {
    pub listen_address: String,
    pub listen_port: u16,
    pub enabled: bool,
    pub max_retries: u8,
    pub streaming_first_byte_timeout: u64,
    pub streaming_idle_timeout: u64,
    pub non_streaming_timeout: u64,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            listen_address: "127.0.0.1".to_string(),
            listen_port: 3900,
            enabled: false,
            max_retries: 3,
            streaming_first_byte_timeout: 60,
            streaming_idle_timeout: 120,
            non_streaming_timeout: 600,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppStateResponse {
    providers: Vec<Provider>,
    provider_health: Vec<ProviderHealth>,
    models: Vec<Model>,
    agents: Vec<Agent>,
    mcp_servers: Vec<Value>,
    logs: Vec<LogEntry>,
    gateway: GatewayState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelemetryConfig {
    enabled: bool,
    capture_bodies: bool,
    retention_days: i64,
    max_body_preview_bytes: usize,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            capture_bodies: false,
            retention_days: 30,
            max_body_preview_bytes: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestTrace {
    id: String,
    timestamp: String,
    project_sequence: i64,
    project_id: String,
    project_name: String,
    workspace_path: Option<String>,
    git_remote: Option<String>,
    session_id: Option<String>,
    protocol: String,
    route_path: String,
    provider_id: String,
    provider_name: String,
    service_type: String,
    request_model: Option<String>,
    upstream_url: Option<String>,
    status_code: Option<i64>,
    success: bool,
    latency_ms: u64,
    request_bytes: u64,
    response_bytes: Option<u64>,
    error: Option<String>,
    capture_level: String,
    request_body_preview: Option<String>,
    response_body_preview: Option<String>,
    gateway_user_id: Option<String>,
    gateway_user_name: Option<String>,
    gateway_project_id: Option<String>,
    gateway_project_name: Option<String>,
    gateway_key_id: Option<String>,
    gateway_key_name: Option<String>,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    estimated_cost_usd: f64,
}

#[derive(Debug, Clone)]
struct RequestTraceDraft {
    project_id: String,
    project_name: String,
    workspace_path: Option<String>,
    git_remote: Option<String>,
    session_id: Option<String>,
    protocol: String,
    route_path: String,
    provider_id: String,
    provider_name: String,
    service_type: String,
    request_model: Option<String>,
    upstream_url: Option<String>,
    status_code: Option<i64>,
    success: bool,
    latency_ms: u64,
    request_bytes: u64,
    response_bytes: Option<u64>,
    error: Option<String>,
    capture_level: String,
    request_body_preview: Option<String>,
    response_body_preview: Option<String>,
    gateway_user_id: Option<String>,
    gateway_user_name: Option<String>,
    gateway_project_id: Option<String>,
    gateway_project_name: Option<String>,
    gateway_key_id: Option<String>,
    gateway_key_name: Option<String>,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    estimated_cost_usd: f64,
}

#[derive(Clone)]
struct ResponseTraceRecorder {
    database: Arc<Database>,
    config: TelemetryConfig,
    trace_id: String,
    principal: Option<GatewayPrincipal>,
    request_usage: UsageEstimate,
    request_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GatewayUser {
    id: String,
    display_name: String,
    email: Option<String>,
    status: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GatewayProject {
    id: String,
    user_id: String,
    user_name: String,
    name: String,
    slug: String,
    status: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GatewayKey {
    id: String,
    key_prefix: String,
    name: String,
    user_id: String,
    user_name: String,
    project_id: String,
    project_name: String,
    status: String,
    quota_requests: i64,
    quota_tokens: i64,
    used_requests: i64,
    used_tokens: i64,
    expires_at: Option<String>,
    last_used_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Clone)]
struct GatewayPrincipal {
    key_id: String,
    key_name: String,
    user_id: String,
    user_name: String,
    project_id: String,
    project_name: String,
    project_slug: String,
    quota_requests: i64,
    quota_tokens: i64,
    used_requests: i64,
    used_tokens: i64,
}

#[derive(Debug, Clone, Copy, Default)]
struct UsageEstimate {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GatewayUsageSummary {
    total_requests: usize,
    total_tokens: u64,
    estimated_cost_usd: f64,
    by_user: Vec<GatewayUsageBucket>,
    by_project: Vec<GatewayUsageBucket>,
    by_key: Vec<GatewayKeyUsageBucket>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GatewayUsageBucket {
    id: String,
    name: String,
    user_id: Option<String>,
    user_name: Option<String>,
    project_id: Option<String>,
    project_name: Option<String>,
    requests: usize,
    tokens: u64,
    estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GatewayKeyUsageBucket {
    key_id: String,
    key_name: String,
    user_id: String,
    user_name: String,
    project_id: String,
    project_name: String,
    used_requests: i64,
    used_tokens: i64,
    quota_requests: i64,
    quota_tokens: i64,
    estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DistillationJob {
    id: String,
    name: String,
    status: String,
    project_id: Option<String>,
    user_id: Option<String>,
    trace_count: usize,
    export_path: Option<String>,
    error: Option<String>,
    filters: Value,
    created_at: String,
    completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentProcessInvocation {
    command: String,
    executable: String,
    args: Vec<String>,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    response: Option<String>,
    is_error: bool,
    duration_ms: u64,
}

#[derive(Clone, Copy)]
enum ResponseAdapter {
    OpenAiChatToClaudeMessages { stream: bool },
}

struct PreparedProxyRequest {
    path: String,
    body: Bytes,
    adapter: Option<ResponseAdapter>,
}

#[derive(Debug, Clone)]
struct ProjectContext {
    project_id: String,
    project_name: String,
    workspace_path: Option<String>,
    git_remote: Option<String>,
    session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TraceQuery {
    project_id: Option<String>,
    limit: Option<usize>,
    order: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TelemetrySummary {
    total_requests: usize,
    total_projects: usize,
    success_rate: f64,
    avg_latency_ms: f64,
    capture_bodies: bool,
    retention_days: i64,
    by_project: Vec<TelemetryProjectSummary>,
    by_provider: Vec<TelemetryProviderSummary>,
    by_model: Vec<TelemetryModelSummary>,
    recent_errors: Vec<RequestTrace>,
    insights: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TelemetryProjectSummary {
    project_id: String,
    project_name: String,
    requests: usize,
    success_rate: f64,
    avg_latency_ms: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TelemetryProviderSummary {
    provider_id: String,
    provider_name: String,
    requests: usize,
    success_rate: f64,
    avg_latency_ms: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TelemetryModelSummary {
    model: String,
    requests: usize,
    success_rate: f64,
    avg_latency_ms: f64,
}

fn default_app_type() -> String {
    "codex".to_string()
}

fn default_provider_type() -> String {
    "custom".to_string()
}

fn default_status() -> String {
    "disconnected".to_string()
}

pub struct Database {
    conn: Mutex<Connection>,
    app_dir: PathBuf,
}

impl Database {
    fn open_default() -> CoreResult<Self> {
        let app_dir = app_dir();
        fs::create_dir_all(&app_dir).map_err(|err| CoreError::io(&app_dir, err))?;
        let db_path = app_dir.join(DB_FILE);
        let conn = Connection::open(&db_path)?;
        Ok(Self {
            conn: Mutex::new(conn),
            app_dir,
        })
    }

    #[cfg(test)]
    fn memory() -> CoreResult<Self> {
        Ok(Self {
            conn: Mutex::new(Connection::open_in_memory()?),
            app_dir: app_dir(),
        })
    }

    fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> CoreResult<T>) -> CoreResult<T> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| CoreError::Database(rusqlite::Error::InvalidQuery))?;
        f(&conn)
    }

    fn initialize(&self) -> CoreResult<()> {
        self.with_conn(|conn| {
            conn.execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                PRAGMA foreign_keys = ON;

                CREATE TABLE IF NOT EXISTS providers (
                    id TEXT PRIMARY KEY,
                    app_type TEXT NOT NULL,
                    name TEXT NOT NULL,
                    provider_type TEXT NOT NULL,
                    service_type TEXT NOT NULL,
                    base_url TEXT NOT NULL,
                    base_urls_json TEXT NOT NULL DEFAULT '[]',
                    api_key TEXT NOT NULL DEFAULT '',
                    api_keys_json TEXT NOT NULL DEFAULT '[]',
                    status TEXT NOT NULL DEFAULT 'disconnected',
                    latency TEXT,
                    description TEXT,
                    skip_tls INTEGER NOT NULL DEFAULT 0,
                    low_quality INTEGER NOT NULL DEFAULT 0,
                    auto_blacklist INTEGER NOT NULL DEFAULT 0,
                    codex_passthrough INTEGER NOT NULL DEFAULT 0,
                    codex_tools_compat INTEGER NOT NULL DEFAULT 0,
                    normalize_user_id INTEGER NOT NULL DEFAULT 0,
                    normalize_chat_roles INTEGER NOT NULL DEFAULT 0,
                    reasoning_style TEXT,
                    custom_headers_json TEXT NOT NULL DEFAULT '[]',
                    proxy_url TEXT,
                    route_prefix TEXT,
                    model_mapping_json TEXT NOT NULL DEFAULT '{}',
                    priority INTEGER NOT NULL DEFAULT 0,
                    is_current INTEGER NOT NULL DEFAULT 0,
                    failover_enabled INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS provider_endpoints (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    provider_id TEXT NOT NULL,
                    position INTEGER NOT NULL,
                    base_url TEXT NOT NULL,
                    UNIQUE(provider_id, position),
                    FOREIGN KEY(provider_id) REFERENCES providers(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS provider_keys (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    provider_id TEXT NOT NULL,
                    position INTEGER NOT NULL,
                    key_value TEXT NOT NULL,
                    UNIQUE(provider_id, position),
                    FOREIGN KEY(provider_id) REFERENCES providers(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS current_providers (
                    app_type TEXT PRIMARY KEY,
                    provider_id TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    FOREIGN KEY(provider_id) REFERENCES providers(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS app_settings (
                    key TEXT PRIMARY KEY,
                    value_json TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS models (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    tags_json TEXT NOT NULL DEFAULT '[]',
                    is_default INTEGER NOT NULL DEFAULT 0,
                    context_window TEXT NOT NULL DEFAULT '',
                    latency TEXT NOT NULL DEFAULT '',
                    max_output TEXT NOT NULL DEFAULT '',
                    supports_images INTEGER NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS agents (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    description TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    model_id TEXT NOT NULL,
                    status TEXT NOT NULL,
                    last_run TEXT,
                    metrics_json TEXT NOT NULL,
                    allowed_providers_json TEXT NOT NULL DEFAULT '[]',
                    context_length_limit INTEGER,
                    max_output_limit INTEGER,
                    temperature REAL,
                    auto_format_convert INTEGER NOT NULL DEFAULT 1
                );

                CREATE TABLE IF NOT EXISTS proxy_config (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    listen_address TEXT NOT NULL,
                    listen_port INTEGER NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 0,
                    max_retries INTEGER NOT NULL,
                    streaming_first_byte_timeout INTEGER NOT NULL,
                    streaming_idle_timeout INTEGER NOT NULL,
                    non_streaming_timeout INTEGER NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS provider_health (
                    provider_id TEXT PRIMARY KEY,
                    is_healthy INTEGER NOT NULL DEFAULT 0,
                    consecutive_failures INTEGER NOT NULL DEFAULT 0,
                    last_success_at TEXT,
                    last_failure_at TEXT,
                    last_error TEXT,
                    circuit_open_until TEXT
                );

                CREATE TABLE IF NOT EXISTS proxy_logs (
                    id TEXT PRIMARY KEY,
                    timestamp TEXT NOT NULL,
                    level TEXT NOT NULL,
                    source TEXT NOT NULL,
                    message TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS gateway_users (
                    id TEXT PRIMARY KEY,
                    display_name TEXT NOT NULL,
                    email TEXT,
                    status TEXT NOT NULL DEFAULT 'active',
                    created_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS gateway_projects (
                    id TEXT PRIMARY KEY,
                    user_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    slug TEXT NOT NULL UNIQUE,
                    status TEXT NOT NULL DEFAULT 'active',
                    created_at TEXT NOT NULL,
                    FOREIGN KEY(user_id) REFERENCES gateway_users(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS gateway_keys (
                    id TEXT PRIMARY KEY,
                    key_hash TEXT NOT NULL UNIQUE,
                    key_prefix TEXT NOT NULL,
                    name TEXT NOT NULL,
                    user_id TEXT NOT NULL,
                    project_id TEXT NOT NULL,
                    status TEXT NOT NULL DEFAULT 'active',
                    quota_requests INTEGER NOT NULL DEFAULT 0,
                    quota_tokens INTEGER NOT NULL DEFAULT 0,
                    used_requests INTEGER NOT NULL DEFAULT 0,
                    used_tokens INTEGER NOT NULL DEFAULT 0,
                    expires_at TEXT,
                    last_used_at TEXT,
                    created_at TEXT NOT NULL,
                    FOREIGN KEY(user_id) REFERENCES gateway_users(id) ON DELETE CASCADE,
                    FOREIGN KEY(project_id) REFERENCES gateway_projects(id) ON DELETE CASCADE
                );

                CREATE TABLE IF NOT EXISTS telemetry_config (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    enabled INTEGER NOT NULL DEFAULT 1,
                    capture_bodies INTEGER NOT NULL DEFAULT 0,
                    retention_days INTEGER NOT NULL DEFAULT 30,
                    max_body_preview_bytes INTEGER NOT NULL DEFAULT 4096,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE IF NOT EXISTS request_traces (
                    id TEXT PRIMARY KEY,
                    timestamp TEXT NOT NULL,
                    project_sequence INTEGER NOT NULL DEFAULT 0,
                    project_id TEXT NOT NULL,
                    project_name TEXT NOT NULL,
                    workspace_path TEXT,
                    git_remote TEXT,
                    session_id TEXT,
                    protocol TEXT NOT NULL,
                    route_path TEXT NOT NULL,
                    provider_id TEXT NOT NULL,
                    provider_name TEXT NOT NULL,
                    service_type TEXT NOT NULL,
                    request_model TEXT,
                    upstream_url TEXT,
                    status_code INTEGER,
                    success INTEGER NOT NULL DEFAULT 0,
                    latency_ms INTEGER NOT NULL DEFAULT 0,
                    request_bytes INTEGER NOT NULL DEFAULT 0,
                    response_bytes INTEGER,
                    error TEXT,
                    capture_level TEXT NOT NULL DEFAULT 'metadata',
                    request_body_preview TEXT,
                    response_body_preview TEXT,
                    gateway_user_id TEXT,
                    gateway_user_name TEXT,
                    gateway_project_id TEXT,
                    gateway_project_name TEXT,
                    gateway_key_id TEXT,
                    gateway_key_name TEXT,
                    prompt_tokens INTEGER NOT NULL DEFAULT 0,
                    completion_tokens INTEGER NOT NULL DEFAULT 0,
                    total_tokens INTEGER NOT NULL DEFAULT 0,
                    estimated_cost_usd REAL NOT NULL DEFAULT 0
                );

                CREATE TABLE IF NOT EXISTS distillation_jobs (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    status TEXT NOT NULL,
                    project_id TEXT,
                    user_id TEXT,
                    trace_count INTEGER NOT NULL DEFAULT 0,
                    export_path TEXT,
                    error TEXT,
                    filters_json TEXT NOT NULL DEFAULT '{}',
                    created_at TEXT NOT NULL,
                    completed_at TEXT
                );

                CREATE INDEX IF NOT EXISTS idx_request_traces_project_time
                    ON request_traces(project_id, timestamp DESC);
                CREATE INDEX IF NOT EXISTS idx_request_traces_provider_time
                    ON request_traces(provider_id, timestamp DESC);
                CREATE INDEX IF NOT EXISTS idx_request_traces_model_time
                    ON request_traces(request_model, timestamp DESC);
                "#,
            )?;
            ensure_provider_health_columns(conn)?;
            ensure_request_traces_columns(conn)?;

            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM providers", [], |row| row.get(0))?;
            if count == 0 {
                seed_defaults(conn)?;
            }
            ensure_default_agents(conn)?;
            conn.execute(
                "INSERT OR IGNORE INTO proxy_config (
                    id, listen_address, listen_port, enabled, max_retries,
                    streaming_first_byte_timeout, streaming_idle_timeout,
                    non_streaming_timeout, updated_at
                ) VALUES (1, '127.0.0.1', 3900, 0, 3, 60, 120, 600, ?1)",
                [Utc::now().to_rfc3339()],
            )?;
            let telemetry = TelemetryConfig::default();
            conn.execute(
                "INSERT OR IGNORE INTO telemetry_config (
                    id, enabled, capture_bodies, retention_days,
                    max_body_preview_bytes, updated_at
                ) VALUES (1, ?1, ?2, ?3, ?4, ?5)",
                params![
                    telemetry.enabled as i64,
                    telemetry.capture_bodies as i64,
                    telemetry.retention_days,
                    telemetry.max_body_preview_bytes as i64,
                    Utc::now().to_rfc3339()
                ],
            )?;
            seed_app_settings(conn)?;
            Ok(())
        })?;
        self.import_existing_configs()?;
        self.sync_provider_indexes()?;
        Ok(())
    }

    fn reset(&self) -> CoreResult<()> {
        self.with_conn(|conn| {
            conn.execute_batch(
                "DELETE FROM providers; DELETE FROM provider_endpoints; DELETE FROM provider_keys; DELETE FROM current_providers; DELETE FROM models; DELETE FROM agents; DELETE FROM provider_health; DELETE FROM proxy_logs; DELETE FROM request_traces; DELETE FROM app_settings; DELETE FROM gateway_keys; DELETE FROM gateway_projects; DELETE FROM gateway_users; DELETE FROM distillation_jobs;",
            )?;
            seed_defaults(conn)?;
            seed_app_settings(conn)?;
            Ok(())
        })
    }

    fn import_existing_configs(&self) -> CoreResult<()> {
        if let Some(provider) = import_claude_provider()? {
            self.upsert_seed_provider(provider)?;
        }
        if let Some(provider) = import_codex_provider()? {
            self.upsert_seed_provider(provider)?;
        }
        if let Some(provider) = import_gemini_provider()? {
            self.upsert_seed_provider(provider)?;
        }
        Ok(())
    }

    fn upsert_seed_provider(&self, provider: Provider) -> CoreResult<()> {
        let inserted = self.with_conn(|conn| {
            let exists: Option<String> = conn
                .query_row(
                    "SELECT id FROM providers WHERE id = ?1",
                    [&provider.id],
                    |row| row.get(0),
                )
                .optional()?;
            if exists.is_none() {
                insert_provider(conn, &provider)?;
                return Ok(true);
            }
            Ok(false)
        })?;
        if inserted {
            self.add_log(
                "info",
                "Config",
                &format!("Imported existing {} config", provider.name),
            )?;
        }
        Ok(())
    }

    fn providers(&self, mask: bool) -> CoreResult<Vec<Provider>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, app_type, name, provider_type, service_type, base_url, base_urls_json,
                    api_key, api_keys_json, status, latency, description, skip_tls, low_quality,
                    auto_blacklist, codex_passthrough, codex_tools_compat, normalize_user_id,
                    normalize_chat_roles, reasoning_style, custom_headers_json, proxy_url,
                    route_prefix, model_mapping_json, priority, is_current, failover_enabled
                 FROM providers ORDER BY priority ASC, created_at ASC",
            )?;
            let rows = stmt.query_map([], |row| provider_from_row(row, mask))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(CoreError::Database)
        })
    }

    #[cfg(test)]
    fn provider_endpoint_count(&self, provider_id: &str) -> CoreResult<i64> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM provider_endpoints WHERE provider_id = ?1",
                [provider_id],
                |row| row.get(0),
            )
            .map_err(CoreError::Database)
        })
    }

    #[cfg(test)]
    fn provider_key_count(&self, provider_id: &str) -> CoreResult<i64> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM provider_keys WHERE provider_id = ?1",
                [provider_id],
                |row| row.get(0),
            )
            .map_err(CoreError::Database)
        })
    }

    #[cfg(test)]
    fn app_settings_count(&self) -> CoreResult<i64> {
        self.with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM app_settings", [], |row| row.get(0))
                .map_err(CoreError::Database)
        })
    }

    #[cfg(test)]
    fn current_provider_for_app(&self, app_type: &str) -> CoreResult<Option<String>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT provider_id FROM current_providers WHERE app_type = ?1",
                [app_type],
                |row| row.get(0),
            )
            .optional()
            .map_err(CoreError::Database)
        })
    }

    fn sync_provider_indexes(&self) -> CoreResult<()> {
        self.with_conn(|conn| {
            let providers = list_providers_raw(conn)?;
            for provider in &providers {
                sync_provider_children(conn, provider)?;
                sync_current_provider_marker(conn, provider)?;
            }
            Ok(())
        })
    }

    fn provider_health(&self) -> CoreResult<Vec<ProviderHealth>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT provider_id, is_healthy, consecutive_failures, last_success_at,
                    last_failure_at, last_error, circuit_open_until
                 FROM provider_health ORDER BY provider_id",
            )?;
            let rows = stmt.query_map([], provider_health_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(CoreError::Database)
        })
    }

    fn provider_raw(&self, id: &str) -> CoreResult<Option<Provider>> {
        self.with_conn(|conn| get_provider(conn, id, false))
    }

    fn add_provider(&self, provider: &Provider) -> CoreResult<()> {
        self.with_conn(|conn| insert_provider(conn, provider))?;
        self.add_log(
            "info",
            "ProviderRouter",
            &format!("Registered provider {}", provider.name),
        )
    }

    fn update_provider(&self, id: &str, value: Value) -> CoreResult<()> {
        self.with_conn(|conn| {
            let mut provider = get_provider(conn, id, false)?
                .ok_or_else(|| CoreError::Message(format!("provider not found: {id}")))?;
            merge_provider(&mut provider, value);
            update_provider_row(conn, id, &provider)?;
            sync_provider_children(conn, &provider)?;
            sync_current_provider_marker(conn, &provider)
        })?;
        self.add_log("info", "ProviderRouter", &format!("Updated provider {id}"))
    }

    fn delete_provider(&self, id: &str) -> CoreResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM providers WHERE id = ?1", [id])?;
            conn.execute("DELETE FROM models WHERE provider_id = ?1", [id])?;
            conn.execute("DELETE FROM provider_health WHERE provider_id = ?1", [id])?;
            conn.execute(
                "DELETE FROM provider_endpoints WHERE provider_id = ?1",
                [id],
            )?;
            conn.execute("DELETE FROM provider_keys WHERE provider_id = ?1", [id])?;
            conn.execute("DELETE FROM current_providers WHERE provider_id = ?1", [id])?;
            Ok(())
        })?;
        self.add_log("warn", "ProviderRouter", &format!("Deleted provider {id}"))
    }

    fn reorder_provider(&self, current_index: usize, direction: &str) -> CoreResult<()> {
        let providers = self.providers(false)?;
        if current_index >= providers.len() {
            return Err(CoreError::Message("provider index out of bounds".into()));
        }
        let next_index = if direction == "up" {
            current_index.checked_sub(1)
        } else {
            Some(current_index + 1)
        }
        .filter(|idx| *idx < providers.len())
        .ok_or_else(|| CoreError::Message("provider reorder out of bounds".into()))?;

        self.with_conn(|conn| {
            conn.execute(
                "UPDATE providers SET priority = ?1 WHERE id = ?2",
                params![providers[next_index].priority, providers[current_index].id],
            )?;
            conn.execute(
                "UPDATE providers SET priority = ?1 WHERE id = ?2",
                params![providers[current_index].priority, providers[next_index].id],
            )?;
            Ok(())
        })?;
        self.add_log("info", "Gateway", "Provider failover priority changed")
    }

    fn switch_provider(&self, id: &str) -> CoreResult<()> {
        let provider = self
            .provider_raw(id)?
            .ok_or_else(|| CoreError::Message(format!("provider not found: {id}")))?;
        write_live_config(&provider, &self.app_dir)?;
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE providers SET is_current = CASE WHEN id = ?1 THEN 1 ELSE 0 END WHERE app_type = ?2",
                params![id, provider.app_type],
            )?;
            conn.execute(
                "INSERT INTO current_providers (app_type, provider_id, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(app_type) DO UPDATE SET
                    provider_id = excluded.provider_id,
                    updated_at = excluded.updated_at",
                params![provider.app_type, id, Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })?;
        self.add_log(
            "info",
            "Config",
            &format!(
                "Switched {} provider to {}",
                provider.app_type, provider.name
            ),
        )
    }

    fn models(&self) -> CoreResult<Vec<Model>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, provider_id, tags_json, is_default, context_window, latency, max_output, supports_images
                 FROM models ORDER BY provider_id, name",
            )?;
            let rows = stmt.query_map([], model_from_row)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(CoreError::Database)
        })
    }

    fn add_model(&self, model: &Model) -> CoreResult<()> {
        self.with_conn(|conn| insert_model(conn, model))?;
        self.add_log(
            "info",
            "Models",
            &format!("Registered model {}", model.name),
        )
    }

    fn provider_model_name_for_reference(
        &self,
        provider_id: &str,
        model_reference: &str,
    ) -> CoreResult<Option<String>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT name FROM models
                 WHERE provider_id = ?1 AND (id = ?2 OR name = ?2)
                 ORDER BY CASE WHEN id = ?2 THEN 0 ELSE 1 END
                 LIMIT 1",
                params![provider_id, model_reference],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(CoreError::Database)
        })
    }

    fn provider_default_model_name(&self, provider_id: &str) -> CoreResult<Option<String>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT name FROM models
                 WHERE provider_id = ?1
                 ORDER BY is_default DESC, name ASC
                 LIMIT 1",
                [provider_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(CoreError::Database)
        })
    }

    fn provider_ids_for_model_reference(&self, model_reference: &str) -> CoreResult<Vec<String>> {
        self.with_conn(|conn| {
            let mut stmt =
                conn.prepare("SELECT DISTINCT provider_id FROM models WHERE id = ?1 OR name = ?1")?;
            let rows = stmt.query_map([model_reference], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(CoreError::Database)
        })
    }

    fn import_provider_models(
        &self,
        provider_id: &str,
        upstream_models: &[UpstreamModel],
    ) -> CoreResult<Vec<Model>> {
        let imported = self.with_conn(|conn| {
            let has_default = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM models WHERE provider_id = ?1 AND is_default = 1)",
                [provider_id],
                |row| row.get::<_, i64>(0),
            )? != 0;
            let mut imported = Vec::new();
            for upstream_model in upstream_models {
                if upstream_model.id.trim().is_empty() {
                    continue;
                }
                let existing =
                    get_model_by_provider_and_name(conn, provider_id, &upstream_model.id)?;
                let is_default = existing
                    .as_ref()
                    .map(|model| model.is_default)
                    .unwrap_or(!has_default && imported.is_empty());
                let model = Model {
                    id: existing
                        .as_ref()
                        .map(|model| model.id.clone())
                        .unwrap_or_else(|| {
                            provider_model_record_id(provider_id, &upstream_model.id)
                        }),
                    name: upstream_model.id.clone(),
                    provider_id: provider_id.to_string(),
                    tags: existing
                        .as_ref()
                        .map(|model| model.tags.clone())
                        .unwrap_or_else(|| vec!["Upstream".into()]),
                    is_default,
                    context_window: upstream_model
                        .context_window
                        .clone()
                        .or_else(|| existing.as_ref().map(|model| model.context_window.clone()))
                        .unwrap_or_default(),
                    latency: existing
                        .as_ref()
                        .map(|model| model.latency.clone())
                        .unwrap_or_else(|| "—".into()),
                    max_output: upstream_model
                        .max_output
                        .clone()
                        .or_else(|| existing.as_ref().map(|model| model.max_output.clone()))
                        .unwrap_or_default(),
                    supports_images: upstream_model.supports_images
                        || existing
                            .as_ref()
                            .map(|model| model.supports_images)
                            .unwrap_or(false),
                };
                insert_model(conn, &model)?;
                conn.execute(
                    "DELETE FROM models
                     WHERE provider_id = ?1 AND lower(name) = lower(?2) AND id <> ?3",
                    params![provider_id, upstream_model.id, model.id],
                )?;
                imported.push(model);
            }
            Ok(imported)
        })?;
        self.add_log(
            "info",
            "Models",
            &format!(
                "Imported {} upstream models for provider {}",
                imported.len(),
                provider_id
            ),
        )?;
        Ok(imported)
    }

    fn update_model(&self, id: &str, updates: Value) -> CoreResult<()> {
        self.with_conn(|conn| {
            let mut model = get_model(conn, id)?
                .ok_or_else(|| CoreError::Message(format!("model not found: {id}")))?;
            merge_model(&mut model, updates);
            conn.execute(
                "UPDATE models SET name = ?2, provider_id = ?3, tags_json = ?4, is_default = ?5,
                    context_window = ?6, latency = ?7, max_output = ?8, supports_images = ?9
                 WHERE id = ?1",
                params![
                    id,
                    model.name,
                    model.provider_id,
                    json_string(&model.tags),
                    model.is_default as i64,
                    model.context_window,
                    model.latency,
                    model.max_output,
                    model.supports_images as i64
                ],
            )?;
            Ok(())
        })
    }

    fn delete_model(&self, id: &str) -> CoreResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM models WHERE id = ?1", [id])?;
            Ok(())
        })
    }

    fn agents(&self) -> CoreResult<Vec<Agent>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, description, provider_id, model_id, status, last_run, metrics_json,
                    allowed_providers_json, context_length_limit, max_output_limit, temperature, auto_format_convert
                 FROM agents ORDER BY name",
            )?;
            let rows = stmt.query_map([], agent_from_row)?;
            rows.collect::<Result<Vec<_>, _>>().map_err(CoreError::Database)
        })
    }

    fn agent(&self, id: &str) -> CoreResult<Option<Agent>> {
        self.with_conn(|conn| get_agent(conn, id))
    }

    fn update_agent(&self, id: &str, updates: Value) -> CoreResult<()> {
        self.with_conn(|conn| {
            let mut agent = get_agent(conn, id)?
                .ok_or_else(|| CoreError::Message(format!("agent not found: {id}")))?;
            merge_agent(&mut agent, updates);
            conn.execute(
                "UPDATE agents SET provider_id = ?2, model_id = ?3, status = ?4, last_run = ?5,
                    metrics_json = ?6, allowed_providers_json = ?7, context_length_limit = ?8,
                    max_output_limit = ?9, temperature = ?10, auto_format_convert = ?11
                 WHERE id = ?1",
                params![
                    id,
                    agent.provider_id,
                    agent.model_id,
                    agent.status,
                    agent.last_run,
                    json_string(&agent.metrics),
                    json_string(&agent.allowed_providers),
                    agent.context_length_limit,
                    agent.max_output_limit,
                    agent.temperature,
                    agent.auto_format_convert as i64
                ],
            )?;
            Ok(())
        })?;
        self.add_log("info", "Agent", &format!("Updated agent {id}"))
    }

    fn logs(&self) -> CoreResult<Vec<LogEntry>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, timestamp, level, source, message FROM proxy_logs ORDER BY timestamp DESC LIMIT 250",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok(LogEntry {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    level: row.get(2)?,
                    source: row.get(3)?,
                    message: row.get(4)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(CoreError::Database)
        })
    }

    pub fn add_log(&self, level: &str, source: &str, message: &str) -> CoreResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO proxy_logs (id, timestamp, level, source, message) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![Uuid::new_v4().to_string(), Utc::now().to_rfc3339(), level, source, message],
            )?;
            conn.execute(
                "DELETE FROM proxy_logs WHERE id NOT IN (
                    SELECT id FROM proxy_logs ORDER BY timestamp DESC LIMIT 250
                )",
                [],
            )?;
            Ok(())
        })
    }

    fn clear_logs(&self) -> CoreResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM proxy_logs", [])?;
            Ok(())
        })?;
        self.add_log("info", "Logs", "Cleared local logs")
    }

    fn gateway_users(&self) -> CoreResult<Vec<GatewayUser>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, display_name, email, status, created_at
                 FROM gateway_users ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], gateway_user_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(CoreError::Database)
        })
    }

    fn add_gateway_user(&self, body: Value) -> CoreResult<GatewayUser> {
        let display_name = body
            .get("displayName")
            .or_else(|| body.get("display_name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CoreError::Message("displayName is required".into()))?
            .to_string();
        let email = body
            .get("email")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let status = body
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("active")
            .to_string();
        let user = GatewayUser {
            id: format!("usr_{}", compact_uuid()),
            display_name,
            email,
            status,
            created_at: Utc::now().to_rfc3339(),
        };
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO gateway_users (id, display_name, email, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    user.id,
                    user.display_name,
                    user.email,
                    user.status,
                    user.created_at
                ],
            )?;
            Ok(())
        })?;
        self.add_log(
            "info",
            "TenantAdmin",
            &format!("Created gateway user {}", user.display_name),
        )?;
        Ok(user)
    }

    fn update_gateway_user(&self, id: &str, body: Value) -> CoreResult<GatewayUser> {
        let mut user = self
            .gateway_user(id)?
            .ok_or_else(|| CoreError::Message(format!("gateway user not found: {id}")))?;
        if let Some(display_name) = body
            .get("displayName")
            .or_else(|| body.get("display_name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            user.display_name = display_name.to_string();
        }
        if body.get("email").is_some() {
            user.email = body
                .get("email")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        if let Some(status) = body.get("status").and_then(Value::as_str) {
            user.status = status.to_string();
        }
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE gateway_users SET display_name = ?2, email = ?3, status = ?4 WHERE id = ?1",
                params![id, user.display_name, user.email, user.status],
            )?;
            Ok(())
        })?;
        Ok(user)
    }

    fn gateway_user(&self, id: &str) -> CoreResult<Option<GatewayUser>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, display_name, email, status, created_at FROM gateway_users WHERE id = ?1",
                [id],
                gateway_user_from_row,
            )
            .optional()
            .map_err(CoreError::Database)
        })
    }

    fn gateway_projects(&self) -> CoreResult<Vec<GatewayProject>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT p.id, p.user_id, u.display_name, p.name, p.slug, p.status, p.created_at
                 FROM gateway_projects p
                 JOIN gateway_users u ON u.id = p.user_id
                 ORDER BY p.created_at DESC",
            )?;
            let rows = stmt.query_map([], gateway_project_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(CoreError::Database)
        })
    }

    fn add_gateway_project(&self, body: Value) -> CoreResult<GatewayProject> {
        let user_id = body
            .get("userId")
            .or_else(|| body.get("user_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Message("userId is required".into()))?
            .to_string();
        let user = self
            .gateway_user(&user_id)?
            .ok_or_else(|| CoreError::Message(format!("gateway user not found: {user_id}")))?;
        let name = body
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CoreError::Message("name is required".into()))?
            .to_string();
        let slug = body
            .get("slug")
            .and_then(Value::as_str)
            .map(slugify)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| slugify(&name));
        let status = body
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("active")
            .to_string();
        let project = GatewayProject {
            id: format!("prj_{}", compact_uuid()),
            user_id: user.id,
            user_name: user.display_name,
            name,
            slug,
            status,
            created_at: Utc::now().to_rfc3339(),
        };
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO gateway_projects (id, user_id, name, slug, status, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    project.id,
                    project.user_id,
                    project.name,
                    project.slug,
                    project.status,
                    project.created_at
                ],
            )?;
            Ok(())
        })?;
        self.add_log(
            "info",
            "TenantAdmin",
            &format!("Created gateway project {}", project.name),
        )?;
        Ok(project)
    }

    fn update_gateway_project(&self, id: &str, body: Value) -> CoreResult<GatewayProject> {
        let mut project = self
            .gateway_project(id)?
            .ok_or_else(|| CoreError::Message(format!("gateway project not found: {id}")))?;
        if let Some(name) = body
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            project.name = name.to_string();
        }
        if let Some(slug) = body.get("slug").and_then(Value::as_str) {
            project.slug = slugify(slug);
        }
        if let Some(status) = body.get("status").and_then(Value::as_str) {
            project.status = status.to_string();
        }
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE gateway_projects SET name = ?2, slug = ?3, status = ?4 WHERE id = ?1",
                params![id, project.name, project.slug, project.status],
            )?;
            Ok(())
        })?;
        Ok(project)
    }

    fn gateway_project(&self, id: &str) -> CoreResult<Option<GatewayProject>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT p.id, p.user_id, u.display_name, p.name, p.slug, p.status, p.created_at
                 FROM gateway_projects p
                 JOIN gateway_users u ON u.id = p.user_id
                 WHERE p.id = ?1",
                [id],
                gateway_project_from_row,
            )
            .optional()
            .map_err(CoreError::Database)
        })
    }

    fn gateway_keys(&self) -> CoreResult<Vec<GatewayKey>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT k.id, k.key_prefix, k.name, k.user_id, u.display_name, k.project_id,
                    p.name, k.status, k.quota_requests, k.quota_tokens, k.used_requests,
                    k.used_tokens, k.expires_at, k.last_used_at, k.created_at
                 FROM gateway_keys k
                 JOIN gateway_users u ON u.id = k.user_id
                 JOIN gateway_projects p ON p.id = k.project_id
                 ORDER BY k.created_at DESC",
            )?;
            let rows = stmt.query_map([], gateway_key_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(CoreError::Database)
        })
    }

    fn add_gateway_key(&self, body: Value) -> CoreResult<(GatewayKey, String)> {
        let user_id = body
            .get("userId")
            .or_else(|| body.get("user_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Message("userId is required".into()))?
            .to_string();
        let project_id = body
            .get("projectId")
            .or_else(|| body.get("project_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| CoreError::Message("projectId is required".into()))?
            .to_string();
        let user = self
            .gateway_user(&user_id)?
            .ok_or_else(|| CoreError::Message(format!("gateway user not found: {user_id}")))?;
        let project = self.gateway_project(&project_id)?.ok_or_else(|| {
            CoreError::Message(format!("gateway project not found: {project_id}"))
        })?;
        if project.user_id != user.id {
            return Err(CoreError::Message(
                "project must belong to the selected user".into(),
            ));
        }
        let name = body
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Gateway Key")
            .to_string();
        let quota_requests = body
            .get("quotaRequests")
            .or_else(|| body.get("quota_requests"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0);
        let quota_tokens = body
            .get("quotaTokens")
            .or_else(|| body.get("quota_tokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0)
            .max(0);
        let status = body
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("active")
            .to_string();
        let expires_at = body
            .get("expiresAt")
            .or_else(|| body.get("expires_at"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let raw_key = format!("csw_{}", compact_uuid());
        let key_prefix = raw_key.chars().take(12).collect::<String>();
        let key = GatewayKey {
            id: format!("key_{}", compact_uuid()),
            key_prefix,
            name,
            user_id: user.id,
            user_name: user.display_name,
            project_id: project.id,
            project_name: project.name,
            status,
            quota_requests,
            quota_tokens,
            used_requests: 0,
            used_tokens: 0,
            expires_at,
            last_used_at: None,
            created_at: Utc::now().to_rfc3339(),
        };
        let key_hash = hash_gateway_key(&raw_key);
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO gateway_keys (
                    id, key_hash, key_prefix, name, user_id, project_id, status,
                    quota_requests, quota_tokens, used_requests, used_tokens,
                    expires_at, last_used_at, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 0, ?10, NULL, ?11)",
                params![
                    key.id,
                    key_hash,
                    key.key_prefix,
                    key.name,
                    key.user_id,
                    key.project_id,
                    key.status,
                    key.quota_requests,
                    key.quota_tokens,
                    key.expires_at,
                    key.created_at
                ],
            )?;
            Ok(())
        })?;
        self.add_log(
            "info",
            "TenantAdmin",
            &format!("Issued gateway key {} for {}", key.name, key.project_name),
        )?;
        Ok((key, raw_key))
    }

    fn update_gateway_key(&self, id: &str, body: Value) -> CoreResult<GatewayKey> {
        let mut key = self
            .gateway_key(id)?
            .ok_or_else(|| CoreError::Message(format!("gateway key not found: {id}")))?;
        if let Some(name) = body
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            key.name = name.to_string();
        }
        if let Some(status) = body.get("status").and_then(Value::as_str) {
            key.status = status.to_string();
        }
        if let Some(value) = body
            .get("quotaRequests")
            .or_else(|| body.get("quota_requests"))
            .and_then(Value::as_i64)
        {
            key.quota_requests = value.max(0);
        }
        if let Some(value) = body
            .get("quotaTokens")
            .or_else(|| body.get("quota_tokens"))
            .and_then(Value::as_i64)
        {
            key.quota_tokens = value.max(0);
        }
        if body.get("expiresAt").is_some() || body.get("expires_at").is_some() {
            key.expires_at = body
                .get("expiresAt")
                .or_else(|| body.get("expires_at"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
        }
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE gateway_keys SET name = ?2, status = ?3, quota_requests = ?4,
                    quota_tokens = ?5, expires_at = ?6 WHERE id = ?1",
                params![
                    id,
                    key.name,
                    key.status,
                    key.quota_requests,
                    key.quota_tokens,
                    key.expires_at
                ],
            )?;
            Ok(())
        })?;
        self.gateway_key(id)?
            .ok_or_else(|| CoreError::Message(format!("gateway key not found: {id}")))
    }

    fn revoke_gateway_key(&self, id: &str) -> CoreResult<GatewayKey> {
        self.update_gateway_key(id, json!({ "status": "revoked" }))
    }

    fn gateway_key(&self, id: &str) -> CoreResult<Option<GatewayKey>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT k.id, k.key_prefix, k.name, k.user_id, u.display_name, k.project_id,
                    p.name, k.status, k.quota_requests, k.quota_tokens, k.used_requests,
                    k.used_tokens, k.expires_at, k.last_used_at, k.created_at
                 FROM gateway_keys k
                 JOIN gateway_users u ON u.id = k.user_id
                 JOIN gateway_projects p ON p.id = k.project_id
                 WHERE k.id = ?1",
                [id],
                gateway_key_from_row,
            )
            .optional()
            .map_err(CoreError::Database)
        })
    }

    fn gateway_key_count(&self) -> CoreResult<i64> {
        self.with_conn(|conn| {
            conn.query_row("SELECT COUNT(*) FROM gateway_keys", [], |row| row.get(0))
                .map_err(CoreError::Database)
        })
    }

    fn principal_for_gateway_key(&self, raw_key: &str) -> CoreResult<Option<GatewayPrincipal>> {
        let key_hash = hash_gateway_key(raw_key);
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT k.id, k.name, k.user_id, u.display_name, k.project_id, p.name,
                    p.slug, k.quota_requests, k.quota_tokens, k.used_requests, k.used_tokens
                 FROM gateway_keys k
                 JOIN gateway_users u ON u.id = k.user_id
                 JOIN gateway_projects p ON p.id = k.project_id
                 WHERE k.key_hash = ?1
                    AND k.status = 'active'
                    AND u.status = 'active'
                    AND p.status = 'active'
                    AND (k.expires_at IS NULL OR k.expires_at > ?2)",
                params![key_hash, now],
                |row| {
                    Ok(GatewayPrincipal {
                        key_id: row.get(0)?,
                        key_name: row.get(1)?,
                        user_id: row.get(2)?,
                        user_name: row.get(3)?,
                        project_id: row.get(4)?,
                        project_name: row.get(5)?,
                        project_slug: row.get(6)?,
                        quota_requests: row.get(7)?,
                        quota_tokens: row.get(8)?,
                        used_requests: row.get(9)?,
                        used_tokens: row.get(10)?,
                    })
                },
            )
            .optional()
            .map_err(CoreError::Database)
        })
    }

    fn record_gateway_usage(
        &self,
        principal: &GatewayPrincipal,
        usage: UsageEstimate,
        count_request: bool,
    ) -> CoreResult<()> {
        let tokens = usage.total_tokens as i64;
        let requests = if count_request { 1 } else { 0 };
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE gateway_keys SET
                    used_requests = used_requests + ?2,
                    used_tokens = used_tokens + ?3,
                    last_used_at = ?4
                 WHERE id = ?1",
                params![principal.key_id, requests, tokens, Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })
    }

    fn record_gateway_response_usage(
        &self,
        principal: &GatewayPrincipal,
        usage: UsageEstimate,
    ) -> CoreResult<()> {
        self.record_gateway_usage(
            principal,
            UsageEstimate {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
                estimated_cost_usd: 0.0,
            },
            false,
        )
    }
    fn gateway_usage_summary(&self) -> CoreResult<GatewayUsageSummary> {
        let keys = self.gateway_keys()?;
        let traces = self.request_traces(TraceQuery {
            project_id: None,
            limit: Some(5000),
            order: None,
        })?;
        Ok(build_gateway_usage_summary(&traces, &keys))
    }

    fn create_distillation_job(&self, body: Value) -> CoreResult<DistillationJob> {
        let name = body
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Distillation Export")
            .to_string();
        let project_id = body
            .get("projectId")
            .or_else(|| body.get("project_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let user_id = body
            .get("userId")
            .or_else(|| body.get("user_id"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let limit = body
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(1000)
            .clamp(1, 10_000) as usize;
        let include_failures = body
            .get("includeFailures")
            .or_else(|| body.get("include_failures"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let filters = json!({
            "projectId": project_id,
            "userId": user_id,
            "limit": limit,
            "includeFailures": include_failures
        });
        let job_id = format!("dst_{}", compact_uuid());
        let created_at = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO distillation_jobs (
                    id, name, status, project_id, user_id, trace_count,
                    export_path, error, filters_json, created_at, completed_at
                 ) VALUES (?1, ?2, 'running', ?3, ?4, 0, NULL, NULL, ?5, ?6, NULL)",
                params![
                    job_id,
                    name,
                    project_id,
                    user_id,
                    json_string(&filters),
                    created_at
                ],
            )?;
            Ok(())
        })?;

        let result = self.export_distillation_dataset(&job_id, &filters, limit, include_failures);
        match result {
            Ok((trace_count, export_path)) => {
                let completed_at = Utc::now().to_rfc3339();
                self.with_conn(|conn| {
                    conn.execute(
                        "UPDATE distillation_jobs SET status = 'completed', trace_count = ?2,
                            export_path = ?3, error = NULL, completed_at = ?4 WHERE id = ?1",
                        params![job_id, trace_count as i64, export_path, completed_at],
                    )?;
                    Ok(())
                })?;
            }
            Err(err) => {
                let completed_at = Utc::now().to_rfc3339();
                self.with_conn(|conn| {
                    conn.execute(
                        "UPDATE distillation_jobs SET status = 'failed', error = ?2,
                            completed_at = ?3 WHERE id = ?1",
                        params![job_id, err.to_string(), completed_at],
                    )?;
                    Ok(())
                })?;
            }
        }
        self.distillation_job(&job_id)?
            .ok_or_else(|| CoreError::Message(format!("distillation job not found: {job_id}")))
    }

    fn export_distillation_dataset(
        &self,
        job_id: &str,
        filters: &Value,
        limit: usize,
        include_failures: bool,
    ) -> CoreResult<(usize, String)> {
        let project_id = filters
            .get("projectId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let user_id = filters
            .get("userId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let traces = self.request_traces(TraceQuery {
            project_id: None,
            limit: Some(5000),
            order: Some("asc".into()),
        })?;
        let mut lines = Vec::new();
        for trace in traces {
            if let Some(project_id) = project_id.as_ref() {
                let matches_internal = trace.gateway_project_id.as_ref() == Some(project_id);
                let matches_runtime = &trace.project_id == project_id;
                if !matches_internal && !matches_runtime {
                    continue;
                }
            }
            if !include_failures && !trace.success {
                continue;
            }
            if let Some(user_id) = user_id.as_ref() {
                if trace.gateway_user_id.as_ref() != Some(user_id) {
                    continue;
                }
            }
            let Some(request) = trace.request_body_preview.as_ref() else {
                continue;
            };
            let Some(response) = trace.response_body_preview.as_ref() else {
                continue;
            };
            lines.push(json_string(&json!({
                "traceId": trace.id,
                "projectId": trace.gateway_project_id.as_ref().unwrap_or(&trace.project_id),
                "projectName": trace.gateway_project_name.as_ref().unwrap_or(&trace.project_name),
                "userId": trace.gateway_user_id,
                "userName": trace.gateway_user_name,
                "model": trace.request_model,
                "request": request,
                "response": response,
                "promptTokens": trace.prompt_tokens,
                "completionTokens": trace.completion_tokens,
                "totalTokens": trace.total_tokens,
                "estimatedCostUsd": trace.estimated_cost_usd,
                "createdAt": trace.timestamp
            })));
            if lines.len() >= limit {
                break;
            }
        }
        let export_dir = self.app_dir.join("distillation");
        fs::create_dir_all(&export_dir).map_err(|err| CoreError::io(&export_dir, err))?;
        let export_path = export_dir.join(format!("{job_id}.jsonl"));
        fs::write(&export_path, lines.join("\n"))
            .map_err(|err| CoreError::io(&export_path, err))?;
        Ok((lines.len(), export_path.to_string_lossy().to_string()))
    }

    fn distillation_jobs(&self) -> CoreResult<Vec<DistillationJob>> {
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, name, status, project_id, user_id, trace_count, export_path,
                    error, filters_json, created_at, completed_at
                 FROM distillation_jobs ORDER BY created_at DESC",
            )?;
            let rows = stmt.query_map([], distillation_job_from_row)?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(CoreError::Database)
        })
    }

    fn distillation_job(&self, id: &str) -> CoreResult<Option<DistillationJob>> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, status, project_id, user_id, trace_count, export_path,
                    error, filters_json, created_at, completed_at
                 FROM distillation_jobs WHERE id = ?1",
                [id],
                distillation_job_from_row,
            )
            .optional()
            .map_err(CoreError::Database)
        })
    }

    fn telemetry_config(&self) -> CoreResult<TelemetryConfig> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT enabled, capture_bodies, retention_days, max_body_preview_bytes
                 FROM telemetry_config WHERE id = 1",
                [],
                |row| {
                    Ok(TelemetryConfig {
                        enabled: row.get::<_, i64>(0)? != 0,
                        capture_bodies: row.get::<_, i64>(1)? != 0,
                        retention_days: row.get(2)?,
                        max_body_preview_bytes: row.get::<_, i64>(3)?.max(0) as usize,
                    })
                },
            )
            .optional()?
            .map(Ok)
            .unwrap_or_else(|| Ok(TelemetryConfig::default()))
        })
    }

    fn update_telemetry_config(&self, updates: Value) -> CoreResult<TelemetryConfig> {
        let mut config = self.telemetry_config()?;
        if let Some(value) = updates.get("enabled").and_then(Value::as_bool) {
            config.enabled = value;
        }
        if let Some(value) = updates.get("captureBodies").and_then(Value::as_bool) {
            config.capture_bodies = value;
        }
        if let Some(value) = updates.get("retentionDays").and_then(Value::as_i64) {
            config.retention_days = value.clamp(1, 3650);
        }
        if let Some(value) = updates.get("maxBodyPreviewBytes").and_then(Value::as_u64) {
            config.max_body_preview_bytes = (value as usize).clamp(256, 64 * 1024);
        }
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO telemetry_config (
                    id, enabled, capture_bodies, retention_days, max_body_preview_bytes, updated_at
                 ) VALUES (1, ?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                    enabled = excluded.enabled,
                    capture_bodies = excluded.capture_bodies,
                    retention_days = excluded.retention_days,
                    max_body_preview_bytes = excluded.max_body_preview_bytes,
                    updated_at = excluded.updated_at",
                params![
                    config.enabled as i64,
                    config.capture_bodies as i64,
                    config.retention_days,
                    config.max_body_preview_bytes as i64,
                    Utc::now().to_rfc3339()
                ],
            )?;
            Ok(())
        })?;
        self.add_log("info", "Telemetry", "Telemetry capture settings updated")?;
        Ok(config)
    }

    fn add_request_trace(
        &self,
        trace: RequestTraceDraft,
        config: &TelemetryConfig,
    ) -> CoreResult<String> {
        if !config.enabled {
            return Ok(String::new());
        }
        let should_prune = should_prune_telemetry();
        let trace_id = Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO request_traces (
                    id, timestamp, project_sequence, project_id, project_name, workspace_path,
                    git_remote, session_id, protocol, route_path, provider_id, provider_name,
                    service_type, request_model, upstream_url, status_code, success, latency_ms,
                    request_bytes, response_bytes, error, capture_level, request_body_preview,
                    response_body_preview, gateway_user_id, gateway_user_name,
                    gateway_project_id, gateway_project_name, gateway_key_id, gateway_key_name,
                    prompt_tokens, completion_tokens, total_tokens, estimated_cost_usd
                 ) VALUES (
                    ?1, ?2,
                    (SELECT COALESCE(MAX(project_sequence), 0) + 1 FROM request_traces WHERE project_id = ?3),
                    ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                    ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26,
                    ?27, ?28, ?29, ?30, ?31, ?32, ?33
                )",
                params![
                    trace_id.clone(),
                    Utc::now().to_rfc3339(),
                    trace.project_id,
                    trace.project_name,
                    trace.workspace_path,
                    trace.git_remote,
                    trace.session_id,
                    trace.protocol,
                    trace.route_path,
                    trace.provider_id,
                    trace.provider_name,
                    trace.service_type,
                    trace.request_model,
                    trace.upstream_url,
                    trace.status_code,
                    trace.success as i64,
                    trace.latency_ms as i64,
                    trace.request_bytes as i64,
                    trace.response_bytes.map(|value| value as i64),
                    trace.error,
                    trace.capture_level,
                    trace.request_body_preview,
                    trace.response_body_preview,
                    trace.gateway_user_id,
                    trace.gateway_user_name,
                    trace.gateway_project_id,
                    trace.gateway_project_name,
                    trace.gateway_key_id,
                    trace.gateway_key_name,
                    trace.prompt_tokens as i64,
                    trace.completion_tokens as i64,
                    trace.total_tokens as i64,
                    trace.estimated_cost_usd
                ],
            )?;

            if should_prune {
                let threshold = Utc::now() - Duration::days(config.retention_days);
                conn.execute(
                    "DELETE FROM request_traces WHERE timestamp < ?1",
                    [threshold.to_rfc3339()],
                )?;
                conn.execute(
                    "DELETE FROM request_traces WHERE id NOT IN (
                        SELECT id FROM request_traces ORDER BY timestamp DESC LIMIT 10000
                    )",
                    [],
                )?;
            }
            Ok(())
        })?;
        Ok(trace_id)
    }

    fn update_request_trace_response(
        &self,
        trace_id: &str,
        response_bytes: u64,
        response_body_preview: Option<String>,
        usage: UsageEstimate,
    ) -> CoreResult<()> {
        if trace_id.is_empty() {
            return Ok(());
        }
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE request_traces SET
                    response_bytes = ?1,
                    response_body_preview = ?2,
                    capture_level = CASE
                        WHEN ?2 IS NOT NULL OR request_body_preview IS NOT NULL THEN 'body'
                        ELSE capture_level
                    END,
                    prompt_tokens = ?3,
                    completion_tokens = ?4,
                    total_tokens = ?5,
                    estimated_cost_usd = ?6
                 WHERE id = ?7",
                params![
                    response_bytes as i64,
                    response_body_preview,
                    usage.prompt_tokens as i64,
                    usage.completion_tokens as i64,
                    usage.total_tokens as i64,
                    usage.estimated_cost_usd,
                    trace_id
                ],
            )?;
            Ok(())
        })
    }

    fn request_traces(&self, query: TraceQuery) -> CoreResult<Vec<RequestTrace>> {
        let limit = query.limit.unwrap_or(100).clamp(1, 500) as i64;
        let ascending = matches!(query.order.as_deref(), Some("asc" | "ascending"));
        self.with_conn(|conn| {
            let sql = if query.project_id.is_some() && ascending {
                "SELECT id, timestamp, project_sequence, project_id, project_name, workspace_path,
                    git_remote, session_id, protocol, route_path, provider_id, provider_name,
                    service_type, request_model, upstream_url, status_code, success, latency_ms,
                    request_bytes, response_bytes, error, capture_level, request_body_preview,
                    response_body_preview, gateway_user_id, gateway_user_name,
                    gateway_project_id, gateway_project_name, gateway_key_id, gateway_key_name,
                    prompt_tokens, completion_tokens, total_tokens, estimated_cost_usd
                 FROM request_traces WHERE project_id = ?1 ORDER BY project_sequence ASC, timestamp ASC LIMIT ?2"
            } else if query.project_id.is_some() {
                "SELECT id, timestamp, project_sequence, project_id, project_name, workspace_path,
                    git_remote, session_id, protocol, route_path, provider_id, provider_name,
                    service_type, request_model, upstream_url, status_code, success, latency_ms,
                    request_bytes, response_bytes, error, capture_level, request_body_preview,
                    response_body_preview, gateway_user_id, gateway_user_name,
                    gateway_project_id, gateway_project_name, gateway_key_id, gateway_key_name,
                    prompt_tokens, completion_tokens, total_tokens, estimated_cost_usd
                 FROM request_traces WHERE project_id = ?1 ORDER BY timestamp DESC LIMIT ?2"
            } else if ascending {
                "SELECT id, timestamp, project_sequence, project_id, project_name, workspace_path,
                    git_remote, session_id, protocol, route_path, provider_id, provider_name,
                    service_type, request_model, upstream_url, status_code, success, latency_ms,
                    request_bytes, response_bytes, error, capture_level, request_body_preview,
                    response_body_preview, gateway_user_id, gateway_user_name,
                    gateway_project_id, gateway_project_name, gateway_key_id, gateway_key_name,
                    prompt_tokens, completion_tokens, total_tokens, estimated_cost_usd
                 FROM request_traces ORDER BY timestamp ASC LIMIT ?1"
            } else {
                "SELECT id, timestamp, project_sequence, project_id, project_name, workspace_path,
                    git_remote, session_id, protocol, route_path, provider_id, provider_name,
                    service_type, request_model, upstream_url, status_code, success, latency_ms,
                    request_bytes, response_bytes, error, capture_level, request_body_preview,
                    response_body_preview, gateway_user_id, gateway_user_name,
                    gateway_project_id, gateway_project_name, gateway_key_id, gateway_key_name,
                    prompt_tokens, completion_tokens, total_tokens, estimated_cost_usd
                 FROM request_traces ORDER BY timestamp DESC LIMIT ?1"
            };
            let mut stmt = conn.prepare(sql)?;
            if let Some(project_id) = query.project_id {
                let rows = stmt.query_map(params![project_id, limit], request_trace_from_row)?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(CoreError::Database)
            } else {
                let rows = stmt.query_map(params![limit], request_trace_from_row)?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(CoreError::Database)
            }
        })
    }

    fn telemetry_summary(&self) -> CoreResult<TelemetrySummary> {
        let config = self.telemetry_config()?;
        let traces = self.request_traces(TraceQuery {
            project_id: None,
            limit: Some(5000),
            order: None,
        })?;
        Ok(build_telemetry_summary(traces, config))
    }

    fn clear_request_traces(&self) -> CoreResult<()> {
        self.with_conn(|conn| {
            conn.execute("DELETE FROM request_traces", [])?;
            Ok(())
        })?;
        self.add_log("warn", "Telemetry", "Cleared request telemetry traces")
    }

    fn proxy_config(&self) -> CoreResult<ProxyConfig> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT listen_address, listen_port, enabled, max_retries,
                    streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout
                 FROM proxy_config WHERE id = 1",
                [],
                |row| {
                    Ok(ProxyConfig {
                        listen_address: row.get(0)?,
                        listen_port: row.get::<_, i64>(1)? as u16,
                        enabled: row.get::<_, i64>(2)? != 0,
                        max_retries: row.get::<_, i64>(3)? as u8,
                        streaming_first_byte_timeout: row.get::<_, i64>(4)? as u64,
                        streaming_idle_timeout: row.get::<_, i64>(5)? as u64,
                        non_streaming_timeout: row.get::<_, i64>(6)? as u64,
                    })
                },
            )
            .map_err(CoreError::Database)
        })
    }

    fn update_proxy_config(&self, config: &ProxyConfig) -> CoreResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE proxy_config SET listen_address = ?1, listen_port = ?2, enabled = ?3,
                    max_retries = ?4, streaming_first_byte_timeout = ?5, streaming_idle_timeout = ?6,
                    non_streaming_timeout = ?7, updated_at = ?8 WHERE id = 1",
                params![
                    config.listen_address,
                    config.listen_port as i64,
                    config.enabled as i64,
                    config.max_retries as i64,
                    config.streaming_first_byte_timeout as i64,
                    config.streaming_idle_timeout as i64,
                    config.non_streaming_timeout as i64,
                    Utc::now().to_rfc3339()
                ],
            )?;
            Ok(())
        })
    }

    pub fn set_proxy_enabled(&self, enabled: bool) -> CoreResult<()> {
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE proxy_config SET enabled = ?1, updated_at = ?2 WHERE id = 1",
                params![enabled as i64, Utc::now().to_rfc3339()],
            )?;
            Ok(())
        })
    }

    pub fn gateway_state(&self) -> CoreResult<GatewayState> {
        let config = self.proxy_config()?;
        Ok(GatewayState {
            status: if config.enabled { "running" } else { "stopped" }.to_string(),
            uptime_seconds: if config.enabled { 1 } else { 0 },
            cpu_usage: 0,
            memory_usage: 0,
            port: config.listen_port,
        })
    }

    fn record_provider_success(&self, provider_id: &str, latency_ms: u128) -> CoreResult<()> {
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO provider_health (
                    provider_id, is_healthy, consecutive_failures, last_success_at,
                    last_failure_at, last_error, circuit_open_until
                 ) VALUES (?1, 1, 0, ?2, NULL, NULL, NULL)
                 ON CONFLICT(provider_id) DO UPDATE SET
                    is_healthy = 1,
                    consecutive_failures = 0,
                    last_success_at = excluded.last_success_at,
                    last_error = NULL,
                    circuit_open_until = NULL",
                params![provider_id, now],
            )?;
            conn.execute(
                "UPDATE providers SET status = 'connected', latency = ?2 WHERE id = ?1",
                params![provider_id, format!("{latency_ms}ms")],
            )?;
            Ok(())
        })
    }

    fn record_provider_failure(&self, provider_id: &str, error: &str) -> CoreResult<()> {
        let now = Utc::now();
        let (failures, circuit_opened) = self.with_conn(|conn| {
            let existing_failures = conn
                .query_row(
                    "SELECT consecutive_failures FROM provider_health WHERE provider_id = ?1",
                    [provider_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or_default();
            let failures = existing_failures + 1;
            let circuit_open_until = if failures >= CIRCUIT_BREAKER_FAILURE_THRESHOLD {
                Some((now + Duration::seconds(CIRCUIT_BREAKER_OPEN_SECONDS)).to_rfc3339())
            } else {
                None
            };
            conn.execute(
                "INSERT INTO provider_health (
                    provider_id, is_healthy, consecutive_failures, last_success_at,
                    last_failure_at, last_error, circuit_open_until
                 ) VALUES (?1, 0, ?2, NULL, ?3, ?4, ?5)
                 ON CONFLICT(provider_id) DO UPDATE SET
                    is_healthy = 0,
                    consecutive_failures = excluded.consecutive_failures,
                    last_failure_at = excluded.last_failure_at,
                    last_error = excluded.last_error,
                    circuit_open_until = excluded.circuit_open_until",
                params![
                    provider_id,
                    failures,
                    now.to_rfc3339(),
                    error,
                    circuit_open_until
                ],
            )?;
            if circuit_open_until.is_some() {
                conn.execute(
                    "UPDATE providers SET status = 'disconnected' WHERE id = ?1",
                    [provider_id],
                )?;
            }
            Ok((failures, circuit_open_until.is_some()))
        })?;
        if circuit_opened {
            self.add_log(
                "warn",
                "ProviderHealth",
                &format!(
                    "Circuit opened for {provider_id} after {failures} consecutive failures for {CIRCUIT_BREAKER_OPEN_SECONDS}s"
                ),
            )?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn force_open_circuit_for_test(&self, provider_id: &str) -> CoreResult<()> {
        let open_until =
            (Utc::now() + Duration::seconds(CIRCUIT_BREAKER_OPEN_SECONDS)).to_rfc3339();
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO provider_health (
                    provider_id, is_healthy, consecutive_failures, last_failure_at, last_error, circuit_open_until
                 ) VALUES (?1, 0, ?2, ?3, 'forced', ?4)
                 ON CONFLICT(provider_id) DO UPDATE SET
                    is_healthy = 0,
                    consecutive_failures = excluded.consecutive_failures,
                    last_failure_at = excluded.last_failure_at,
                    last_error = excluded.last_error,
                    circuit_open_until = excluded.circuit_open_until",
                params![
                    provider_id,
                    CIRCUIT_BREAKER_FAILURE_THRESHOLD,
                    Utc::now().to_rfc3339(),
                    open_until
                ],
            )?;
            Ok(())
        })
    }

    fn open_circuit_provider_ids(&self) -> CoreResult<Vec<String>> {
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT provider_id FROM provider_health
                 WHERE circuit_open_until IS NOT NULL AND circuit_open_until > ?1",
            )?;
            let rows = stmt.query_map([now], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(CoreError::Database)
        })
    }

    fn providers_for_protocol(
        &self,
        protocol: &str,
        request_model: Option<&str>,
    ) -> CoreResult<Vec<Provider>> {
        let mut candidates = self
            .providers(false)?
            .into_iter()
            .filter(|p| p.status != "disabled")
            .filter(|p| p.status == "connected" || !p.auth_keys().is_empty())
            .filter(|p| protocol_matches(&p.service_type, protocol))
            .collect::<Vec<_>>();

        if candidates.iter().any(|p| p.is_current) {
            candidates.retain(|p| p.is_current || p.failover_enabled);
        }

        let open_circuit_ids = self.open_circuit_provider_ids()?;
        let available = candidates
            .iter()
            .filter(|p| !open_circuit_ids.iter().any(|id| id == &p.id))
            .cloned()
            .collect::<Vec<_>>();
        if !available.is_empty() {
            candidates = available;
        }

        if let Some(model) = request_model {
            let model_provider_ids = self.provider_ids_for_model_reference(model)?;
            candidates.sort_by_key(|p| {
                (
                    if p.status == "connected" { 0 } else { 1 },
                    if p.model_mapping.contains_key(model)
                        || model_provider_ids.iter().any(|id| id == &p.id)
                    {
                        0
                    } else {
                        1
                    },
                    if p.is_current { 0 } else { 1 },
                    p.priority,
                )
            });
        } else {
            candidates.sort_by_key(|p| {
                (
                    if p.status == "connected" { 0 } else { 1 },
                    if p.is_current { 0 } else { 1 },
                    p.priority,
                )
            });
        }

        Ok(candidates)
    }
}

fn seed_defaults(conn: &Connection) -> CoreResult<()> {
    let providers = vec![
        Provider {
            id: "claude-official".into(),
            app_type: "claude".into(),
            name: "Anthropic Official".into(),
            provider_type: "anthropic".into(),
            service_type: "claude".into(),
            base_url: "https://api.anthropic.com".into(),
            status: "disconnected".into(),
            priority: 10,
            ..default_provider()
        },
        Provider {
            id: "codex-openai".into(),
            app_type: "codex".into(),
            name: "OpenAI / Codex".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: "https://api.openai.com".into(),
            status: "disconnected".into(),
            priority: 20,
            ..default_provider()
        },
        Provider {
            id: "gemini-official".into(),
            app_type: "gemini".into(),
            name: "Google Gemini".into(),
            provider_type: "gemini".into(),
            service_type: "gemini".into(),
            base_url: "https://generativelanguage.googleapis.com".into(),
            status: "disconnected".into(),
            priority: 30,
            ..default_provider()
        },
    ];
    for provider in providers {
        insert_provider(conn, &provider)?;
    }

    for model in [
        Model {
            id: "claude-sonnet-4-5".into(),
            name: "claude-sonnet-4-5".into(),
            provider_id: "claude-official".into(),
            tags: vec!["Claude".into(), "Coding".into()],
            is_default: true,
            context_window: "200K tokens".into(),
            latency: "".into(),
            max_output: "8192 tokens".into(),
            supports_images: true,
        },
        Model {
            id: "gpt-5.4".into(),
            name: "gpt-5.4".into(),
            provider_id: "codex-openai".into(),
            tags: vec!["Codex".into(), "Responses".into()],
            is_default: true,
            context_window: "400K tokens".into(),
            latency: "".into(),
            max_output: "128K tokens".into(),
            supports_images: true,
        },
        Model {
            id: "gemini-2.5-pro".into(),
            name: "gemini-2.5-pro".into(),
            provider_id: "gemini-official".into(),
            tags: vec!["Gemini".into(), "Long Context".into()],
            is_default: true,
            context_window: "2M tokens".into(),
            latency: "".into(),
            max_output: "8192 tokens".into(),
            supports_images: true,
        },
    ] {
        insert_model(conn, &model)?;
    }

    for agent in default_agents() {
        insert_agent(conn, &agent)?;
    }

    conn.execute(
        "INSERT INTO proxy_logs (id, timestamp, level, source, message) VALUES (?1, ?2, 'info', 'Gateway', 'codexswitch core initialized on 127.0.0.1:3900')",
        params![Uuid::new_v4().to_string(), Utc::now().to_rfc3339()],
    )?;
    Ok(())
}

fn ensure_default_agents(conn: &Connection) -> CoreResult<()> {
    for agent in default_agents() {
        insert_agent_if_missing(conn, &agent)?;
    }
    Ok(())
}

fn seed_app_settings(conn: &Connection) -> CoreResult<()> {
    let now = Utc::now().to_rfc3339();
    for (key, value) in [
        ("ui.language", json!("en")),
        ("ui.theme", json!("dark")),
        ("app.version", json!(env!("CARGO_PKG_VERSION"))),
    ] {
        conn.execute(
            "INSERT OR IGNORE INTO app_settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)",
            params![key, json_string(&value), now],
        )?;
    }
    Ok(())
}

fn default_provider() -> Provider {
    Provider {
        id: String::new(),
        app_type: default_app_type(),
        name: String::new(),
        provider_type: default_provider_type(),
        service_type: String::new(),
        base_url: String::new(),
        base_urls: Vec::new(),
        api_key: String::new(),
        api_keys: Vec::new(),
        status: default_status(),
        latency: None,
        description: None,
        skip_tls: false,
        low_quality: false,
        auto_blacklist: false,
        codex_passthrough: false,
        codex_tools_compat: false,
        normalize_user_id: false,
        normalize_chat_roles: false,
        reasoning_style: None,
        custom_headers: Vec::new(),
        proxy_url: None,
        route_prefix: None,
        model_mapping: BTreeMap::new(),
        priority: 100,
        is_current: false,
        failover_enabled: true,
    }
}

fn default_agents() -> Vec<Agent> {
    vec![
        Agent {
            id: "claude-code".into(),
            name: "Claude Code".into(),
            desc: "Claude Code local CLI configuration target.".into(),
            provider_id: "claude-official".into(),
            model_id: "claude-sonnet-4-5".into(),
            status: "idle".into(),
            last_run: None,
            metrics: json!({ "tokensSpent": 0, "avgLatency": "", "callsCount": 0 }),
            allowed_providers: vec!["claude-official".into()],
            context_length_limit: Some(200),
            max_output_limit: Some(8192),
            temperature: Some(0.5),
            auto_format_convert: true,
        },
        Agent {
            id: "codex".into(),
            name: "Codex Workspace CLI".into(),
            desc: "Codex local CLI configuration target.".into(),
            provider_id: "codex-openai".into(),
            model_id: "gpt-5.4".into(),
            status: "idle".into(),
            last_run: None,
            metrics: json!({ "tokensSpent": 0, "avgLatency": "", "callsCount": 0 }),
            allowed_providers: vec!["codex-openai".into()],
            context_length_limit: Some(400),
            max_output_limit: Some(32768),
            temperature: Some(0.7),
            auto_format_convert: true,
        },
        Agent {
            id: "codex-desktop".into(),
            name: "Codex Desktop".into(),
            desc: "Codex macOS desktop app target using the shared local Codex config and proxy routing.".into(),
            provider_id: "codex-openai".into(),
            model_id: "gpt-5.4".into(),
            status: "idle".into(),
            last_run: None,
            metrics: json!({ "tokensSpent": 0, "avgLatency": "", "callsCount": 0 }),
            allowed_providers: vec!["codex-openai".into()],
            context_length_limit: Some(400),
            max_output_limit: Some(32768),
            temperature: Some(0.7),
            auto_format_convert: true,
        },
        Agent {
            id: "gemini-cli".into(),
            name: "Gemini CLI".into(),
            desc: "Gemini CLI local configuration target.".into(),
            provider_id: "gemini-official".into(),
            model_id: "gemini-2.5-pro".into(),
            status: "idle".into(),
            last_run: None,
            metrics: json!({ "tokensSpent": 0, "avgLatency": "", "callsCount": 0 }),
            allowed_providers: vec!["gemini-official".into()],
            context_length_limit: Some(1024),
            max_output_limit: Some(8192),
            temperature: Some(0.2),
            auto_format_convert: true,
        },
    ]
}

fn insert_provider(conn: &Connection, provider: &Provider) -> CoreResult<()> {
    let mut provider = provider.clone();
    if provider.service_type.is_empty() {
        provider.service_type = infer_service_type(&provider.provider_type);
    }
    if provider.app_type.is_empty() {
        provider.app_type = infer_app_type(&provider.service_type);
    }
    let api_keys = if provider.api_keys.is_empty() && !provider.api_key.is_empty() {
        vec![provider.api_key.clone()]
    } else {
        provider.api_keys.clone()
    };
    let base_urls = if provider.base_urls.is_empty() && !provider.base_url.is_empty() {
        vec![provider.base_url.clone()]
    } else {
        provider.base_urls.clone()
    };
    provider.api_keys = api_keys;
    provider.base_urls = base_urls;

    conn.execute(
        "INSERT INTO providers (
            id, app_type, name, provider_type, service_type, base_url, base_urls_json,
            api_key, api_keys_json, status, latency, description, skip_tls, low_quality,
            auto_blacklist, codex_passthrough, codex_tools_compat, normalize_user_id,
            normalize_chat_roles, reasoning_style, custom_headers_json, proxy_url,
            route_prefix, model_mapping_json, priority, is_current, failover_enabled, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28)",
        params![
            provider.id,
            provider.app_type,
            provider.name,
            provider.provider_type,
            provider.service_type,
            provider.base_url,
            json_string(&provider.base_urls),
            provider.api_key,
            json_string(&provider.api_keys),
            provider.status,
            provider.latency,
            provider.description,
            provider.skip_tls as i64,
            provider.low_quality as i64,
            provider.auto_blacklist as i64,
            provider.codex_passthrough as i64,
            provider.codex_tools_compat as i64,
            provider.normalize_user_id as i64,
            provider.normalize_chat_roles as i64,
            provider.reasoning_style,
            json_string(&provider.custom_headers),
            provider.proxy_url,
            provider.route_prefix,
            json_string(&provider.model_mapping),
            provider.priority,
            provider.is_current as i64,
            provider.failover_enabled as i64,
            Utc::now().to_rfc3339()
        ],
    )?;
    sync_provider_children(conn, &provider)?;
    sync_current_provider_marker(conn, &provider)?;
    Ok(())
}

fn update_provider_row(conn: &Connection, id: &str, provider: &Provider) -> CoreResult<()> {
    conn.execute(
        "UPDATE providers SET app_type = ?2, name = ?3, provider_type = ?4, service_type = ?5,
            base_url = ?6, base_urls_json = ?7, api_key = ?8, api_keys_json = ?9, status = ?10,
            latency = ?11, description = ?12, skip_tls = ?13, low_quality = ?14,
            auto_blacklist = ?15, codex_passthrough = ?16, codex_tools_compat = ?17,
            normalize_user_id = ?18, normalize_chat_roles = ?19, reasoning_style = ?20,
            custom_headers_json = ?21, proxy_url = ?22, route_prefix = ?23,
            model_mapping_json = ?24, priority = ?25, is_current = ?26, failover_enabled = ?27
         WHERE id = ?1",
        params![
            id,
            provider.app_type,
            provider.name,
            provider.provider_type,
            provider.service_type,
            provider.base_url,
            json_string(&provider.base_urls),
            provider.api_key,
            json_string(&provider.api_keys),
            provider.status,
            provider.latency,
            provider.description,
            provider.skip_tls as i64,
            provider.low_quality as i64,
            provider.auto_blacklist as i64,
            provider.codex_passthrough as i64,
            provider.codex_tools_compat as i64,
            provider.normalize_user_id as i64,
            provider.normalize_chat_roles as i64,
            provider.reasoning_style,
            json_string(&provider.custom_headers),
            provider.proxy_url,
            provider.route_prefix,
            json_string(&provider.model_mapping),
            provider.priority,
            provider.is_current as i64,
            provider.failover_enabled as i64
        ],
    )?;
    Ok(())
}

fn provider_from_row(row: &rusqlite::Row<'_>, mask: bool) -> rusqlite::Result<Provider> {
    let raw_key: String = row.get(7)?;
    let mut keys: Vec<String> = parse_json(row.get::<_, String>(8)?, Vec::new());
    if keys.is_empty() && !raw_key.is_empty() {
        keys.push(raw_key.clone());
    }
    let api_key = if mask { mask_secret(&raw_key) } else { raw_key };
    if mask {
        keys = keys.into_iter().map(|key| mask_secret(&key)).collect();
    }
    Ok(Provider {
        id: row.get(0)?,
        app_type: row.get(1)?,
        name: row.get(2)?,
        provider_type: row.get(3)?,
        service_type: row.get(4)?,
        base_url: row.get(5)?,
        base_urls: parse_json(row.get::<_, String>(6)?, Vec::new()),
        api_key,
        api_keys: keys,
        status: row.get(9)?,
        latency: row.get(10)?,
        description: row.get(11)?,
        skip_tls: row.get::<_, i64>(12)? != 0,
        low_quality: row.get::<_, i64>(13)? != 0,
        auto_blacklist: row.get::<_, i64>(14)? != 0,
        codex_passthrough: row.get::<_, i64>(15)? != 0,
        codex_tools_compat: row.get::<_, i64>(16)? != 0,
        normalize_user_id: row.get::<_, i64>(17)? != 0,
        normalize_chat_roles: row.get::<_, i64>(18)? != 0,
        reasoning_style: row.get(19)?,
        custom_headers: parse_json(row.get::<_, String>(20)?, Vec::new()),
        proxy_url: row.get(21)?,
        route_prefix: row.get(22)?,
        model_mapping: parse_json(row.get::<_, String>(23)?, BTreeMap::new()),
        priority: row.get(24)?,
        is_current: row.get::<_, i64>(25)? != 0,
        failover_enabled: row.get::<_, i64>(26)? != 0,
    })
}

fn list_providers_raw(conn: &Connection) -> CoreResult<Vec<Provider>> {
    let mut stmt = conn.prepare(
        "SELECT id, app_type, name, provider_type, service_type, base_url, base_urls_json,
            api_key, api_keys_json, status, latency, description, skip_tls, low_quality,
            auto_blacklist, codex_passthrough, codex_tools_compat, normalize_user_id,
            normalize_chat_roles, reasoning_style, custom_headers_json, proxy_url,
            route_prefix, model_mapping_json, priority, is_current, failover_enabled
         FROM providers ORDER BY priority ASC, created_at ASC",
    )?;
    let rows = stmt.query_map([], |row| provider_from_row(row, false))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(CoreError::Database)
}

fn sync_provider_children(conn: &Connection, provider: &Provider) -> CoreResult<()> {
    conn.execute(
        "DELETE FROM provider_endpoints WHERE provider_id = ?1",
        [&provider.id],
    )?;
    conn.execute(
        "DELETE FROM provider_keys WHERE provider_id = ?1",
        [&provider.id],
    )?;

    let base_urls = if provider.base_urls.is_empty() && !provider.base_url.is_empty() {
        vec![provider.base_url.clone()]
    } else {
        provider.base_urls.clone()
    };
    for (position, base_url) in base_urls.iter().enumerate() {
        conn.execute(
            "INSERT INTO provider_endpoints (provider_id, position, base_url) VALUES (?1, ?2, ?3)",
            params![provider.id, position as i64, base_url],
        )?;
    }

    let api_keys = provider.auth_keys();
    for (position, key) in api_keys.iter().enumerate() {
        conn.execute(
            "INSERT INTO provider_keys (provider_id, position, key_value) VALUES (?1, ?2, ?3)",
            params![provider.id, position as i64, key],
        )?;
    }
    Ok(())
}

fn sync_current_provider_marker(conn: &Connection, provider: &Provider) -> CoreResult<()> {
    if provider.is_current {
        conn.execute(
            "INSERT INTO current_providers (app_type, provider_id, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(app_type) DO UPDATE SET
                provider_id = excluded.provider_id,
                updated_at = excluded.updated_at",
            params![provider.app_type, provider.id, Utc::now().to_rfc3339()],
        )?;
    } else {
        conn.execute(
            "DELETE FROM current_providers WHERE app_type = ?1 AND provider_id = ?2",
            params![provider.app_type, provider.id],
        )?;
    }
    Ok(())
}

fn provider_health_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ProviderHealth> {
    Ok(ProviderHealth {
        provider_id: row.get(0)?,
        is_healthy: row.get::<_, i64>(1)? != 0,
        consecutive_failures: row.get(2)?,
        last_success_at: row.get(3)?,
        last_failure_at: row.get(4)?,
        last_error: row.get(5)?,
        circuit_open_until: row.get(6)?,
    })
}

fn gateway_user_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GatewayUser> {
    Ok(GatewayUser {
        id: row.get(0)?,
        display_name: row.get(1)?,
        email: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
    })
}

fn gateway_project_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GatewayProject> {
    Ok(GatewayProject {
        id: row.get(0)?,
        user_id: row.get(1)?,
        user_name: row.get(2)?,
        name: row.get(3)?,
        slug: row.get(4)?,
        status: row.get(5)?,
        created_at: row.get(6)?,
    })
}

fn gateway_key_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GatewayKey> {
    Ok(GatewayKey {
        id: row.get(0)?,
        key_prefix: row.get(1)?,
        name: row.get(2)?,
        user_id: row.get(3)?,
        user_name: row.get(4)?,
        project_id: row.get(5)?,
        project_name: row.get(6)?,
        status: row.get(7)?,
        quota_requests: row.get(8)?,
        quota_tokens: row.get(9)?,
        used_requests: row.get(10)?,
        used_tokens: row.get(11)?,
        expires_at: row.get(12)?,
        last_used_at: row.get(13)?,
        created_at: row.get(14)?,
    })
}

fn distillation_job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DistillationJob> {
    Ok(DistillationJob {
        id: row.get(0)?,
        name: row.get(1)?,
        status: row.get(2)?,
        project_id: row.get(3)?,
        user_id: row.get(4)?,
        trace_count: row.get::<_, i64>(5)?.max(0) as usize,
        export_path: row.get(6)?,
        error: row.get(7)?,
        filters: parse_json(row.get::<_, String>(8)?, json!({})),
        created_at: row.get(9)?,
        completed_at: row.get(10)?,
    })
}

fn request_trace_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RequestTrace> {
    Ok(RequestTrace {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        project_sequence: row.get(2)?,
        project_id: row.get(3)?,
        project_name: row.get(4)?,
        workspace_path: row.get(5)?,
        git_remote: row.get(6)?,
        session_id: row.get(7)?,
        protocol: row.get(8)?,
        route_path: row.get(9)?,
        provider_id: row.get(10)?,
        provider_name: row.get(11)?,
        service_type: row.get(12)?,
        request_model: row.get(13)?,
        upstream_url: row.get(14)?,
        status_code: row.get(15)?,
        success: row.get::<_, i64>(16)? != 0,
        latency_ms: row.get::<_, i64>(17)?.max(0) as u64,
        request_bytes: row.get::<_, i64>(18)?.max(0) as u64,
        response_bytes: row
            .get::<_, Option<i64>>(19)?
            .map(|value| value.max(0) as u64),
        error: row.get(20)?,
        capture_level: row.get(21)?,
        request_body_preview: row.get(22)?,
        response_body_preview: row.get(23)?,
        gateway_user_id: row.get(24)?,
        gateway_user_name: row.get(25)?,
        gateway_project_id: row.get(26)?,
        gateway_project_name: row.get(27)?,
        gateway_key_id: row.get(28)?,
        gateway_key_name: row.get(29)?,
        prompt_tokens: row.get::<_, i64>(30)?.max(0) as u64,
        completion_tokens: row.get::<_, i64>(31)?.max(0) as u64,
        total_tokens: row.get::<_, i64>(32)?.max(0) as u64,
        estimated_cost_usd: row.get::<_, f64>(33)?.max(0.0),
    })
}

#[derive(Default)]
struct TelemetryBucket {
    name: String,
    requests: usize,
    successes: usize,
    total_latency: u64,
}

fn build_telemetry_summary(traces: Vec<RequestTrace>, config: TelemetryConfig) -> TelemetrySummary {
    let total_requests = traces.len();
    let successes = traces.iter().filter(|trace| trace.success).count();
    let total_latency = traces.iter().map(|trace| trace.latency_ms).sum::<u64>();
    let mut projects: HashMap<String, TelemetryBucket> = HashMap::new();
    let mut providers: HashMap<String, TelemetryBucket> = HashMap::new();
    let mut models: HashMap<String, TelemetryBucket> = HashMap::new();

    for trace in &traces {
        add_telemetry_bucket(
            &mut projects,
            &trace.project_id,
            &trace.project_name,
            trace.success,
            trace.latency_ms,
        );
        add_telemetry_bucket(
            &mut providers,
            &trace.provider_id,
            &trace.provider_name,
            trace.success,
            trace.latency_ms,
        );
        if let Some(model) = trace
            .request_model
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            add_telemetry_bucket(&mut models, model, model, trace.success, trace.latency_ms);
        }
    }

    let by_project = sort_project_summaries(projects);
    let by_provider = sort_provider_summaries(providers);
    let by_model = sort_model_summaries(models);
    let recent_errors = traces
        .iter()
        .filter(|trace| !trace.success)
        .take(10)
        .cloned()
        .collect::<Vec<_>>();
    let insights = distill_telemetry_insights(&by_project, &by_provider, &by_model, &recent_errors);

    TelemetrySummary {
        total_requests,
        total_projects: by_project.len(),
        success_rate: ratio(successes, total_requests),
        avg_latency_ms: average_latency(total_latency, total_requests),
        capture_bodies: config.capture_bodies,
        retention_days: config.retention_days,
        by_project,
        by_provider,
        by_model,
        recent_errors,
        insights,
    }
}

fn add_telemetry_bucket(
    buckets: &mut HashMap<String, TelemetryBucket>,
    key: &str,
    name: &str,
    success: bool,
    latency_ms: u64,
) {
    let bucket = buckets
        .entry(key.to_string())
        .or_insert_with(|| TelemetryBucket {
            name: name.to_string(),
            ..TelemetryBucket::default()
        });
    bucket.requests += 1;
    bucket.successes += usize::from(success);
    bucket.total_latency += latency_ms;
}

fn sort_project_summaries(
    buckets: HashMap<String, TelemetryBucket>,
) -> Vec<TelemetryProjectSummary> {
    let mut summaries = buckets
        .into_iter()
        .map(|(project_id, bucket)| TelemetryProjectSummary {
            project_id,
            project_name: bucket.name,
            requests: bucket.requests,
            success_rate: ratio(bucket.successes, bucket.requests),
            avg_latency_ms: average_latency(bucket.total_latency, bucket.requests),
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|a, b| {
        b.requests
            .cmp(&a.requests)
            .then_with(|| a.project_id.cmp(&b.project_id))
    });
    summaries
}

fn sort_provider_summaries(
    buckets: HashMap<String, TelemetryBucket>,
) -> Vec<TelemetryProviderSummary> {
    let mut summaries = buckets
        .into_iter()
        .map(|(provider_id, bucket)| TelemetryProviderSummary {
            provider_id,
            provider_name: bucket.name,
            requests: bucket.requests,
            success_rate: ratio(bucket.successes, bucket.requests),
            avg_latency_ms: average_latency(bucket.total_latency, bucket.requests),
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|a, b| {
        b.requests
            .cmp(&a.requests)
            .then_with(|| a.provider_id.cmp(&b.provider_id))
    });
    summaries
}

fn sort_model_summaries(buckets: HashMap<String, TelemetryBucket>) -> Vec<TelemetryModelSummary> {
    let mut summaries = buckets
        .into_iter()
        .map(|(model, bucket)| TelemetryModelSummary {
            model,
            requests: bucket.requests,
            success_rate: ratio(bucket.successes, bucket.requests),
            avg_latency_ms: average_latency(bucket.total_latency, bucket.requests),
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|a, b| {
        b.requests
            .cmp(&a.requests)
            .then_with(|| a.model.cmp(&b.model))
    });
    summaries
}

fn distill_telemetry_insights(
    projects: &[TelemetryProjectSummary],
    providers: &[TelemetryProviderSummary],
    models: &[TelemetryModelSummary],
    errors: &[RequestTrace],
) -> Vec<String> {
    let mut insights = Vec::new();
    if let Some(project) = projects.first() {
        insights.push(format!(
            "{} is the busiest project with {} captured requests.",
            project.project_name, project.requests
        ));
    }
    if let Some(provider) = providers
        .iter()
        .filter(|provider| provider.requests >= 3)
        .min_by(|a, b| {
            a.success_rate
                .partial_cmp(&b.success_rate)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    {
        if provider.success_rate < 0.9 {
            insights.push(format!(
                "{} has the weakest success rate at {:.1}%.",
                provider.provider_name,
                provider.success_rate * 100.0
            ));
        }
    }
    if let Some(model) = models.first() {
        insights.push(format!(
            "{} is the most used model route with {} requests.",
            model.model, model.requests
        ));
    }
    if let Some(error) = errors.first() {
        insights.push(format!(
            "Latest failure: {} via {} returned {}.",
            error.route_path,
            error.provider_name,
            error
                .status_code
                .map(|status| status.to_string())
                .unwrap_or_else(|| "an error".into())
        ));
    }
    if insights.is_empty() {
        insights.push("Telemetry is ready. Route traffic through the local proxy to build project-level summaries.".into());
    }
    insights
}

fn ratio(successes: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        successes as f64 / total as f64
    }
}

fn average_latency(total_latency: u64, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        total_latency as f64 / total as f64
    }
}

fn should_prune_telemetry() -> bool {
    let now = Utc::now().timestamp();
    let last = TELEMETRY_LAST_PRUNE_EPOCH.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 60 {
        return false;
    }
    TELEMETRY_LAST_PRUNE_EPOCH
        .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
}

fn ensure_provider_health_columns(conn: &Connection) -> CoreResult<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(provider_health)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let columns = rows.collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|column| column == "circuit_open_until") {
        conn.execute(
            "ALTER TABLE provider_health ADD COLUMN circuit_open_until TEXT",
            [],
        )?;
    }
    Ok(())
}

fn ensure_request_traces_columns(conn: &Connection) -> CoreResult<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(request_traces)")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let columns = rows.collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|column| column == "project_sequence") {
        conn.execute(
            "ALTER TABLE request_traces ADD COLUMN project_sequence INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    for (column, definition) in [
        ("gateway_user_id", "TEXT"),
        ("gateway_user_name", "TEXT"),
        ("gateway_project_id", "TEXT"),
        ("gateway_project_name", "TEXT"),
        ("gateway_key_id", "TEXT"),
        ("gateway_key_name", "TEXT"),
        ("prompt_tokens", "INTEGER NOT NULL DEFAULT 0"),
        ("completion_tokens", "INTEGER NOT NULL DEFAULT 0"),
        ("total_tokens", "INTEGER NOT NULL DEFAULT 0"),
        ("estimated_cost_usd", "REAL NOT NULL DEFAULT 0"),
    ] {
        if !columns.iter().any(|existing| existing == column) {
            conn.execute(
                &format!("ALTER TABLE request_traces ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
    }
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_request_traces_project_sequence
            ON request_traces(project_id, project_sequence ASC)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_request_traces_gateway_key_time
            ON request_traces(gateway_key_id, timestamp DESC)",
        [],
    )?;
    Ok(())
}

fn get_provider(conn: &Connection, id: &str, mask: bool) -> CoreResult<Option<Provider>> {
    conn.query_row(
        "SELECT id, app_type, name, provider_type, service_type, base_url, base_urls_json,
            api_key, api_keys_json, status, latency, description, skip_tls, low_quality,
            auto_blacklist, codex_passthrough, codex_tools_compat, normalize_user_id,
            normalize_chat_roles, reasoning_style, custom_headers_json, proxy_url,
            route_prefix, model_mapping_json, priority, is_current, failover_enabled
         FROM providers WHERE id = ?1",
        [id],
        |row| provider_from_row(row, mask),
    )
    .optional()
    .map_err(CoreError::Database)
}

fn insert_model(conn: &Connection, model: &Model) -> CoreResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO models (id, name, provider_id, tags_json, is_default, context_window, latency, max_output, supports_images)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            model.id,
            model.name,
            model.provider_id,
            json_string(&model.tags),
            model.is_default as i64,
            model.context_window,
            model.latency,
            model.max_output,
            model.supports_images as i64
        ],
    )?;
    Ok(())
}

fn model_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Model> {
    Ok(Model {
        id: row.get(0)?,
        name: row.get(1)?,
        provider_id: row.get(2)?,
        tags: parse_json(row.get::<_, String>(3)?, Vec::new()),
        is_default: row.get::<_, i64>(4)? != 0,
        context_window: row.get(5)?,
        latency: row.get(6)?,
        max_output: row.get(7)?,
        supports_images: row.get::<_, i64>(8)? != 0,
    })
}

fn get_model(conn: &Connection, id: &str) -> CoreResult<Option<Model>> {
    conn.query_row(
        "SELECT id, name, provider_id, tags_json, is_default, context_window, latency, max_output, supports_images FROM models WHERE id = ?1",
        [id],
        model_from_row,
    )
    .optional()
    .map_err(CoreError::Database)
}

fn get_model_by_provider_and_name(
    conn: &Connection,
    provider_id: &str,
    model_name: &str,
) -> CoreResult<Option<Model>> {
    conn.query_row(
        "SELECT id, name, provider_id, tags_json, is_default, context_window, latency, max_output, supports_images
         FROM models
         WHERE provider_id = ?1 AND lower(name) = lower(?2)
         ORDER BY CASE WHEN name = ?2 THEN 0 ELSE 1 END
         LIMIT 1",
        params![provider_id, model_name],
        model_from_row,
    )
    .optional()
    .map_err(CoreError::Database)
}

fn provider_model_record_id(provider_id: &str, model_id: &str) -> String {
    let slug = model_id
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let slug = if slug.is_empty() {
        "model".to_string()
    } else {
        slug
    };
    format!("{provider_id}-{slug}")
}

fn insert_agent(conn: &Connection, agent: &Agent) -> CoreResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO agents (id, name, description, provider_id, model_id, status, last_run,
            metrics_json, allowed_providers_json, context_length_limit, max_output_limit, temperature, auto_format_convert)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            agent.id,
            agent.name,
            agent.desc,
            agent.provider_id,
            agent.model_id,
            agent.status,
            agent.last_run,
            json_string(&agent.metrics),
            json_string(&agent.allowed_providers),
            agent.context_length_limit,
            agent.max_output_limit,
            agent.temperature,
            agent.auto_format_convert as i64
        ],
    )?;
    Ok(())
}

fn insert_agent_if_missing(conn: &Connection, agent: &Agent) -> CoreResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO agents (id, name, description, provider_id, model_id, status, last_run,
            metrics_json, allowed_providers_json, context_length_limit, max_output_limit, temperature, auto_format_convert)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            agent.id,
            agent.name,
            agent.desc,
            agent.provider_id,
            agent.model_id,
            agent.status,
            agent.last_run,
            json_string(&agent.metrics),
            json_string(&agent.allowed_providers),
            agent.context_length_limit,
            agent.max_output_limit,
            agent.temperature,
            agent.auto_format_convert as i64
        ],
    )?;
    Ok(())
}

fn agent_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Agent> {
    Ok(Agent {
        id: row.get(0)?,
        name: row.get(1)?,
        desc: row.get(2)?,
        provider_id: row.get(3)?,
        model_id: row.get(4)?,
        status: row.get(5)?,
        last_run: row.get(6)?,
        metrics: parse_json(row.get::<_, String>(7)?, json!({})),
        allowed_providers: parse_json(row.get::<_, String>(8)?, Vec::new()),
        context_length_limit: row.get(9)?,
        max_output_limit: row.get(10)?,
        temperature: row.get(11)?,
        auto_format_convert: row.get::<_, i64>(12)? != 0,
    })
}

fn get_agent(conn: &Connection, id: &str) -> CoreResult<Option<Agent>> {
    conn.query_row(
        "SELECT id, name, description, provider_id, model_id, status, last_run, metrics_json,
            allowed_providers_json, context_length_limit, max_output_limit, temperature, auto_format_convert
         FROM agents WHERE id = ?1",
        [id],
        agent_from_row,
    )
    .optional()
    .map_err(CoreError::Database)
}

async fn health(State(state): State<SharedState>) -> CoreResult<Json<Value>> {
    let proxy = state.database.proxy_config()?;
    Ok(Json(json!({
        "ok": true,
        "version": env!("CARGO_PKG_VERSION"),
        "proxy": {
            "enabled": proxy.enabled,
            "listenAddress": proxy.listen_address,
            "listenPort": proxy.listen_port
        }
    })))
}

async fn api_state(State(state): State<SharedState>) -> CoreResult<Json<AppStateResponse>> {
    Ok(Json(AppStateResponse {
        providers: state.database.providers(true)?,
        provider_health: state.database.provider_health()?,
        models: state.database.models()?,
        agents: state.database.agents()?,
        mcp_servers: Vec::new(),
        logs: state.database.logs()?,
        gateway: state.database.gateway_state()?,
    }))
}

async fn list_providers(State(state): State<SharedState>) -> CoreResult<Json<Vec<Provider>>> {
    Ok(Json(state.database.providers(true)?))
}

async fn add_provider(
    State(state): State<SharedState>,
    Json(mut provider): Json<Provider>,
) -> CoreResult<Json<Value>> {
    if provider.id.trim().is_empty() {
        provider.id = format!("custom-{}", Uuid::new_v4());
    }
    if provider.service_type.trim().is_empty() {
        provider.service_type = infer_service_type(&provider.provider_type);
    }
    provider.app_type = infer_app_type(&provider.service_type);
    provider.status = if provider.api_key.trim().is_empty() {
        "disconnected".into()
    } else {
        "connected".into()
    };
    state.database.add_provider(&provider)?;
    Ok(Json(json!({ "success": true, "provider": provider.id })))
}

async fn update_provider(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
    Json(value): Json<Value>,
) -> CoreResult<Json<Value>> {
    state.database.update_provider(&id, value)?;
    Ok(Json(json!({ "success": true })))
}

async fn delete_provider(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
) -> CoreResult<Json<Value>> {
    state.database.delete_provider(&id)?;
    Ok(Json(json!({ "success": true })))
}

async fn switch_provider(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
) -> CoreResult<Json<Value>> {
    state.database.switch_provider(&id)?;
    Ok(Json(json!({ "success": true })))
}

async fn fetch_provider_models(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
) -> CoreResult<Json<Vec<UpstreamModel>>> {
    let provider = state
        .database
        .provider_raw(&id)?
        .ok_or_else(|| CoreError::Message(format!("provider not found: {id}")))?;
    Ok(Json(fetch_upstream_models(&provider).await?))
}

async fn sync_provider_models(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
) -> CoreResult<Json<Value>> {
    let provider = state
        .database
        .provider_raw(&id)?
        .ok_or_else(|| CoreError::Message(format!("provider not found: {id}")))?;
    let upstream_models = fetch_upstream_models(&provider).await?;
    let imported = state
        .database
        .import_provider_models(&id, &upstream_models)?;
    Ok(Json(json!({
        "success": true,
        "upstreamCount": upstream_models.len(),
        "imported": imported
    })))
}

async fn reorder_providers(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> CoreResult<Json<Value>> {
    let current_index = body
        .get("currentIndex")
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize;
    let direction = body
        .get("direction")
        .and_then(Value::as_str)
        .unwrap_or("down");
    state.database.reorder_provider(current_index, direction)?;
    Ok(Json(json!({ "success": true })))
}

async fn test_provider(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
) -> CoreResult<Json<Value>> {
    let provider = state
        .database
        .provider_raw(&id)?
        .ok_or_else(|| CoreError::Message(format!("provider not found: {id}")))?;
    let started = Instant::now();
    let result = test_provider_connection(&provider).await;
    let latency_ms = started.elapsed().as_millis();
    let latency = format!("{latency_ms}ms");
    let status = if result.is_ok() {
        "connected"
    } else {
        "disconnected"
    };
    state.database.update_provider(
        &id,
        json!({
            "status": status,
            "latency": latency,
        }),
    )?;
    match result {
        Ok(()) => {
            state.database.record_provider_success(&id, latency_ms)?;
            state.database.add_log(
                "info",
                "ProviderRouter",
                &format!("Connection verified for {}", provider.name),
            )?;
        }
        Err(err) => {
            state
                .database
                .record_provider_failure(&id, &err.to_string())?;
            state.database.add_log(
                "error",
                "ProviderRouter",
                &format!("Connection failed for {}: {err}", provider.name),
            )?;
        }
    }
    Ok(Json(
        json!({ "success": status == "connected", "status": status }),
    ))
}

async fn test_all_providers(State(state): State<SharedState>) -> CoreResult<Json<Value>> {
    let providers = state.database.providers(false)?;
    for provider in providers {
        let _ = test_provider(State(state.clone()), AxumPath(provider.id)).await;
    }
    Ok(Json(json!({ "success": true })))
}

async fn test_provider_connection(provider: &Provider) -> CoreResult<()> {
    let url = provider_models_url(provider);
    let client = provider_http_client(provider, 10)?;
    let mut request = client.get(url);
    request = apply_custom_headers(request, provider);
    if let Some(key) = provider.primary_key() {
        request = apply_auth(request, provider, &key);
    }
    let response = request.send().await?;
    if response.status().is_success() || response.status().as_u16() == 404 {
        Ok(())
    } else {
        Err(CoreError::Message(format!(
            "upstream returned {}",
            response.status()
        )))
    }
}

async fn fetch_upstream_models(provider: &Provider) -> CoreResult<Vec<UpstreamModel>> {
    let url = provider_models_url(provider);
    let client = provider_http_client(provider, 20)?;
    let mut request = client.get(&url);
    request = apply_custom_headers(request, provider);
    if let Some(key) = provider.primary_key() {
        request = apply_auth(request, provider, &key);
    }
    let response = request.send().await?;
    let status = response.status();
    let bytes = response.bytes().await?;
    if !status.is_success() {
        return Err(CoreError::Message(format!(
            "upstream model list returned {status}: {}",
            String::from_utf8_lossy(&bytes)
        )));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|err| CoreError::Config(format!("invalid upstream model list JSON: {err}")))?;
    Ok(parse_upstream_models(&provider.service_type, &value))
}

fn provider_http_client(provider: &Provider, timeout_secs: u64) -> CoreResult<reqwest::Client> {
    let mut builder =
        reqwest::Client::builder().timeout(std::time::Duration::from_secs(timeout_secs));
    if provider.skip_tls {
        builder = builder.danger_accept_invalid_certs(true);
    }
    if let Some(proxy_url) = provider
        .proxy_url
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        builder = builder.proxy(Proxy::all(proxy_url)?);
    }
    Ok(builder.build()?)
}

async fn get_proxy_config(State(state): State<SharedState>) -> CoreResult<Json<ProxyConfig>> {
    Ok(Json(state.database.proxy_config()?))
}

async fn update_proxy_config(
    State(state): State<SharedState>,
    Json(config): Json<ProxyConfig>,
) -> CoreResult<Json<Value>> {
    state.database.update_proxy_config(&config)?;
    state
        .database
        .add_log("info", "Gateway", "Proxy configuration updated")?;
    Ok(Json(json!({ "success": true })))
}

async fn start_proxy(State(state): State<SharedState>) -> CoreResult<Json<Value>> {
    state.database.set_proxy_enabled(true)?;
    state
        .database
        .add_log("info", "Gateway", "Local proxy enabled")?;
    Ok(Json(
        json!({ "success": true, "gateway": state.database.gateway_state()? }),
    ))
}

async fn stop_proxy(State(state): State<SharedState>) -> CoreResult<Json<Value>> {
    state.database.set_proxy_enabled(false)?;
    state
        .database
        .add_log("warn", "Gateway", "Local proxy disabled")?;
    Ok(Json(
        json!({ "success": true, "gateway": state.database.gateway_state()? }),
    ))
}

async fn proxy_status(State(state): State<SharedState>) -> CoreResult<Json<GatewayState>> {
    Ok(Json(state.database.gateway_state()?))
}

async fn toggle_gateway(State(state): State<SharedState>) -> CoreResult<Json<Value>> {
    let current = state.database.proxy_config()?.enabled;
    state.database.set_proxy_enabled(!current)?;
    state.database.add_log(
        if current { "warn" } else { "info" },
        "Gateway",
        if current {
            "Local proxy stopped"
        } else {
            "Local proxy started"
        },
    )?;
    Ok(Json(
        json!({ "success": true, "gateway": state.database.gateway_state()? }),
    ))
}

async fn restart_gateway(State(state): State<SharedState>) -> CoreResult<Json<Value>> {
    state.database.set_proxy_enabled(false)?;
    state.database.set_proxy_enabled(true)?;
    state
        .database
        .add_log("info", "Gateway", "Local proxy restarted")?;
    Ok(Json(json!({ "success": true })))
}

async fn quick_task(State(state): State<SharedState>) -> CoreResult<Json<Value>> {
    state
        .database
        .add_log("info", "Diagnostics", "Quick local diagnostic completed")?;
    Ok(Json(json!({ "success": true })))
}

async fn reset_database(State(state): State<SharedState>) -> CoreResult<Json<Value>> {
    state.database.reset()?;
    Ok(Json(json!({ "success": true })))
}

async fn list_logs(State(state): State<SharedState>) -> CoreResult<Json<Vec<LogEntry>>> {
    Ok(Json(state.database.logs()?))
}

async fn clear_logs(State(state): State<SharedState>) -> CoreResult<Json<Value>> {
    state.database.clear_logs()?;
    Ok(Json(json!({ "success": true })))
}

async fn list_gateway_users(
    State(state): State<SharedState>,
) -> CoreResult<Json<Vec<GatewayUser>>> {
    Ok(Json(state.database.gateway_users()?))
}

async fn create_gateway_user(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> CoreResult<Json<Value>> {
    let user = state.database.add_gateway_user(body)?;
    Ok(Json(json!({ "success": true, "user": user })))
}

async fn update_gateway_user(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<Value>,
) -> CoreResult<Json<Value>> {
    let user = state.database.update_gateway_user(&id, body)?;
    Ok(Json(json!({ "success": true, "user": user })))
}

async fn list_gateway_projects(
    State(state): State<SharedState>,
) -> CoreResult<Json<Vec<GatewayProject>>> {
    Ok(Json(state.database.gateway_projects()?))
}

async fn create_gateway_project(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> CoreResult<Json<Value>> {
    let project = state.database.add_gateway_project(body)?;
    Ok(Json(json!({ "success": true, "project": project })))
}

async fn update_gateway_project(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<Value>,
) -> CoreResult<Json<Value>> {
    let project = state.database.update_gateway_project(&id, body)?;
    Ok(Json(json!({ "success": true, "project": project })))
}

async fn list_gateway_keys(State(state): State<SharedState>) -> CoreResult<Json<Vec<GatewayKey>>> {
    Ok(Json(state.database.gateway_keys()?))
}

async fn create_gateway_key(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> CoreResult<Json<Value>> {
    let (key, raw_key) = state.database.add_gateway_key(body)?;
    Ok(Json(
        json!({ "success": true, "key": key, "rawKey": raw_key }),
    ))
}

async fn update_gateway_key(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<Value>,
) -> CoreResult<Json<Value>> {
    let key = state.database.update_gateway_key(&id, body)?;
    Ok(Json(json!({ "success": true, "key": key })))
}

async fn revoke_gateway_key(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
) -> CoreResult<Json<Value>> {
    let key = state.database.revoke_gateway_key(&id)?;
    Ok(Json(json!({ "success": true, "key": key })))
}

async fn gateway_usage_summary(
    State(state): State<SharedState>,
) -> CoreResult<Json<GatewayUsageSummary>> {
    Ok(Json(state.database.gateway_usage_summary()?))
}

async fn list_distillation_jobs(
    State(state): State<SharedState>,
) -> CoreResult<Json<Vec<DistillationJob>>> {
    Ok(Json(state.database.distillation_jobs()?))
}

async fn create_distillation_job(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> CoreResult<Json<Value>> {
    let job = state.database.create_distillation_job(body)?;
    Ok(Json(json!({ "success": true, "job": job })))
}

async fn get_distillation_job(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
) -> CoreResult<Json<DistillationJob>> {
    let job = state
        .database
        .distillation_job(&id)?
        .ok_or_else(|| CoreError::Message(format!("distillation job not found: {id}")))?;
    Ok(Json(job))
}

async fn get_telemetry_config(
    State(state): State<SharedState>,
) -> CoreResult<Json<TelemetryConfig>> {
    Ok(Json(state.database.telemetry_config()?))
}

async fn update_telemetry_config(
    State(state): State<SharedState>,
    Json(updates): Json<Value>,
) -> CoreResult<Json<TelemetryConfig>> {
    Ok(Json(state.database.update_telemetry_config(updates)?))
}

async fn telemetry_summary(State(state): State<SharedState>) -> CoreResult<Json<TelemetrySummary>> {
    Ok(Json(state.database.telemetry_summary()?))
}

async fn list_telemetry_traces(
    State(state): State<SharedState>,
    Query(query): Query<TraceQuery>,
) -> CoreResult<Json<Vec<RequestTrace>>> {
    Ok(Json(state.database.request_traces(query)?))
}

async fn clear_telemetry_traces(State(state): State<SharedState>) -> CoreResult<Json<Value>> {
    state.database.clear_request_traces()?;
    Ok(Json(json!({ "success": true })))
}

async fn chat_send(
    State(state): State<SharedState>,
    Json(body): Json<Value>,
) -> CoreResult<Json<Value>> {
    let input = body.get("input").and_then(Value::as_str).unwrap_or("");
    let provider_id = body
        .get("selectedProviderId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let model_id = body
        .get("selectedModelId")
        .and_then(Value::as_str)
        .unwrap_or("");
    let lang = body.get("lang").and_then(Value::as_str).unwrap_or("en");
    let telemetry_config = state.database.telemetry_config()?;
    let original_body = serde_json::to_vec(&body).unwrap_or_default();
    let project_context = extract_project_context(&HeaderMap::new(), &original_body);
    let provider = state.database.provider_raw(provider_id)?;

    let Some(provider) = provider else {
        return Ok(Json(json!({
            "success": true,
            "isSuccess": false,
            "replyContent": if lang == "zh" {
                "未找到选中的 provider。请先在 AI 通道管理中创建并保存 provider。"
            } else {
                "Selected provider was not found. Add and save a provider first."
            },
            "estimatedLatencyMs": 0
        })));
    };

    if provider.primary_key().is_none() || provider.status == "disconnected" {
        state.database.add_log(
            "error",
            "Playground",
            &format!(
                "Sandbox invocation blocked: provider {} has no active key",
                provider.name
            ),
        )?;
        let request_body_preview = body_preview_if_enabled(&telemetry_config, &original_body);
        let _ = record_proxy_trace(
            &state.database,
            &telemetry_config,
            &project_context,
            "sandbox",
            "/api/chat/send",
            &provider,
            &provider.base_url,
            Some(model_id),
            None,
            false,
            0,
            original_body.len() as u64,
            None,
            Some("provider has no active key".into()),
            request_body_preview,
            None,
        );
        return Ok(Json(json!({
            "success": true,
            "isSuccess": false,
            "replyContent": if lang == "zh" {
                format!("无法连通服务商：{}。请确认 API Key 与 Base URL 后再测试。", provider.name)
            } else {
                format!("Connection failed with provider {}. Configure API key and Base URL before testing.", provider.name)
            },
            "estimatedLatencyMs": 0
        })));
    }

    let upstream_model_id =
        resolve_provider_model_for_request(Some(&state.database), &provider, model_id)?;
    let started = Instant::now();
    let request = build_chat_upstream_request(&provider, &body, &upstream_model_id, input)?;
    let upstream = request.upstream;
    let route_path = request.path;
    let request_body = request.body;
    let request_body_preview = body_preview_if_enabled(&telemetry_config, &request_body);
    let request_bytes = request_body.len() as u64;
    let mut headers = HeaderMap::new();
    headers.insert("content-type", HeaderValue::from_static("application/json"));
    let primary_key = provider.primary_key();
    let response = send_proxy_request(
        &provider,
        &upstream,
        &headers,
        request_body,
        primary_key.as_deref(),
    )
    .await;
    let latency = started.elapsed().as_millis() as u64;

    let response = match response {
        Ok(response) => response,
        Err(err) => {
            state
                .database
                .record_provider_failure(&provider.id, &err.to_string())?;
            state.database.add_log(
                "error",
                "Playground",
                &format!(
                    "Sandbox request failed for {} with model {}: {err}",
                    provider.name, upstream_model_id
                ),
            )?;
            let _ = record_proxy_trace(
                &state.database,
                &telemetry_config,
                &project_context,
                "sandbox",
                &route_path,
                &provider,
                &upstream,
                Some(&upstream_model_id),
                None,
                false,
                latency,
                request_bytes,
                None,
                Some(err.to_string()),
                request_body_preview.clone(),
                None,
            );
            return Ok(Json(json!({
                "success": true,
                "isSuccess": false,
                "replyContent": format_chat_error(lang, &provider.name, &err.to_string()),
                "estimatedLatencyMs": latency
            })));
        }
    };

    let status = response.status();
    let bytes = response.bytes().await?;
    if !status.is_success() {
        let error = String::from_utf8_lossy(&bytes).to_string();
        let response_body_preview = body_preview_if_enabled(&telemetry_config, &bytes);
        state.database.record_provider_failure(
            &provider.id,
            &format!("upstream returned {status}: {error}"),
        )?;
        state.database.add_log(
            "error",
            "Playground",
            &format!(
                "Sandbox upstream returned {status} for {} with model {}",
                provider.name, upstream_model_id
            ),
        )?;
        let _ = record_proxy_trace(
            &state.database,
            &telemetry_config,
            &project_context,
            "sandbox",
            &route_path,
            &provider,
            &upstream,
            Some(&upstream_model_id),
            Some(status),
            false,
            latency,
            request_bytes,
            Some(bytes.len() as u64),
            Some(format!("upstream returned {status}: {error}")),
            request_body_preview.clone(),
            response_body_preview,
        );
        return Ok(Json(json!({
            "success": true,
            "isSuccess": false,
            "replyContent": format_chat_error(lang, &provider.name, &format!("{status}: {error}")),
            "estimatedLatencyMs": latency
        })));
    }

    let value: Value = serde_json::from_slice(&bytes)
        .unwrap_or_else(|_| json!({ "text": String::from_utf8_lossy(&bytes).to_string() }));
    let reply = extract_chat_reply(&provider.service_type, &value);
    let response_body_preview = body_preview_if_enabled(&telemetry_config, &bytes);
    state
        .database
        .record_provider_success(&provider.id, latency as u128)?;
    state.database.add_log(
        "info",
        "Playground",
        &format!(
            "Sandbox sent real upstream request to {} with model {}",
            provider.name, upstream_model_id
        ),
    )?;
    let _ = record_proxy_trace(
        &state.database,
        &telemetry_config,
        &project_context,
        "sandbox",
        &route_path,
        &provider,
        &upstream,
        Some(&upstream_model_id),
        Some(status),
        true,
        latency,
        request_bytes,
        Some(bytes.len() as u64),
        None,
        request_body_preview,
        response_body_preview,
    );
    Ok(Json(json!({
        "success": true,
        "isSuccess": true,
        "replyContent": reply,
        "estimatedLatencyMs": latency,
        "rawResponse": value
    })))
}

struct ChatUpstreamRequest {
    upstream: String,
    path: String,
    body: Bytes,
}

fn resolve_provider_model_for_request(
    database: Option<&Database>,
    provider: &Provider,
    model_reference: &str,
) -> CoreResult<String> {
    if let Some(mapped) = provider.model_mapping.get(model_reference) {
        return Ok(mapped.clone());
    }
    if let Some(database) = database {
        if let Some(model_name) =
            database.provider_model_name_for_reference(&provider.id, model_reference)?
        {
            if let Some(mapped) = provider.model_mapping.get(&model_name) {
                return Ok(mapped.clone());
            }
            return Ok(model_name);
        }
    }
    Ok(model_reference.to_string())
}

fn build_chat_upstream_request(
    provider: &Provider,
    body: &Value,
    upstream_model_id: &str,
    input: &str,
) -> CoreResult<ChatUpstreamRequest> {
    let temperature = body
        .get("temperature")
        .and_then(Value::as_f64)
        .unwrap_or(0.7);
    let max_tokens = body
        .get("maxTokens")
        .and_then(Value::as_u64)
        .unwrap_or(4096);
    let (path, payload) = match provider.service_type.as_str() {
        "claude" => (
            "/v1/messages".to_string(),
            json!({
                "model": upstream_model_id,
                "max_tokens": max_tokens,
                "temperature": temperature,
                "messages": [{ "role": "user", "content": input }]
            }),
        ),
        "openai" => (
            "/v1/chat/completions".to_string(),
            json!({
                "model": upstream_model_id,
                "temperature": temperature,
                "max_tokens": max_tokens,
                "messages": [{ "role": "user", "content": input }]
            }),
        ),
        "gemini" => (
            format!("/v1beta/models/{upstream_model_id}:generateContent"),
            json!({
                "contents": [{ "parts": [{ "text": input }] }],
                "generationConfig": {
                    "temperature": temperature,
                    "maxOutputTokens": max_tokens
                }
            }),
        ),
        _ => (
            "/v1/responses".to_string(),
            json!({
                "model": upstream_model_id,
                "temperature": temperature,
                "max_output_tokens": max_tokens,
                "input": input
            }),
        ),
    };

    serde_json::to_vec(&payload)
        .map(Bytes::from)
        .map(|body| ChatUpstreamRequest {
            upstream: upstream_url(provider, &path),
            path,
            body,
        })
        .map_err(|err| CoreError::Config(format!("failed to build sandbox request: {err}")))
}

fn extract_chat_reply(service_type: &str, value: &Value) -> String {
    if let Some(text) = value.get("output_text").and_then(Value::as_str) {
        return text.to_string();
    }
    if let Some(text) = value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
    {
        return text.to_string();
    }
    if let Some(text) = value.pointer("/content/0/text").and_then(Value::as_str) {
        return text.to_string();
    }
    if let Some(text) = value
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(Value::as_str)
    {
        return text.to_string();
    }
    if let Some(text) = extract_responses_output_text(value) {
        return text;
    }
    if let Some(text) = value.get("text").and_then(Value::as_str) {
        return text.to_string();
    }
    serde_json::to_string_pretty(value)
        .unwrap_or_else(|_| format!("No textual {service_type} response content was found."))
}

fn extract_responses_output_text(value: &Value) -> Option<String> {
    let output = value.get("output")?.as_array()?;
    let mut parts = Vec::new();
    for item in output {
        if let Some(content) = item.get("content").and_then(Value::as_array) {
            for chunk in content {
                if let Some(text) = chunk.get("text").and_then(Value::as_str) {
                    parts.push(text.to_string());
                }
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(""))
    }
}

fn format_chat_error(lang: &str, provider_name: &str, error: &str) -> String {
    if lang == "zh" {
        format!("无法连通服务商：{provider_name}。\n\n{error}")
    } else {
        format!("Connection failed with provider {provider_name}.\n\n{error}")
    }
}

async fn list_models(State(state): State<SharedState>) -> CoreResult<Json<Vec<Model>>> {
    Ok(Json(state.database.models()?))
}

async fn add_model(
    State(state): State<SharedState>,
    Json(model): Json<Model>,
) -> CoreResult<Json<Value>> {
    state.database.add_model(&model)?;
    Ok(Json(json!({ "success": true })))
}

async fn update_model(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
    Json(updates): Json<Value>,
) -> CoreResult<Json<Value>> {
    state.database.update_model(&id, updates)?;
    Ok(Json(json!({ "success": true })))
}

async fn delete_model(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
) -> CoreResult<Json<Value>> {
    state.database.delete_model(&id)?;
    Ok(Json(json!({ "success": true })))
}

async fn update_agent(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
    Json(updates): Json<Value>,
) -> CoreResult<Json<Value>> {
    state.database.update_agent(&id, updates)?;
    Ok(Json(json!({ "success": true })))
}

async fn invoke_agent(
    State(state): State<SharedState>,
    AxumPath(id): AxumPath<String>,
    body: Option<Json<Value>>,
) -> CoreResult<Json<Value>> {
    let agent = state
        .database
        .agent(&id)?
        .ok_or_else(|| CoreError::Message(format!("agent not found: {id}")))?;
    let body = body.map(|Json(value)| value).unwrap_or_else(|| json!({}));
    if matches!(agent.id.as_str(), "codex" | "codex-desktop") {
        return Err(CoreError::Message(format!(
            "real process invocation is not implemented for agent {}",
            agent.name
        )));
    }

    state.database.update_agent(
        &id,
        json!({
            "status": "running",
            "lastRun": Utc::now().to_rfc3339()
        }),
    )?;
    let invocation = match agent.id.as_str() {
        "gemini-cli" => run_gemini_agent_process(&agent, &body).await,
        "claude-code" => run_claude_agent_process(&agent, &body).await,
        _ => Err(CoreError::Message(format!(
            "real process invocation is not implemented for agent {}",
            agent.name
        ))),
    };
    match invocation {
        Ok(invocation) if agent_invocation_succeeded(&invocation) => {
            state.database.update_agent(
                &id,
                json!({
                    "status": "idle",
                    "lastRun": Utc::now().to_rfc3339()
                }),
            )?;
            state.database.add_log(
                "info",
                "Agent",
                &format!(
                    "Agent {id} invoked real {} process in {}ms",
                    invocation.command, invocation.duration_ms
                ),
            )?;
            Ok(Json(json!({
                "success": true,
                "agentId": id,
                "invocation": invocation
            })))
        }
        Ok(invocation) => {
            state.database.update_agent(
                &id,
                json!({
                    "status": "error",
                    "lastRun": Utc::now().to_rfc3339()
                }),
            )?;
            state.database.add_log(
                "error",
                "Agent",
                &format!(
                    "Agent {id} {} process exited with {:?}: {}",
                    invocation.command, invocation.exit_code, invocation.stderr
                ),
            )?;
            Ok(Json(json!({
                "success": false,
                "agentId": id,
                "invocation": invocation
            })))
        }
        Err(err) => {
            state.database.update_agent(
                &id,
                json!({
                    "status": "error",
                    "lastRun": Utc::now().to_rfc3339()
                }),
            )?;
            Err(err)
        }
    }
}

async fn run_claude_agent_process(
    _agent: &Agent,
    body: &Value,
) -> CoreResult<AgentProcessInvocation> {
    let prompt = body
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Do not call tools. Reply exactly: CODEXSWITCH_CLAUDE_AGENT_OK");
    let executable = env::var("CODEXSWITCH_CLAUDE_CLI_PATH").unwrap_or_else(|_| "claude".into());
    let mut args = vec![
        "-p".to_string(),
        prompt.to_string(),
        "--output-format".to_string(),
        "json".to_string(),
    ];
    if let Some(model) = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        args.push("--model".to_string());
        args.push(model.to_string());
    }
    let inherit_auth_env = env::var("CODEXSWITCH_CLAUDE_INHERIT_AUTH_ENV")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let env_overrides = if inherit_auth_env {
        Vec::new()
    } else {
        let base_url = body
            .get("baseUrl")
            .or_else(|| body.get("base_url"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| env::var("CODEXSWITCH_CLAUDE_BASE_URL").ok())
            .unwrap_or_else(|| "http://127.0.0.1:3900".into());
        let auth_token = body
            .get("authToken")
            .or_else(|| body.get("auth_token"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| env::var("CODEXSWITCH_CLAUDE_AUTH_TOKEN").ok())
            .unwrap_or_else(|| "codexswitch-local".into());
        vec![
            ("ANTHROPIC_BASE_URL".to_string(), base_url),
            ("ANTHROPIC_AUTH_TOKEN".to_string(), auth_token),
        ]
    };
    let env_removals = if inherit_auth_env {
        Vec::new()
    } else {
        vec!["ANTHROPIC_API_KEY".to_string()]
    };
    run_agent_process("claude", executable, args, env_removals, env_overrides).await
}

async fn run_gemini_agent_process(
    agent: &Agent,
    body: &Value,
) -> CoreResult<AgentProcessInvocation> {
    let prompt = body
        .get("prompt")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Do not call tools. Reply exactly: CODEXSWITCH_GEMINI_AGENT_OK");
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&agent.model_id);
    let executable = env::var("CODEXSWITCH_GEMINI_CLI_PATH").unwrap_or_else(|_| "gemini".into());
    let args = vec![
        "--model".to_string(),
        model.to_string(),
        "--output-format".to_string(),
        "json".to_string(),
        "-p".to_string(),
        prompt.to_string(),
    ];
    run_agent_process("gemini", executable, args, Vec::new(), Vec::new()).await
}

async fn run_agent_process(
    command: &str,
    executable: String,
    args: Vec<String>,
    env_removals: Vec<String>,
    env_overrides: Vec<(String, String)>,
) -> CoreResult<AgentProcessInvocation> {
    let process_args = args.clone();
    let process_executable = executable.clone();
    let started = Instant::now();
    let output = tokio::task::spawn_blocking(move || {
        let mut command = Command::new(&process_executable);
        command.args(&process_args);
        for name in env_removals {
            command.env_remove(name);
        }
        for (name, value) in env_overrides {
            command.env(name, value);
        }
        command.output()
    })
    .await
    .map_err(|err| CoreError::Config(format!("failed to join {command} process: {err}")))?
    .map_err(|err| CoreError::Config(format!("failed to start {command}: {err}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let response = extract_agent_process_response(&stdout);
    let is_error = extract_agent_process_is_error(&stdout);
    Ok(AgentProcessInvocation {
        command: command.into(),
        executable,
        args,
        exit_code: output.status.code(),
        stdout,
        stderr,
        response,
        is_error,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn extract_agent_process_response(stdout: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(stdout).ok()?;
    for key in ["response", "result", "text", "output_text"] {
        if let Some(text) = value.get(key).and_then(Value::as_str) {
            return Some(text.to_string());
        }
    }
    value
        .pointer("/message/content/0/text")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn extract_agent_process_is_error(stdout: &str) -> bool {
    serde_json::from_str::<Value>(stdout)
        .ok()
        .and_then(|value| {
            value
                .get("is_error")
                .or_else(|| value.get("isError"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

fn agent_invocation_succeeded(invocation: &AgentProcessInvocation) -> bool {
    invocation.exit_code == Some(0) && !invocation.is_error
}

async fn proxy_messages(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> CoreResult<Response> {
    proxy_protocol(state, "claude", "/v1/messages", headers, body).await
}

async fn proxy_count_tokens(body: Bytes) -> CoreResult<Response> {
    let tokens = request_usage_from_body(&body, read_model_from_body(&body).as_deref())
        .prompt_tokens
        .max(1);
    Ok(Json(json!({ "input_tokens": tokens })).into_response())
}

async fn proxy_chat(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> CoreResult<Response> {
    proxy_protocol(state, "openai", "/v1/chat/completions", headers, body).await
}

async fn proxy_responses(
    State(state): State<SharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> CoreResult<Response> {
    proxy_protocol(state, "responses", "/v1/responses", headers, body).await
}

async fn proxy_gemini(
    State(state): State<SharedState>,
    AxumPath(path): AxumPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> CoreResult<Response> {
    proxy_protocol(
        state,
        "gemini",
        &format!("/v1beta/models/{path}"),
        headers,
        body,
    )
    .await
}

async fn proxy_models(
    State(state): State<SharedState>,
    headers: HeaderMap,
) -> CoreResult<Response> {
    if state.database.gateway_key_count()? > 0 {
        let Some(raw_key) = gateway_key_from_headers(&headers) else {
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "codexswitch gateway key is required" })),
            )
                .into_response());
        };
        let Some(principal) = state.database.principal_for_gateway_key(&raw_key)? else {
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "invalid or inactive codexswitch gateway key" })),
            )
                .into_response());
        };
        if let Some(reason) = quota_violation(&principal) {
            return Ok((
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": reason, "keyId": principal.key_id })),
            )
                .into_response());
        }
    }

    let models = state.database.models()?;
    let mut seen = BTreeSet::new();
    Ok(Json(json!({
        "object": "list",
        "data": models.into_iter().filter(|model| seen.insert(model.name.clone())).map(|model| {
            json!({
                "id": model.name,
                "object": "model",
                "owned_by": model.provider_id
            })
        }).collect::<Vec<_>>()
    }))
    .into_response())
}

async fn proxy_protocol(
    state: SharedState,
    protocol: &str,
    path: &str,
    headers: HeaderMap,
    body: Bytes,
) -> CoreResult<Response> {
    let proxy_config = state.database.proxy_config()?;
    if !proxy_config.enabled {
        return Ok((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "codexswitch proxy is stopped" })),
        )
            .into_response());
    }

    let telemetry_config = state.database.telemetry_config()?;
    let request_model = read_model_from_body(&body);
    let principal = if state.database.gateway_key_count()? > 0 {
        let Some(raw_key) = gateway_key_from_headers(&headers) else {
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "codexswitch gateway key is required" })),
            )
                .into_response());
        };
        let Some(principal) = state.database.principal_for_gateway_key(&raw_key)? else {
            return Ok((
                StatusCode::UNAUTHORIZED,
                Json(json!({ "error": "invalid or inactive codexswitch gateway key" })),
            )
                .into_response());
        };
        if let Some(reason) = quota_violation(&principal) {
            return Ok((
                StatusCode::TOO_MANY_REQUESTS,
                Json(json!({ "error": reason, "keyId": principal.key_id })),
            )
                .into_response());
        }
        Some(principal)
    } else {
        None
    };
    let raw_project_context = extract_project_context(&headers, &body);
    let project_context = context_for_principal(&raw_project_context, principal.as_ref());
    let request_usage = request_usage_from_body(&body, request_model.as_deref());
    let request_body_preview = body_preview_if_enabled(&telemetry_config, &body);
    let providers = state
        .database
        .providers_for_protocol(protocol, request_model.as_deref())?;
    if providers.is_empty() {
        return Err(CoreError::Message(format!(
            "no provider configured for {protocol}"
        )));
    }

    let total_attempts = providers
        .iter()
        .map(|provider| provider.auth_keys().len().max(1))
        .sum::<usize>();
    let max_attempts = (proxy_config.max_retries as usize).saturating_add(1).max(1);
    let mut attempt = 0usize;

    for provider in providers {
        if attempt >= max_attempts {
            break;
        }
        let prepared =
            prepare_proxy_request(&state.database, protocol, path, &provider, body.clone())?;
        let upstream = upstream_url(&provider, &prepared.path);
        let auth_keys = provider.auth_keys();
        let auth_attempts = if auth_keys.is_empty() {
            vec![None]
        } else {
            auth_keys.into_iter().map(Some).collect::<Vec<_>>()
        };

        for key in auth_attempts {
            if attempt >= max_attempts {
                break;
            }
            attempt += 1;
            let is_last_attempt = attempt >= total_attempts || attempt >= max_attempts;
            let started = Instant::now();
            match send_proxy_request(
                &provider,
                &upstream,
                &headers,
                prepared.body.clone(),
                key.as_deref(),
            )
            .await
            {
                Ok(response) => {
                    let status = response.status();
                    let latency_ms = started.elapsed().as_millis() as u64;
                    if should_retry_proxy_status(status) && !is_last_attempt {
                        record_proxy_trace_with_gateway(
                            &state.database,
                            &telemetry_config,
                            &project_context,
                            principal.as_ref(),
                            request_usage,
                            protocol,
                            path,
                            &provider,
                            &upstream,
                            request_model.as_deref(),
                            Some(status),
                            false,
                            latency_ms,
                            body.len() as u64,
                            None,
                            Some(format!("upstream returned {status}")),
                            request_body_preview.clone(),
                            None,
                        )?;
                        state.database.record_provider_failure(
                            &provider.id,
                            &format!("upstream returned {status}"),
                        )?;
                        state.database.add_log(
                            "warn",
                            "Proxy",
                            &format!(
                                "{} {} -> {} via {}; trying next route",
                                status.as_u16(),
                                path,
                                upstream,
                                provider.name
                            ),
                        )?;
                        continue;
                    }
                    if status.is_success() {
                        state
                            .database
                            .record_provider_success(&provider.id, latency_ms as u128)?;
                    } else {
                        state.database.record_provider_failure(
                            &provider.id,
                            &format!("upstream returned {status}"),
                        )?;
                    }
                    let trace_id = record_proxy_trace_with_gateway(
                        &state.database,
                        &telemetry_config,
                        &project_context,
                        principal.as_ref(),
                        request_usage,
                        protocol,
                        path,
                        &provider,
                        &upstream,
                        request_model.as_deref(),
                        Some(status),
                        status.is_success(),
                        latency_ms,
                        body.len() as u64,
                        None,
                        if status.is_success() {
                            None
                        } else {
                            Some(format!("upstream returned {status}"))
                        },
                        request_body_preview.clone(),
                        None,
                    )?;
                    state.database.add_log(
                        if status.is_success() { "info" } else { "warn" },
                        "Proxy",
                        &format!(
                            "{} {} -> {} via {}",
                            status.as_u16(),
                            path,
                            upstream,
                            provider.name
                        ),
                    )?;
                    if let Some(principal) = principal.as_ref() {
                        state.database.record_gateway_usage(
                            principal,
                            UsageEstimate::default(),
                            true,
                        )?;
                    }
                    return build_proxy_response(
                        response,
                        Some(ResponseTraceRecorder {
                            database: state.database.clone(),
                            config: telemetry_config.clone(),
                            trace_id,
                            principal: principal.clone(),
                            request_usage,
                            request_model: request_model.clone(),
                        }),
                        prepared.adapter,
                    )
                    .await;
                }
                Err(err) if !is_last_attempt => {
                    let latency_ms = started.elapsed().as_millis() as u64;
                    record_proxy_trace_with_gateway(
                        &state.database,
                        &telemetry_config,
                        &project_context,
                        principal.as_ref(),
                        request_usage,
                        protocol,
                        path,
                        &provider,
                        &upstream,
                        request_model.as_deref(),
                        None,
                        false,
                        latency_ms,
                        body.len() as u64,
                        None,
                        Some(err.to_string()),
                        request_body_preview.clone(),
                        None,
                    )?;
                    state
                        .database
                        .record_provider_failure(&provider.id, &err.to_string())?;
                    state.database.add_log(
                        "warn",
                        "Proxy",
                        &format!(
                            "{} -> {} via {} failed: {}; trying next route",
                            path, upstream, provider.name, err
                        ),
                    )?;
                }
                Err(err) => {
                    let latency_ms = started.elapsed().as_millis() as u64;
                    record_proxy_trace_with_gateway(
                        &state.database,
                        &telemetry_config,
                        &project_context,
                        principal.as_ref(),
                        request_usage,
                        protocol,
                        path,
                        &provider,
                        &upstream,
                        request_model.as_deref(),
                        None,
                        false,
                        latency_ms,
                        body.len() as u64,
                        None,
                        Some(err.to_string()),
                        request_body_preview.clone(),
                        None,
                    )?;
                    state
                        .database
                        .record_provider_failure(&provider.id, &err.to_string())?;
                    return Err(err);
                }
            }
            if attempt >= max_attempts {
                break;
            }
        }
    }

    Err(CoreError::Message(format!(
        "no usable provider configured for {protocol}"
    )))
}

async fn send_proxy_request(
    provider: &Provider,
    upstream: &str,
    headers: &HeaderMap,
    body: Bytes,
    key: Option<&str>,
) -> CoreResult<reqwest::Response> {
    let mut client_builder = reqwest::Client::builder();
    if provider.skip_tls {
        client_builder = client_builder.danger_accept_invalid_certs(true);
    }
    if let Some(proxy_url) = provider
        .proxy_url
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        client_builder = client_builder.proxy(Proxy::all(proxy_url)?);
    }
    let client = client_builder.build()?;
    let mut request = client.post(upstream).body(body);
    request = forward_headers(request, &headers);
    request = apply_custom_headers(request, provider);
    if let Some(key) = key {
        request = apply_auth(request, provider, key);
    }

    Ok(request.send().await?)
}

async fn build_proxy_response(
    response: reqwest::Response,
    trace_recorder: Option<ResponseTraceRecorder>,
    adapter: Option<ResponseAdapter>,
) -> CoreResult<Response> {
    let status = response.status();
    let mut builder = Response::builder().status(status);
    let is_event_stream = response
        .headers()
        .get("content-type")
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains("text/event-stream"))
        .unwrap_or(false);
    for (name, value) in response.headers().iter() {
        if !is_hop_by_hop(name.as_str())
            && !(adapter.is_some() && name.as_str().eq_ignore_ascii_case("content-length"))
            && !(adapter.is_some() && name.as_str().eq_ignore_ascii_case("content-type"))
        {
            builder = builder.header(name, value);
        }
    }

    if !is_event_stream {
        let mut bytes = response.bytes().await?;
        if status.is_success() {
            if let Some(adapter) = adapter {
                let (content_type, adapted) = adapt_proxy_response_body(adapter, &bytes)?;
                builder = builder.header("content-type", content_type);
                bytes = adapted;
            }
        }
        if let Some(recorder) = trace_recorder.as_ref() {
            let response_body_preview =
                response_preview_if_enabled(&recorder.config, &bytes, false);
            let usage = response_usage_from_body(
                &bytes,
                recorder.request_usage,
                recorder.request_model.as_deref(),
            );
            let _ = recorder.database.update_request_trace_response(
                &recorder.trace_id,
                bytes.len() as u64,
                response_body_preview,
                usage,
            );
            if let Some(principal) = recorder.principal.as_ref() {
                let _ = recorder
                    .database
                    .record_gateway_response_usage(principal, usage);
            }
        }
        return builder
            .body(Body::from(bytes))
            .map_err(|err| CoreError::Config(format!("response build failed: {err}")));
    }

    let mut upstream_stream = response.bytes_stream();
    let stream = async_stream::stream! {
        let mut response_bytes = 0u64;
        let mut preview_bytes = Vec::new();
        let mut preview_truncated = false;
        let mut preview_persisted = false;
        let mut completed = true;

        while let Some(chunk_result) = upstream_stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    response_bytes += chunk.len() as u64;
                        if let Some(recorder) = trace_recorder.as_ref() {
                            if recorder.config.enabled
                                && preview_bytes.len() < recorder.config.max_body_preview_bytes
                            {
                                let remaining = recorder.config.max_body_preview_bytes - preview_bytes.len();
                                let take = chunk.len().min(remaining);
                                preview_bytes.extend_from_slice(&chunk[..take]);
                                if take < chunk.len() {
                                    preview_truncated = true;
                                }
                            } else if recorder.config.enabled
                                && !chunk.is_empty()
                            {
                                preview_truncated = true;
                            }
                    }
                    if !preview_persisted {
                        if let Some(recorder) = trace_recorder.as_ref() {
                            if recorder.config.enabled
                                && recorder.config.capture_bodies
                                && !preview_bytes.is_empty()
                            {
                                let response_body_preview = response_preview_if_enabled(
                                    &recorder.config,
                                    &preview_bytes,
                                    true,
                                );
                                    let _ = recorder.database.update_request_trace_response(
                                        &recorder.trace_id,
                                        response_bytes,
                                        response_body_preview,
                                        recorder.request_usage,
                                    );
                                preview_persisted = true;
                            }
                        }
                    }
                    yield Ok(chunk);
                }
                Err(err) => {
                    completed = false;
                    yield Err(std::io::Error::new(std::io::ErrorKind::Other, err.to_string()));
                    break;
                }
            }
        }

        if completed {
            if let Some(recorder) = trace_recorder {
                    let response_body_preview = response_preview_if_enabled(
                        &recorder.config,
                        &preview_bytes,
                        preview_truncated,
                    );
                    let usage = response_usage_from_body(
                        &preview_bytes,
                        recorder.request_usage,
                        recorder.request_model.as_deref(),
                    );
                    let _ = recorder.database.update_request_trace_response(
                        &recorder.trace_id,
                        response_bytes,
                        response_body_preview,
                        usage,
                    );
                    if let Some(principal) = recorder.principal.as_ref() {
                        let _ = recorder.database.record_gateway_response_usage(principal, usage);
                    }
                }
        }
    };

    builder
        .body(Body::from_stream(stream))
        .map_err(|err| CoreError::Config(format!("response build failed: {err}")))
}

fn should_retry_proxy_status(status: StatusCode) -> bool {
    status == StatusCode::UNAUTHORIZED
        || status == StatusCode::FORBIDDEN
        || status == StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn forward_headers(
    mut request: reqwest::RequestBuilder,
    headers: &HeaderMap,
) -> reqwest::RequestBuilder {
    for (name, value) in headers.iter() {
        let name_str = name.as_str();
        if name_str.eq_ignore_ascii_case("host")
            || name_str.eq_ignore_ascii_case("authorization")
            || name_str.eq_ignore_ascii_case("content-length")
            || is_hop_by_hop(name_str)
        {
            continue;
        }
        request = request.header(name.clone(), value.clone());
    }
    request
}

fn apply_custom_headers(
    mut request: reqwest::RequestBuilder,
    provider: &Provider,
) -> reqwest::RequestBuilder {
    for pair in &provider.custom_headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(pair.name.as_bytes()),
            HeaderValue::from_str(&pair.value),
        ) {
            request = request.header(name, value);
        }
    }
    request
}

fn apply_auth(
    request: reqwest::RequestBuilder,
    provider: &Provider,
    key: &str,
) -> reqwest::RequestBuilder {
    match provider.service_type.as_str() {
        "gemini" => request.header("x-goog-api-key", key).bearer_auth(key),
        _ => request.bearer_auth(key),
    }
}

fn upstream_url(provider: &Provider, path: &str) -> String {
    let base = provider
        .base_urls
        .first()
        .filter(|value| !value.is_empty())
        .unwrap_or(&provider.base_url)
        .trim_end_matches('/')
        .to_string();
    let prefixed_path = if let Some(prefix) = provider
        .route_prefix
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        join_url_paths(prefix, path)
    } else {
        path.to_string()
    };
    let normalized_path = if base.ends_with("/v1") && prefixed_path.starts_with("/v1/") {
        prefixed_path.trim_start_matches("/v1").to_string()
    } else if base.ends_with("/v1beta") && prefixed_path.starts_with("/v1beta/") {
        prefixed_path.trim_start_matches("/v1beta").to_string()
    } else {
        prefixed_path
    };
    format!("{}{}", base, normalized_path)
}

fn join_url_paths(prefix: &str, path: &str) -> String {
    let prefix = prefix.trim().trim_matches('/');
    let path = path.trim_start_matches('/');
    if prefix.is_empty() {
        format!("/{path}")
    } else if path.is_empty() {
        format!("/{prefix}")
    } else {
        format!("/{prefix}/{path}")
    }
}

fn provider_models_url(provider: &Provider) -> String {
    match provider.service_type.as_str() {
        "gemini" => format!("{}/v1beta/models", provider.base_url.trim_end_matches('/')),
        _ => upstream_url(provider, "/v1/models"),
    }
}

fn parse_upstream_models(service_type: &str, value: &Value) -> Vec<UpstreamModel> {
    let items = value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut seen = BTreeSet::new();
    items
        .iter()
        .filter_map(|item| parse_upstream_model(service_type, item))
        .filter(|model| seen.insert(model.id.clone()))
        .collect()
}

fn parse_upstream_model(service_type: &str, item: &Value) -> Option<UpstreamModel> {
    let raw_id = item
        .get("id")
        .or_else(|| item.get("name"))
        .and_then(Value::as_str)?;
    let id = normalize_upstream_model_id(service_type, raw_id)?;
    let display_name = item
        .get("displayName")
        .or_else(|| item.get("display_name"))
        .and_then(Value::as_str)
        .and_then(clean_context_value)
        .unwrap_or_else(|| id.clone());
    Some(UpstreamModel {
        id,
        name: display_name,
        owned_by: item
            .get("owned_by")
            .or_else(|| item.get("ownedBy"))
            .and_then(Value::as_str)
            .map(str::to_string),
        context_window: token_limit_label(
            item.get("context_length")
                .or_else(|| item.get("contextWindow"))
                .or_else(|| item.get("inputTokenLimit")),
        ),
        max_output: token_limit_label(
            item.get("max_output_tokens")
                .or_else(|| item.get("maxOutput"))
                .or_else(|| item.get("outputTokenLimit")),
        ),
        supports_images: model_supports_images(item),
    })
}

fn normalize_upstream_model_id(service_type: &str, raw_id: &str) -> Option<String> {
    let trimmed = raw_id.trim();
    if trimmed.is_empty() {
        return None;
    }
    if service_type == "gemini" {
        return Some(trimmed.trim_start_matches("models/").to_string());
    }
    Some(trimmed.to_string())
}

fn token_limit_label(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(text)) => clean_context_value(text),
        Some(Value::Number(number)) => Some(format!("{number} tokens")),
        _ => None,
    }
}

fn model_supports_images(item: &Value) -> bool {
    let text = serde_json::to_string(item)
        .unwrap_or_default()
        .to_ascii_lowercase();
    ["vision", "image", "multimodal"]
        .iter()
        .any(|needle| text.contains(needle))
}

fn read_model_from_body(body: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
}

fn extract_project_context(headers: &HeaderMap, body: &[u8]) -> ProjectContext {
    let header_project = header_value(
        headers,
        &[
            "x-codexswitch-project",
            "x-codex-project",
            "x-project-id",
            "x-project",
        ],
    );
    let header_workspace = header_value(
        headers,
        &[
            "x-codexswitch-workspace",
            "x-codex-workspace",
            "x-workspace-path",
            "x-workspace",
        ],
    );
    let header_git_remote = header_value(
        headers,
        &[
            "x-codexswitch-git-remote",
            "x-codex-git-remote",
            "x-git-remote",
        ],
    );
    let header_session = header_value(
        headers,
        &[
            "x-codexswitch-session",
            "x-codex-session-id",
            "x-session-id",
        ],
    );

    let needs_body = header_project.is_none()
        || header_workspace.is_none()
        || header_git_remote.is_none()
        || header_session.is_none();
    let body_json = if needs_body && body.len() <= 256 * 1024 {
        serde_json::from_slice::<Value>(body).ok()
    } else {
        None
    };

    let workspace_path =
        header_workspace.or_else(|| body_json.as_ref().and_then(json_workspace_path));
    let project_id = header_project
        .or_else(|| body_json.as_ref().and_then(json_project_id))
        .or_else(|| workspace_path.as_deref().and_then(project_name_from_path))
        .unwrap_or_else(|| "default".to_string());
    let project_name = body_json
        .as_ref()
        .and_then(json_project_name)
        .or_else(|| workspace_path.as_deref().and_then(project_name_from_path))
        .unwrap_or_else(|| project_id.clone());

    ProjectContext {
        project_id,
        project_name,
        workspace_path,
        git_remote: header_git_remote.or_else(|| body_json.as_ref().and_then(json_git_remote)),
        session_id: header_session.or_else(|| body_json.as_ref().and_then(json_session_id)),
    }
}

fn header_value(headers: &HeaderMap, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        headers
            .get(*name)
            .and_then(|value| value.to_str().ok())
            .and_then(clean_context_value)
    })
}

fn clean_context_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn json_project_id(value: &Value) -> Option<String> {
    json_string_at(
        value,
        &[
            "projectId",
            "project_id",
            "codexProject",
            "codex_project",
            "metadata.projectId",
            "metadata.project_id",
            "metadata.codexProject",
        ],
    )
    .or_else(|| json_project_child(value, &["id", "slug", "name"]))
}

fn json_project_name(value: &Value) -> Option<String> {
    json_string_at(
        value,
        &[
            "projectName",
            "project_name",
            "metadata.projectName",
            "metadata.project_name",
        ],
    )
    .or_else(|| json_project_child(value, &["name", "id", "slug"]))
}

fn json_workspace_path(value: &Value) -> Option<String> {
    json_string_at(
        value,
        &[
            "workspacePath",
            "workspace_path",
            "workspace",
            "cwd",
            "metadata.workspacePath",
            "metadata.workspace_path",
            "metadata.cwd",
        ],
    )
}

fn json_git_remote(value: &Value) -> Option<String> {
    json_string_at(
        value,
        &[
            "gitRemote",
            "git_remote",
            "repository",
            "metadata.gitRemote",
            "metadata.git_remote",
            "metadata.repository",
        ],
    )
}

fn json_session_id(value: &Value) -> Option<String> {
    json_string_at(
        value,
        &[
            "sessionId",
            "session_id",
            "conversationId",
            "conversation_id",
            "metadata.sessionId",
            "metadata.session_id",
        ],
    )
}

fn json_project_child(value: &Value, fields: &[&str]) -> Option<String> {
    let project = value.get("project")?;
    json_value_string(project).or_else(|| {
        fields
            .iter()
            .find_map(|field| project.get(*field).and_then(json_value_string))
    })
}

fn json_string_at(value: &Value, paths: &[&str]) -> Option<String> {
    paths.iter().find_map(|path| {
        let mut current = value;
        for segment in path.split('.') {
            current = current.get(segment)?;
        }
        json_value_string(current)
    })
}

fn json_value_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => clean_context_value(text),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn project_name_from_path(path: &str) -> Option<String> {
    let trimmed = path.trim().trim_end_matches(['/', '\\']);
    let name = trimmed.rsplit(['/', '\\']).next()?.trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

fn body_preview_if_enabled(config: &TelemetryConfig, body: &[u8]) -> Option<String> {
    if !config.enabled || !config.capture_bodies || body.is_empty() {
        return None;
    }
    if let Ok(mut value) = serde_json::from_slice::<Value>(body) {
        redact_sensitive_json(&mut value);
        return serde_json::to_string(&value)
            .ok()
            .map(|text| truncate_preview(&text, config.max_body_preview_bytes));
    }
    let text = String::from_utf8_lossy(body);
    Some(truncate_preview(&text, config.max_body_preview_bytes))
}

fn response_preview_if_enabled(
    config: &TelemetryConfig,
    body: &[u8],
    truncated: bool,
) -> Option<String> {
    if !config.enabled || !config.capture_bodies || body.is_empty() {
        return None;
    }
    if !truncated {
        return body_preview_if_enabled(config, body);
    }
    if let Ok(mut value) = serde_json::from_slice::<Value>(body) {
        redact_sensitive_json(&mut value);
        return serde_json::to_string(&value)
            .ok()
            .map(|text| truncate_preview(&text, config.max_body_preview_bytes));
    }
    Some("[TRUNCATED_RESPONSE_PREVIEW_OMITTED]".into())
}

fn redact_sensitive_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, child) in map.iter_mut() {
                if is_sensitive_key(key) {
                    *child = Value::String("[REDACTED]".into());
                } else {
                    redact_sensitive_json(child);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_sensitive_json(item);
            }
        }
        _ => {}
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "credential",
        "key",
        "password",
        "secret",
        "session",
        "token",
    ]
    .iter()
    .any(|marker| key.contains(marker))
}

fn compact_uuid() -> String {
    Uuid::new_v4().simple().to_string()
}

fn hash_gateway_key(raw_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_key.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn slugify(value: &str) -> String {
    let slug = value
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        format!("project-{}", compact_uuid())
    } else {
        slug
    }
}

fn gateway_key_from_headers(headers: &HeaderMap) -> Option<String> {
    for name in [
        "authorization",
        "x-api-key",
        "x-codexswitch-key",
        "x-goog-api-key",
    ] {
        let Some(value) = headers.get(name).and_then(|value| value.to_str().ok()) else {
            continue;
        };
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        if name == "authorization" {
            if let Some(token) = trimmed.strip_prefix("Bearer ") {
                return clean_context_value(token);
            }
            if let Some(token) = trimmed.strip_prefix("bearer ") {
                return clean_context_value(token);
            }
            continue;
        }
        return clean_context_value(trimmed);
    }
    None
}

fn quota_violation(principal: &GatewayPrincipal) -> Option<&'static str> {
    if principal.quota_requests > 0 && principal.used_requests >= principal.quota_requests {
        return Some("request quota exceeded");
    }
    if principal.quota_tokens > 0 && principal.used_tokens >= principal.quota_tokens {
        return Some("token quota exceeded");
    }
    None
}

fn context_for_principal(
    base: &ProjectContext,
    principal: Option<&GatewayPrincipal>,
) -> ProjectContext {
    if let Some(principal) = principal {
        ProjectContext {
            project_id: principal.project_slug.clone(),
            project_name: principal.project_name.clone(),
            workspace_path: base.workspace_path.clone(),
            git_remote: base.git_remote.clone(),
            session_id: base.session_id.clone(),
        }
    } else {
        base.clone()
    }
}

fn request_usage_from_body(body: &[u8], model: Option<&str>) -> UsageEstimate {
    let prompt_tokens = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| {
            usage_prompt_tokens(&value).or_else(|| Some(estimate_json_tokens(&value)))
        })
        .unwrap_or_else(|| estimate_text_tokens(&String::from_utf8_lossy(body)));
    UsageEstimate {
        prompt_tokens,
        completion_tokens: 0,
        total_tokens: prompt_tokens,
        estimated_cost_usd: estimate_cost_usd(model, prompt_tokens, 0),
    }
}

fn response_usage_from_body(
    body: &[u8],
    request_usage: UsageEstimate,
    model: Option<&str>,
) -> UsageEstimate {
    let mut prompt_tokens = request_usage.prompt_tokens;
    let (completion_tokens, total_tokens) = if let Ok(value) = serde_json::from_slice::<Value>(body)
    {
        if let Some(usage) = value.get("usage").or_else(|| value.get("usageMetadata")) {
            prompt_tokens = usage_prompt_tokens(usage).unwrap_or(prompt_tokens);
            let completion_tokens = usage_completion_tokens(usage).unwrap_or_default();
            let total_tokens =
                usage_total_tokens(usage).unwrap_or(prompt_tokens + completion_tokens);
            (completion_tokens, total_tokens)
        } else {
            let completion_tokens = estimate_json_tokens(&value);
            (completion_tokens, prompt_tokens + completion_tokens)
        }
    } else {
        let completion_tokens = estimate_text_tokens(&String::from_utf8_lossy(body));
        (completion_tokens, prompt_tokens + completion_tokens)
    };
    UsageEstimate {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        estimated_cost_usd: estimate_cost_usd(model, prompt_tokens, completion_tokens),
    }
}

fn usage_prompt_tokens(value: &Value) -> Option<u64> {
    [
        "prompt_tokens",
        "input_tokens",
        "promptTokenCount",
        "inputTokens",
    ]
    .iter()
    .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn usage_completion_tokens(value: &Value) -> Option<u64> {
    [
        "completion_tokens",
        "output_tokens",
        "candidatesTokenCount",
        "completionTokens",
        "outputTokens",
    ]
    .iter()
    .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn usage_total_tokens(value: &Value) -> Option<u64> {
    ["total_tokens", "totalTokens", "totalTokenCount"]
        .iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
}

fn estimate_json_tokens(value: &Value) -> u64 {
    estimate_text_tokens(&serde_json::to_string(value).unwrap_or_default())
}

fn estimate_text_tokens(text: &str) -> u64 {
    ((text.chars().count() as u64).saturating_add(3) / 4).max(1)
}

fn estimate_cost_usd(model: Option<&str>, prompt_tokens: u64, completion_tokens: u64) -> f64 {
    let model = model.unwrap_or_default().to_ascii_lowercase();
    let (input_per_million, output_per_million) = if model.contains("claude") {
        (3.0, 15.0)
    } else if model.contains("gemini") {
        (1.25, 5.0)
    } else {
        (0.15, 0.60)
    };
    (prompt_tokens as f64 * input_per_million + completion_tokens as f64 * output_per_million)
        / 1_000_000.0
}

fn build_gateway_usage_summary(
    traces: &[RequestTrace],
    keys: &[GatewayKey],
) -> GatewayUsageSummary {
    let mut by_user: HashMap<String, GatewayUsageBucket> = HashMap::new();
    let mut by_project: HashMap<String, GatewayUsageBucket> = HashMap::new();
    let mut total_requests = 0usize;
    let mut total_tokens = 0u64;
    let mut estimated_cost_usd = 0.0;
    for trace in traces {
        let Some(user_id) = trace.gateway_user_id.as_ref() else {
            continue;
        };
        total_requests += 1;
        total_tokens += trace.total_tokens;
        estimated_cost_usd += trace.estimated_cost_usd;
        let user_name = trace
            .gateway_user_name
            .clone()
            .unwrap_or_else(|| user_id.clone());
        add_gateway_usage_bucket(
            &mut by_user,
            user_id,
            &user_name,
            Some(user_id.clone()),
            Some(user_name.clone()),
            None,
            None,
            trace.total_tokens,
            trace.estimated_cost_usd,
        );
        if let Some(project_id) = trace.gateway_project_id.as_ref() {
            let project_name = trace
                .gateway_project_name
                .clone()
                .unwrap_or_else(|| project_id.clone());
            add_gateway_usage_bucket(
                &mut by_project,
                project_id,
                &project_name,
                Some(user_id.clone()),
                Some(user_name),
                Some(project_id.clone()),
                Some(project_name.clone()),
                trace.total_tokens,
                trace.estimated_cost_usd,
            );
        }
    }
    let mut by_user = by_user.into_values().collect::<Vec<_>>();
    by_user.sort_by(|a, b| {
        b.requests
            .cmp(&a.requests)
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut by_project = by_project.into_values().collect::<Vec<_>>();
    by_project.sort_by(|a, b| {
        b.requests
            .cmp(&a.requests)
            .then_with(|| a.name.cmp(&b.name))
    });
    let mut by_key = keys
        .iter()
        .map(|key| GatewayKeyUsageBucket {
            key_id: key.id.clone(),
            key_name: key.name.clone(),
            user_id: key.user_id.clone(),
            user_name: key.user_name.clone(),
            project_id: key.project_id.clone(),
            project_name: key.project_name.clone(),
            used_requests: key.used_requests,
            used_tokens: key.used_tokens,
            quota_requests: key.quota_requests,
            quota_tokens: key.quota_tokens,
            estimated_cost_usd: traces
                .iter()
                .filter(|trace| trace.gateway_key_id.as_ref() == Some(&key.id))
                .map(|trace| trace.estimated_cost_usd)
                .sum(),
        })
        .collect::<Vec<_>>();
    by_key.sort_by(|a, b| {
        b.used_requests
            .cmp(&a.used_requests)
            .then_with(|| a.key_name.cmp(&b.key_name))
    });
    GatewayUsageSummary {
        total_requests,
        total_tokens,
        estimated_cost_usd,
        by_user,
        by_project,
        by_key,
    }
}

fn add_gateway_usage_bucket(
    buckets: &mut HashMap<String, GatewayUsageBucket>,
    id: &str,
    name: &str,
    user_id: Option<String>,
    user_name: Option<String>,
    project_id: Option<String>,
    project_name: Option<String>,
    tokens: u64,
    cost: f64,
) {
    let bucket = buckets
        .entry(id.to_string())
        .or_insert_with(|| GatewayUsageBucket {
            id: id.to_string(),
            name: name.to_string(),
            user_id,
            user_name,
            project_id,
            project_name,
            requests: 0,
            tokens: 0,
            estimated_cost_usd: 0.0,
        });
    bucket.requests += 1;
    bucket.tokens += tokens;
    bucket.estimated_cost_usd += cost;
}

fn truncate_preview(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn record_proxy_trace(
    database: &Database,
    config: &TelemetryConfig,
    context: &ProjectContext,
    protocol: &str,
    path: &str,
    provider: &Provider,
    upstream: &str,
    request_model: Option<&str>,
    status_code: Option<StatusCode>,
    success: bool,
    latency_ms: u64,
    request_bytes: u64,
    response_bytes: Option<u64>,
    error: Option<String>,
    request_body_preview: Option<String>,
    response_body_preview: Option<String>,
) -> CoreResult<String> {
    record_proxy_trace_with_gateway(
        database,
        config,
        context,
        None,
        UsageEstimate::default(),
        protocol,
        path,
        provider,
        upstream,
        request_model,
        status_code,
        success,
        latency_ms,
        request_bytes,
        response_bytes,
        error,
        request_body_preview,
        response_body_preview,
    )
}

fn record_proxy_trace_with_gateway(
    database: &Database,
    config: &TelemetryConfig,
    context: &ProjectContext,
    principal: Option<&GatewayPrincipal>,
    usage: UsageEstimate,
    protocol: &str,
    path: &str,
    provider: &Provider,
    upstream: &str,
    request_model: Option<&str>,
    status_code: Option<StatusCode>,
    success: bool,
    latency_ms: u64,
    request_bytes: u64,
    response_bytes: Option<u64>,
    error: Option<String>,
    request_body_preview: Option<String>,
    response_body_preview: Option<String>,
) -> CoreResult<String> {
    if !config.enabled {
        return Ok(String::new());
    }
    let capture_level = if request_body_preview.is_some() || response_body_preview.is_some() {
        "body"
    } else {
        "metadata"
    };
    database.add_request_trace(
        RequestTraceDraft {
            project_id: context.project_id.clone(),
            project_name: context.project_name.clone(),
            workspace_path: context.workspace_path.clone(),
            git_remote: context.git_remote.clone(),
            session_id: context.session_id.clone(),
            protocol: protocol.to_string(),
            route_path: path.to_string(),
            provider_id: provider.id.clone(),
            provider_name: provider.name.clone(),
            service_type: provider.service_type.clone(),
            request_model: request_model.map(str::to_string),
            upstream_url: Some(upstream.to_string()),
            status_code: status_code.map(|status| status.as_u16() as i64),
            success,
            latency_ms,
            request_bytes,
            response_bytes,
            error,
            capture_level: capture_level.to_string(),
            request_body_preview,
            response_body_preview,
            gateway_user_id: principal.map(|item| item.user_id.clone()),
            gateway_user_name: principal.map(|item| item.user_name.clone()),
            gateway_project_id: principal.map(|item| item.project_id.clone()),
            gateway_project_name: principal.map(|item| item.project_name.clone()),
            gateway_key_id: principal.map(|item| item.key_id.clone()),
            gateway_key_name: principal.map(|item| item.key_name.clone()),
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
            estimated_cost_usd: usage.estimated_cost_usd,
        },
        config,
    )
}

fn rewrite_model_body(
    body: Bytes,
    provider: &Provider,
    database: Option<&Database>,
) -> CoreResult<Bytes> {
    let mut value: Value = serde_json::from_slice(&body)
        .map_err(|err| CoreError::Config(format!("invalid JSON request body: {err}")))?;
    if let Some(model) = value.get("model").and_then(Value::as_str) {
        let resolved = resolve_provider_model_for_request(database, provider, model)?;
        if resolved != model {
            if let Some(obj) = value.as_object_mut() {
                obj.insert("model".into(), Value::String(resolved));
                return serde_json::to_vec(&value)
                    .map(Bytes::from)
                    .map_err(|err| CoreError::Config(format!("failed to rewrite model: {err}")));
            }
        }
    }
    Ok(body)
}

fn prepare_proxy_request(
    database: &Database,
    protocol: &str,
    path: &str,
    provider: &Provider,
    body: Bytes,
) -> CoreResult<PreparedProxyRequest> {
    if protocol == "claude" && provider.service_type == "openai" {
        return convert_claude_messages_to_openai_chat(database, provider, body);
    }

    Ok(PreparedProxyRequest {
        path: path.to_string(),
        body: rewrite_model_body(body, provider, Some(database))?,
        adapter: None,
    })
}

fn convert_claude_messages_to_openai_chat(
    database: &Database,
    provider: &Provider,
    body: Bytes,
) -> CoreResult<PreparedProxyRequest> {
    let value: Value = serde_json::from_slice(&body)
        .map_err(|err| CoreError::Config(format!("invalid Claude messages JSON: {err}")))?;
    let requested_model = value
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let model = resolve_provider_model_for_claude_adapter(database, provider, requested_model)?;
    let stream = value
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut messages = Vec::new();
    if let Some(system) = value.get("system") {
        let text = claude_content_to_text(system);
        if !text.is_empty() {
            messages.push(json!({ "role": "system", "content": text }));
        }
    }
    if let Some(items) = value.get("messages").and_then(Value::as_array) {
        for item in items {
            let role = item
                .get("role")
                .and_then(Value::as_str)
                .filter(|role| *role == "assistant")
                .unwrap_or("user");
            let content = item
                .get("content")
                .map(claude_content_to_text)
                .unwrap_or_default();
            messages.push(json!({ "role": role, "content": content }));
        }
    }
    if messages.is_empty() {
        messages.push(json!({ "role": "user", "content": "" }));
    }
    let mut payload = json!({
        "model": model,
        "messages": messages,
        "stream": false
    });
    if let Some(max_tokens) = value.get("max_tokens").and_then(Value::as_u64) {
        payload["max_tokens"] = json!(max_tokens);
    }
    if let Some(temperature) = value.get("temperature").and_then(Value::as_f64) {
        payload["temperature"] = json!(temperature);
    }
    serde_json::to_vec(&payload)
        .map(Bytes::from)
        .map(|body| PreparedProxyRequest {
            path: "/v1/chat/completions".into(),
            body,
            adapter: Some(ResponseAdapter::OpenAiChatToClaudeMessages { stream }),
        })
        .map_err(|err| CoreError::Config(format!("failed to build OpenAI chat JSON: {err}")))
}

fn resolve_provider_model_for_claude_adapter(
    database: &Database,
    provider: &Provider,
    requested_model: &str,
) -> CoreResult<String> {
    if let Some(mapped) = provider.model_mapping.get(requested_model) {
        return Ok(mapped.clone());
    }
    if let Some(model) =
        database.provider_model_name_for_reference(&provider.id, requested_model)?
    {
        return Ok(model);
    }
    if let Some(model) = database.provider_default_model_name(&provider.id)? {
        return Ok(model);
    }
    Ok(requested_model.to_string())
}

fn claude_content_to_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    if let Some(items) = value.as_array() {
        return items
            .iter()
            .filter_map(|item| {
                item.get("text")
                    .and_then(Value::as_str)
                    .or_else(|| item.get("content").and_then(Value::as_str))
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}

fn adapt_proxy_response_body(
    adapter: ResponseAdapter,
    body: &[u8],
) -> CoreResult<(&'static str, Bytes)> {
    match adapter {
        ResponseAdapter::OpenAiChatToClaudeMessages { stream } => {
            let value: Value = serde_json::from_slice(body)
                .map_err(|err| CoreError::Config(format!("invalid OpenAI response JSON: {err}")))?;
            let text = value
                .pointer("/choices/0/message/content")
                .and_then(Value::as_str)
                .or_else(|| value.get("output_text").and_then(Value::as_str))
                .unwrap_or_default();
            let model = value
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("codexswitch-adapted");
            let usage = value.get("usage").cloned().unwrap_or_else(|| json!({}));
            let input_tokens = usage
                .get("prompt_tokens")
                .or_else(|| usage.get("input_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or_else(|| estimate_text_tokens(text));
            let output_tokens = usage
                .get("completion_tokens")
                .or_else(|| usage.get("output_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or_else(|| estimate_text_tokens(text));
            if stream {
                Ok((
                    "text/event-stream",
                    Bytes::from(openai_chat_to_claude_sse(
                        text,
                        model,
                        input_tokens,
                        output_tokens,
                    )),
                ))
            } else {
                serde_json::to_vec(&json!({
                    "id": value.get("id").and_then(Value::as_str).unwrap_or("msg_codexswitch"),
                    "type": "message",
                    "role": "assistant",
                    "model": model,
                    "content": [{ "type": "text", "text": text }],
                    "stop_reason": "end_turn",
                    "stop_sequence": null,
                    "usage": {
                        "input_tokens": input_tokens,
                        "output_tokens": output_tokens
                    }
                }))
                .map(Bytes::from)
                .map(|bytes| ("application/json", bytes))
                .map_err(|err| {
                    CoreError::Config(format!("failed to build Claude response JSON: {err}"))
                })
            }
        }
    }
}

fn openai_chat_to_claude_sse(
    text: &str,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> String {
    let message = json!({
        "id": "msg_codexswitch",
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": [],
        "stop_reason": null,
        "stop_sequence": null,
        "usage": { "input_tokens": input_tokens, "output_tokens": 0 }
    });
    [
        sse_event(
            "message_start",
            json!({ "type": "message_start", "message": message }),
        ),
        sse_event(
            "content_block_start",
            json!({
                "type": "content_block_start",
                "index": 0,
                "content_block": { "type": "text", "text": "" }
            }),
        ),
        sse_event(
            "content_block_delta",
            json!({
                "type": "content_block_delta",
                "index": 0,
                "delta": { "type": "text_delta", "text": text }
            }),
        ),
        sse_event(
            "content_block_stop",
            json!({ "type": "content_block_stop", "index": 0 }),
        ),
        sse_event(
            "message_delta",
            json!({
                "type": "message_delta",
                "delta": { "stop_reason": "end_turn", "stop_sequence": null },
                "usage": { "output_tokens": output_tokens }
            }),
        ),
        sse_event("message_stop", json!({ "type": "message_stop" })),
    ]
    .join("")
}

fn sse_event(event: &str, data: Value) -> String {
    format!("event: {event}\ndata: {data}\n\n")
}

fn is_hop_by_hop(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailers"
            | "transfer-encoding"
            | "upgrade"
    )
}

impl Provider {
    fn primary_key(&self) -> Option<String> {
        self.auth_keys().into_iter().next()
    }

    fn auth_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        for key in std::iter::once(&self.api_key).chain(self.api_keys.iter()) {
            let key = key.trim();
            if !key.is_empty() && !key.contains('•') && !keys.iter().any(|existing| existing == key)
            {
                keys.push(key.to_string());
            }
        }
        keys
    }
}

fn write_live_config(provider: &Provider, app_dir: &Path) -> CoreResult<()> {
    match provider.app_type.as_str() {
        "claude" => write_claude_live(provider, app_dir),
        "codex" => write_codex_live(provider, app_dir),
        "gemini" => write_gemini_live(provider, app_dir),
        other => Err(CoreError::Config(format!("unsupported app type: {other}"))),
    }
}

fn write_claude_live(provider: &Provider, app_dir: &Path) -> CoreResult<()> {
    let dir = home_dir().join(".claude");
    let legacy = dir.join("claude.json");
    let path = if legacy.exists() {
        legacy
    } else {
        dir.join("settings.json")
    };
    let mut root = read_json_object(&path).unwrap_or_default();
    let mut env = root
        .remove("env")
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        Value::String(provider.base_url.clone()),
    );
    if let Some(key) = provider.primary_key() {
        env.insert("ANTHROPIC_AUTH_TOKEN".into(), Value::String(key));
    }
    root.insert("env".into(), Value::Object(env));
    backup_then_write_json(&path, &Value::Object(root), app_dir)
}

fn write_codex_live(provider: &Provider, app_dir: &Path) -> CoreResult<()> {
    let dir = home_dir().join(".codex");
    let path = dir.join("config.toml");
    let text = fs::read_to_string(&path).unwrap_or_default();
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| CoreError::Config(format!("invalid Codex config.toml: {err}")))?;

    doc["model_provider"] = toml_edit::value("codexswitch");
    let providers =
        doc["model_providers"].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let table = providers["codexswitch"].or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    table["name"] = toml_edit::value(provider.name.clone());
    table["base_url"] = toml_edit::value(provider.base_url.clone());
    table["wire_api"] = toml_edit::value("responses");
    if let Some(key) = provider.primary_key() {
        table["experimental_bearer_token"] = toml_edit::value(key);
    }
    backup_then_write_text(&path, &doc.to_string(), app_dir)
}

fn write_gemini_live(provider: &Provider, app_dir: &Path) -> CoreResult<()> {
    let dir = home_dir().join(".gemini");
    let env_path = dir.join(".env");
    let mut env = read_env(&env_path).unwrap_or_default();
    env.insert("GOOGLE_GEMINI_BASE_URL".into(), provider.base_url.clone());
    if let Some(key) = provider.primary_key() {
        env.insert("GEMINI_API_KEY".into(), key);
    }
    let env_text = env
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n");

    let settings_path = dir.join("settings.json");
    let mut settings = read_json_object(&settings_path).unwrap_or_default();
    settings.insert(
        "security".into(),
        json!({ "auth": { "selectedType": "gemini-api-key" } }),
    );
    let settings_bytes = serde_json::to_vec_pretty(&Value::Object(settings))
        .map_err(|err| CoreError::Config(format!("failed to serialize JSON: {err}")))?;
    backup_then_write_many(
        vec![
            (env_path, (env_text + "\n").into_bytes()),
            (settings_path, settings_bytes),
        ],
        app_dir,
    )
}

fn backup_then_write_json(path: &Path, value: &Value, app_dir: &Path) -> CoreResult<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|err| CoreError::Config(format!("failed to serialize JSON: {err}")))?;
    backup_then_write(path, &bytes, app_dir)
}

fn backup_then_write_text(path: &Path, text: &str, app_dir: &Path) -> CoreResult<()> {
    backup_then_write(path, text.as_bytes(), app_dir)
}

#[derive(Clone)]
struct FileSnapshot {
    path: PathBuf,
    existed: bool,
    bytes: Vec<u8>,
}

fn backup_then_write_many(files: Vec<(PathBuf, Vec<u8>)>, app_dir: &Path) -> CoreResult<()> {
    let snapshots = files
        .iter()
        .map(|(path, _)| snapshot_file(path))
        .collect::<CoreResult<Vec<_>>>()?;
    let mut written_snapshots = Vec::new();

    for ((path, bytes), snapshot) in files.iter().zip(snapshots.iter()) {
        if let Err(err) = backup_then_write(path, bytes, app_dir) {
            let original_error = err.to_string();
            let rollback_errors = written_snapshots
                .iter()
                .rev()
                .filter_map(|snapshot| restore_snapshot(snapshot).err())
                .map(|err| err.to_string())
                .collect::<Vec<_>>();
            if rollback_errors.is_empty() {
                return Err(err);
            }
            return Err(CoreError::Config(format!(
                "{original_error}; rollback failed: {}",
                rollback_errors.join("; ")
            )));
        }
        written_snapshots.push(snapshot.clone());
    }
    Ok(())
}

fn backup_then_write(path: &Path, bytes: &[u8], app_dir: &Path) -> CoreResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| CoreError::io(parent, err))?;
    }
    if path.exists() {
        let backup_dir = app_dir.join("backups");
        fs::create_dir_all(&backup_dir).map_err(|err| CoreError::io(&backup_dir, err))?;
        let safe = path
            .to_string_lossy()
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect::<String>();
        let backup = backup_dir.join(format!("{}_{}", Utc::now().format("%Y%m%d_%H%M%S"), safe));
        fs::copy(path, &backup).map_err(|err| CoreError::io(&backup, err))?;
    }

    let tmp = path.with_extension("tmp-codexswitch");
    fs::write(&tmp, bytes).map_err(|err| CoreError::io(&tmp, err))?;
    fs::rename(&tmp, path).map_err(|err| CoreError::io(path, err))?;
    Ok(())
}

fn snapshot_file(path: &Path) -> CoreResult<FileSnapshot> {
    if path.exists() {
        let bytes = fs::read(path).map_err(|err| CoreError::io(path, err))?;
        Ok(FileSnapshot {
            path: path.to_path_buf(),
            existed: true,
            bytes,
        })
    } else {
        Ok(FileSnapshot {
            path: path.to_path_buf(),
            existed: false,
            bytes: Vec::new(),
        })
    }
}

fn restore_snapshot(snapshot: &FileSnapshot) -> CoreResult<()> {
    if let Some(parent) = snapshot.path.parent() {
        fs::create_dir_all(parent).map_err(|err| CoreError::io(parent, err))?;
    }
    if snapshot.existed {
        fs::write(&snapshot.path, &snapshot.bytes).map_err(|err| CoreError::io(&snapshot.path, err))
    } else {
        match fs::remove_file(&snapshot.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(CoreError::io(&snapshot.path, err)),
        }
    }
}

fn import_claude_provider() -> CoreResult<Option<Provider>> {
    let dir = home_dir().join(".claude");
    let path = if dir.join("settings.json").exists() {
        dir.join("settings.json")
    } else {
        dir.join("claude.json")
    };
    let Some(root) = read_json_object(&path) else {
        return Ok(None);
    };
    let env = root.get("env").and_then(Value::as_object);
    let base_url = env
        .and_then(|e| e.get("ANTHROPIC_BASE_URL"))
        .and_then(Value::as_str)
        .unwrap_or("https://api.anthropic.com");
    let api_key = env
        .and_then(|e| {
            e.get("ANTHROPIC_AUTH_TOKEN")
                .or_else(|| e.get("ANTHROPIC_API_KEY"))
        })
        .and_then(Value::as_str)
        .unwrap_or_default();
    if api_key.is_empty() && base_url == "https://api.anthropic.com" {
        return Ok(None);
    }
    Ok(Some(Provider {
        id: "claude-imported".into(),
        app_type: "claude".into(),
        name: "Imported Claude Code".into(),
        provider_type: "anthropic".into(),
        service_type: "claude".into(),
        base_url: base_url.into(),
        base_urls: vec![base_url.into()],
        api_key: api_key.into(),
        api_keys: if api_key.is_empty() {
            Vec::new()
        } else {
            vec![api_key.into()]
        },
        status: if api_key.is_empty() {
            "disconnected".into()
        } else {
            "connected".into()
        },
        priority: 1,
        is_current: true,
        ..default_provider()
    }))
}

fn import_codex_provider() -> CoreResult<Option<Provider>> {
    let path = home_dir().join(".codex").join("config.toml");
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(_) => return Ok(None),
    };
    let doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|err| CoreError::Config(format!("invalid Codex config.toml: {err}")))?;
    let active = doc
        .get("model_provider")
        .and_then(|item| item.as_str())
        .unwrap_or("openai");
    let provider = doc
        .get("model_providers")
        .and_then(|providers| providers.get(active));
    let base_url = provider
        .and_then(|p| p.get("base_url"))
        .and_then(|item| item.as_str())
        .unwrap_or("https://api.openai.com");
    let token = provider
        .and_then(|p| p.get("experimental_bearer_token"))
        .and_then(|item| item.as_str())
        .unwrap_or_default();
    if base_url == "https://api.openai.com" && token.is_empty() {
        return Ok(None);
    }
    Ok(Some(Provider {
        id: "codex-imported".into(),
        app_type: "codex".into(),
        name: "Imported Codex".into(),
        provider_type: "openai".into(),
        service_type: "responses".into(),
        base_url: base_url.into(),
        base_urls: vec![base_url.into()],
        api_key: token.into(),
        api_keys: if token.is_empty() {
            Vec::new()
        } else {
            vec![token.into()]
        },
        status: if token.is_empty() {
            "disconnected".into()
        } else {
            "connected".into()
        },
        priority: 2,
        is_current: true,
        ..default_provider()
    }))
}

fn import_gemini_provider() -> CoreResult<Option<Provider>> {
    let path = home_dir().join(".gemini").join(".env");
    let env = match read_env(&path) {
        Some(env) => env,
        None => return Ok(None),
    };
    let base_url = env
        .get("GOOGLE_GEMINI_BASE_URL")
        .map(String::as_str)
        .unwrap_or("https://generativelanguage.googleapis.com");
    let key = env
        .get("GEMINI_API_KEY")
        .map(String::as_str)
        .unwrap_or_default();
    if key.is_empty() && base_url == "https://generativelanguage.googleapis.com" {
        return Ok(None);
    }
    Ok(Some(Provider {
        id: "gemini-imported".into(),
        app_type: "gemini".into(),
        name: "Imported Gemini".into(),
        provider_type: "gemini".into(),
        service_type: "gemini".into(),
        base_url: base_url.into(),
        base_urls: vec![base_url.into()],
        api_key: key.into(),
        api_keys: if key.is_empty() {
            Vec::new()
        } else {
            vec![key.into()]
        },
        status: if key.is_empty() {
            "disconnected".into()
        } else {
            "connected".into()
        },
        priority: 3,
        is_current: true,
        ..default_provider()
    }))
}

fn read_json_object(path: &Path) -> Option<Map<String, Value>> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&text)
        .ok()?
        .as_object()
        .cloned()
}

fn read_env(path: &Path) -> Option<HashMap<String, String>> {
    let text = fs::read_to_string(path).ok()?;
    let mut env = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            env.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    Some(env)
}

fn app_dir() -> PathBuf {
    home_dir().join(APP_DIR)
}

fn home_dir() -> PathBuf {
    if let Ok(home) = std::env::var("CODEXSWITCH_TEST_HOME") {
        if !home.trim().is_empty() {
            return PathBuf::from(home);
        }
    }
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn infer_service_type(provider_type: &str) -> String {
    match provider_type {
        "anthropic" => "claude",
        "gemini" => "gemini",
        "openai" | "openrouter" => "responses",
        _ => "responses",
    }
    .to_string()
}

fn infer_app_type(service_type: &str) -> String {
    match service_type {
        "claude" => "claude",
        "gemini" => "gemini",
        _ => "codex",
    }
    .to_string()
}

fn protocol_matches(service_type: &str, protocol: &str) -> bool {
    match protocol {
        "claude" => service_type == "claude" || service_type == "openai",
        "openai" => service_type == "openai" || service_type == "responses",
        other => service_type == other,
    }
}

fn merge_provider(provider: &mut Provider, value: Value) {
    let Some(obj) = value.as_object() else {
        return;
    };
    set_string(obj, "name", &mut provider.name);
    set_string_alias(obj, "type", "providerType", &mut provider.provider_type);
    set_string_alias(
        obj,
        "serviceType",
        "service_type",
        &mut provider.service_type,
    );
    set_string_alias(obj, "appType", "app_type", &mut provider.app_type);
    set_string_alias(obj, "baseUrl", "base_url", &mut provider.base_url);
    if let Some(base_urls) = obj
        .get("baseUrls")
        .or_else(|| obj.get("base_urls"))
        .and_then(Value::as_array)
    {
        provider.base_urls = base_urls
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
    }
    if let Some(api_key) = obj
        .get("apiKey")
        .or_else(|| obj.get("api_key"))
        .and_then(Value::as_str)
    {
        if !api_key.contains('•') {
            provider.api_key = api_key.to_string();
            provider.api_keys = if api_key.is_empty() {
                Vec::new()
            } else {
                vec![api_key.to_string()]
            };
        }
    }
    if let Some(api_keys) = obj
        .get("apiKeys")
        .or_else(|| obj.get("api_keys"))
        .and_then(Value::as_array)
    {
        let parsed = api_keys
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty() && !value.contains('•'))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if !parsed.is_empty() || api_keys.is_empty() {
            provider.api_keys = parsed;
        }
    }
    if let Some(status) = obj.get("status").and_then(Value::as_str) {
        provider.status = status.to_string();
    }
    if let Some(latency) = obj.get("latency").and_then(Value::as_str) {
        provider.latency = Some(latency.to_string());
    }
    if let Some(description) = obj.get("description").and_then(Value::as_str) {
        provider.description = Some(description.to_string());
    }
    set_bool_alias(obj, "skipTls", "skip_tls", &mut provider.skip_tls);
    set_bool_alias(obj, "lowQuality", "low_quality", &mut provider.low_quality);
    set_bool_alias(
        obj,
        "autoBlacklist",
        "auto_blacklist",
        &mut provider.auto_blacklist,
    );
    set_bool_alias(
        obj,
        "codexPassthrough",
        "codex_passthrough",
        &mut provider.codex_passthrough,
    );
    set_bool_alias(
        obj,
        "codexToolsCompat",
        "codex_tools_compat",
        &mut provider.codex_tools_compat,
    );
    set_bool_alias(
        obj,
        "normalizeUserId",
        "normalize_user_id",
        &mut provider.normalize_user_id,
    );
    set_bool_alias(
        obj,
        "normalizeChatRoles",
        "normalize_chat_roles",
        &mut provider.normalize_chat_roles,
    );
    if let Some(reasoning_style) = obj
        .get("reasoningStyle")
        .or_else(|| obj.get("reasoning_style"))
        .and_then(Value::as_str)
    {
        provider.reasoning_style = Some(reasoning_style.to_string());
    }
    if let Some(proxy_url) = obj
        .get("proxyUrl")
        .or_else(|| obj.get("proxy_url"))
        .and_then(Value::as_str)
    {
        provider.proxy_url = Some(proxy_url.to_string());
    }
    if let Some(route_prefix) = obj
        .get("routePrefix")
        .or_else(|| obj.get("route_prefix"))
        .and_then(Value::as_str)
    {
        provider.route_prefix = Some(route_prefix.to_string());
    }
    if let Some(model_mapping) = obj.get("modelMapping").or_else(|| obj.get("model_mapping")) {
        if let Ok(parsed) = serde_json::from_value(model_mapping.clone()) {
            provider.model_mapping = parsed;
        }
    }
    if let Some(headers) = obj
        .get("customHeaders")
        .or_else(|| obj.get("custom_headers"))
    {
        if let Ok(parsed) = serde_json::from_value(headers.clone()) {
            provider.custom_headers = parsed;
        }
    }
    if let Some(priority) = obj.get("priority").and_then(Value::as_i64) {
        provider.priority = priority;
    }
    if let Some(is_current) = obj
        .get("isCurrent")
        .or_else(|| obj.get("is_current"))
        .and_then(Value::as_bool)
    {
        provider.is_current = is_current;
    }
    if let Some(failover_enabled) = obj
        .get("failoverEnabled")
        .or_else(|| obj.get("failover_enabled"))
        .and_then(Value::as_bool)
    {
        provider.failover_enabled = failover_enabled;
    }
    if provider.service_type.is_empty() {
        provider.service_type = infer_service_type(&provider.provider_type);
    }
    if provider.app_type.is_empty() {
        provider.app_type = infer_app_type(&provider.service_type);
    }
}

fn merge_model(model: &mut Model, value: Value) {
    let Some(obj) = value.as_object() else {
        return;
    };
    set_string(obj, "name", &mut model.name);
    set_string_alias(obj, "providerId", "provider_id", &mut model.provider_id);
    set_string_alias(
        obj,
        "contextWindow",
        "context_window",
        &mut model.context_window,
    );
    set_string_alias(obj, "maxOutput", "max_output", &mut model.max_output);
    set_string(obj, "latency", &mut model.latency);
    if let Some(tags) = obj.get("tags").and_then(Value::as_array) {
        model.tags = tags
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }
    if let Some(value) = obj
        .get("isDefault")
        .or_else(|| obj.get("is_default"))
        .and_then(Value::as_bool)
    {
        model.is_default = value;
    }
    if let Some(value) = obj
        .get("supportsImages")
        .or_else(|| obj.get("supports_images"))
        .and_then(Value::as_bool)
    {
        model.supports_images = value;
    }
}

fn merge_agent(agent: &mut Agent, value: Value) {
    let Some(obj) = value.as_object() else {
        return;
    };
    set_string_alias(obj, "providerId", "provider_id", &mut agent.provider_id);
    set_string_alias(obj, "modelId", "model_id", &mut agent.model_id);
    set_string(obj, "status", &mut agent.status);
    if let Some(last_run) = obj
        .get("lastRun")
        .or_else(|| obj.get("last_run"))
        .and_then(Value::as_str)
    {
        agent.last_run = Some(last_run.to_string());
    }
    if let Some(providers) = obj
        .get("allowedProviders")
        .or_else(|| obj.get("allowed_providers"))
        .and_then(Value::as_array)
    {
        agent.allowed_providers = providers
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }
    if let Some(value) = obj
        .get("contextLengthLimit")
        .or_else(|| obj.get("context_length_limit"))
        .and_then(Value::as_u64)
    {
        agent.context_length_limit = Some(value);
    }
    if let Some(value) = obj
        .get("maxOutputLimit")
        .or_else(|| obj.get("max_output_limit"))
        .and_then(Value::as_u64)
    {
        agent.max_output_limit = Some(value);
    }
    if let Some(value) = obj.get("temperature").and_then(Value::as_f64) {
        agent.temperature = Some(value);
    }
    if let Some(value) = obj
        .get("autoFormatConvert")
        .or_else(|| obj.get("auto_format_convert"))
        .and_then(Value::as_bool)
    {
        agent.auto_format_convert = value;
    }
}

fn set_string(obj: &Map<String, Value>, key: &str, target: &mut String) {
    if let Some(value) = obj.get(key).and_then(Value::as_str) {
        *target = value.to_string();
    }
}

fn set_string_alias(obj: &Map<String, Value>, camel: &str, snake: &str, target: &mut String) {
    if let Some(value) = obj
        .get(camel)
        .or_else(|| obj.get(snake))
        .and_then(Value::as_str)
    {
        *target = value.to_string();
    }
}

fn set_bool_alias(obj: &Map<String, Value>, camel: &str, snake: &str, target: &mut bool) {
    if let Some(value) = obj
        .get(camel)
        .or_else(|| obj.get(snake))
        .and_then(Value::as_bool)
    {
        *target = value;
    }
}

fn json_string<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn parse_json<T: for<'a> Deserialize<'a>>(value: String, fallback: T) -> T {
    serde_json::from_str(&value).unwrap_or(fallback)
}

fn mask_secret(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let head = value.chars().take(8).collect::<String>();
    let tail = value
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("{head}••••{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc as StdArc, Mutex as StdMutex, MutexGuard, OnceLock};
    use tempfile::TempDir;

    static TEST_ENV_LOCK: OnceLock<StdMutex<()>> = OnceLock::new();

    fn test_env_lock() -> MutexGuard<'static, ()> {
        TEST_ENV_LOCK
            .get_or_init(|| StdMutex::new(()))
            .lock()
            .unwrap()
    }

    #[test]
    fn masks_api_keys_when_listing() {
        let db = Database::memory().unwrap();
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "test".into(),
            name: "Test".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: "https://example.com".into(),
            api_key: "sk-test-123456789".into(),
            api_keys: vec!["sk-test-123456789".into()],
            status: "connected".into(),
            ..default_provider()
        })
        .unwrap();
        let listed = db.providers(true).unwrap();
        let provider = listed.iter().find(|p| p.id == "test").unwrap();
        assert!(provider.api_key.contains('•'));
        assert!(!provider.api_key.contains("123456789"));
    }

    #[test]
    fn preserves_masked_key_on_update() {
        let db = Database::memory().unwrap();
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "test".into(),
            name: "Test".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: "https://example.com".into(),
            api_key: "sk-original-secret".into(),
            api_keys: vec!["sk-original-secret".into()],
            status: "connected".into(),
            ..default_provider()
        })
        .unwrap();
        db.update_provider(
            "test",
            json!({ "name": "Renamed", "apiKey": "sk-orig••••cret" }),
        )
        .unwrap();
        let provider = db.provider_raw("test").unwrap().unwrap();
        assert_eq!(provider.name, "Renamed");
        assert_eq!(provider.api_key, "sk-original-secret");
    }

    #[test]
    fn updates_provider_routing_fields_without_overwriting_masked_primary_key() {
        let db = Database::memory().unwrap();
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "test".into(),
            name: "Test".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: "https://example.com".into(),
            api_key: "sk-original-secret".into(),
            api_keys: vec!["sk-original-secret".into()],
            status: "connected".into(),
            ..default_provider()
        })
        .unwrap();

        db.update_provider(
            "test",
            json!({
                "apiKey": "sk-orig••••cret",
                "apiKeys": ["sk-next", "sk-ne••••xt"],
                "baseUrls": ["https://a.example.com", "https://b.example.com"],
                "modelMapping": { "codex-mini": "gpt-4o-mini" },
                "routePrefix": "/route",
                "priority": 7,
                "failoverEnabled": true
            }),
        )
        .unwrap();

        let provider = db.provider_raw("test").unwrap().unwrap();
        assert_eq!(provider.api_key, "sk-original-secret");
        assert_eq!(provider.api_keys, vec!["sk-next"]);
        assert_eq!(
            provider.base_urls,
            vec!["https://a.example.com", "https://b.example.com"]
        );
        assert_eq!(
            provider.model_mapping.get("codex-mini").map(String::as_str),
            Some("gpt-4o-mini")
        );
        assert_eq!(provider.route_prefix.as_deref(), Some("/route"));
        assert_eq!(provider.priority, 7);
        assert!(provider.failover_enabled);
    }

    #[test]
    fn normalized_provider_indexes_and_app_settings_are_kept_in_sync() {
        let db = Database::memory().unwrap();
        db.initialize().unwrap();
        assert!(db.app_settings_count().unwrap() >= 3);

        db.add_provider(&Provider {
            id: "schema-provider".into(),
            app_type: "codex".into(),
            name: "Schema Provider".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: "https://a.example.com".into(),
            base_urls: vec![
                "https://a.example.com".into(),
                "https://b.example.com".into(),
            ],
            api_key: "sk-a".into(),
            api_keys: vec!["sk-a".into(), "sk-b".into()],
            status: "connected".into(),
            is_current: true,
            ..default_provider()
        })
        .unwrap();

        assert_eq!(db.provider_endpoint_count("schema-provider").unwrap(), 2);
        assert_eq!(db.provider_key_count("schema-provider").unwrap(), 2);
        assert_eq!(
            db.current_provider_for_app("codex").unwrap().as_deref(),
            Some("schema-provider")
        );

        db.update_provider(
            "schema-provider",
            json!({
                "baseUrls": ["https://only.example.com"],
                "apiKey": "sk-only",
                "apiKeys": ["sk-only"],
                "isCurrent": false
            }),
        )
        .unwrap();
        assert_eq!(db.provider_endpoint_count("schema-provider").unwrap(), 1);
        assert_eq!(db.provider_key_count("schema-provider").unwrap(), 1);
        assert_eq!(db.current_provider_for_app("codex").unwrap(), None);

        db.delete_provider("schema-provider").unwrap();
        assert_eq!(db.provider_endpoint_count("schema-provider").unwrap(), 0);
        assert_eq!(db.provider_key_count("schema-provider").unwrap(), 0);
    }

    #[test]
    fn initialization_adds_missing_codex_desktop_agent_without_overwriting_existing_agents() {
        let db = Database::memory().unwrap();
        db.initialize().unwrap();

        db.with_conn(|conn| {
            conn.execute("DELETE FROM agents WHERE id = 'codex-desktop'", [])?;
            conn.execute(
                "UPDATE agents SET name = 'Custom Codex CLI' WHERE id = 'codex'",
                [],
            )?;
            Ok(())
        })
        .unwrap();

        db.initialize().unwrap();

        let agents = db.agents().unwrap();
        let desktop = agents.iter().find(|agent| agent.id == "codex-desktop");
        assert!(desktop.is_some());
        assert_eq!(desktop.unwrap().provider_id, "codex-openai");
        assert_eq!(
            agents
                .iter()
                .find(|agent| agent.id == "codex")
                .map(|agent| agent.name.as_str()),
            Some("Custom Codex CLI")
        );
    }

    #[test]
    fn writes_codex_config_without_touching_auth_json() {
        let _guard = test_env_lock();
        let tmp = TempDir::new().unwrap();
        std::env::set_var("CODEXSWITCH_TEST_HOME", tmp.path());
        let codex_dir = tmp.path().join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("auth.json"), r#"{"auth_mode":"chatgpt"}"#).unwrap();
        fs::write(codex_dir.join("config.toml"), "model = \"gpt-5\"\n").unwrap();
        let app_dir = tmp.path().join(APP_DIR);
        fs::create_dir_all(&app_dir).unwrap();

        write_codex_live(
            &Provider {
                id: "codex-test".into(),
                app_type: "codex".into(),
                name: "Codex Test".into(),
                provider_type: "openai".into(),
                service_type: "responses".into(),
                base_url: "https://proxy.example.com".into(),
                api_key: "sk-test".into(),
                api_keys: vec!["sk-test".into()],
                ..default_provider()
            },
            &app_dir,
        )
        .unwrap();

        let auth = fs::read_to_string(codex_dir.join("auth.json")).unwrap();
        let config = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert_eq!(auth, r#"{"auth_mode":"chatgpt"}"#);
        assert!(config.contains("model_provider = \"codexswitch\""));
        assert!(config.contains("https://proxy.example.com"));
        let backups = fs::read_dir(app_dir.join("backups")).unwrap().count();
        assert!(backups >= 1);
        std::env::remove_var("CODEXSWITCH_TEST_HOME");
    }

    #[test]
    fn switch_provider_writes_codex_config_and_current_marker() {
        let _guard = test_env_lock();
        let tmp = TempDir::new().unwrap();
        std::env::set_var("CODEXSWITCH_TEST_HOME", tmp.path());
        let codex_dir = tmp.path().join(".codex");
        fs::create_dir_all(&codex_dir).unwrap();
        fs::write(codex_dir.join("auth.json"), r#"{"auth_mode":"chatgpt"}"#).unwrap();
        fs::write(codex_dir.join("config.toml"), "model = \"gpt-5\"\n").unwrap();

        let db = Database::memory().unwrap();
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "codex-switch".into(),
            app_type: "codex".into(),
            name: "Codex Switch".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: "https://switch.example.com".into(),
            api_key: "sk-switch".into(),
            api_keys: vec!["sk-switch".into()],
            status: "connected".into(),
            ..default_provider()
        })
        .unwrap();

        db.switch_provider("codex-switch").unwrap();

        let auth = fs::read_to_string(codex_dir.join("auth.json")).unwrap();
        let config = fs::read_to_string(codex_dir.join("config.toml")).unwrap();
        assert_eq!(auth, r#"{"auth_mode":"chatgpt"}"#);
        assert!(config.contains("model_provider = \"codexswitch\""));
        assert!(config.contains("https://switch.example.com"));
        assert!(config.contains("experimental_bearer_token = \"sk-switch\""));
        assert_eq!(
            db.current_provider_for_app("codex").unwrap().as_deref(),
            Some("codex-switch")
        );
        std::env::remove_var("CODEXSWITCH_TEST_HOME");
    }

    #[test]
    fn gemini_config_write_rolls_back_env_when_settings_write_fails() {
        let _guard = test_env_lock();
        let tmp = TempDir::new().unwrap();
        std::env::set_var("CODEXSWITCH_TEST_HOME", tmp.path());
        let gemini_dir = tmp.path().join(".gemini");
        fs::create_dir_all(&gemini_dir).unwrap();
        fs::write(
            gemini_dir.join(".env"),
            "GEMINI_API_KEY=old\nGOOGLE_GEMINI_BASE_URL=https://old.example.com\n",
        )
        .unwrap();
        fs::write(
            gemini_dir.join("settings.json"),
            r#"{"security":{"auth":{"selectedType":"oauth"}}}"#,
        )
        .unwrap();
        fs::create_dir(gemini_dir.join("settings.tmp-codexswitch")).unwrap();
        let app_dir = tmp.path().join(APP_DIR);
        fs::create_dir_all(&app_dir).unwrap();

        let err = write_gemini_live(
            &Provider {
                id: "gemini-test".into(),
                app_type: "gemini".into(),
                name: "Gemini Test".into(),
                provider_type: "gemini".into(),
                service_type: "gemini".into(),
                base_url: "https://new.example.com".into(),
                api_key: "new-key".into(),
                api_keys: vec!["new-key".into()],
                ..default_provider()
            },
            &app_dir,
        )
        .unwrap_err();

        assert!(err.to_string().contains("settings.tmp-codexswitch"));
        assert_eq!(
            fs::read_to_string(gemini_dir.join(".env")).unwrap(),
            "GEMINI_API_KEY=old\nGOOGLE_GEMINI_BASE_URL=https://old.example.com\n"
        );
        assert_eq!(
            fs::read_to_string(gemini_dir.join("settings.json")).unwrap(),
            r#"{"security":{"auth":{"selectedType":"oauth"}}}"#
        );
        std::env::remove_var("CODEXSWITCH_TEST_HOME");
    }

    #[test]
    fn model_mapping_rewrites_json_body() {
        let mut provider = default_provider();
        provider
            .model_mapping
            .insert("gpt-5.4".into(), "deepseek-v4".into());
        let body = Bytes::from_static(br#"{"model":"gpt-5.4","input":"hello"}"#);
        let rewritten = rewrite_model_body(body, &provider, None).unwrap();
        let value: Value = serde_json::from_slice(&rewritten).unwrap();
        assert_eq!(value["model"], "deepseek-v4");
    }

    #[test]
    fn forwarded_headers_drop_inbound_content_length() {
        let client = reqwest::Client::new();
        let mut headers = HeaderMap::new();
        headers.insert("content-type", HeaderValue::from_static("application/json"));
        headers.insert("content-length", HeaderValue::from_static("38"));

        let request = forward_headers(client.post("http://example.com"), &headers)
            .build()
            .unwrap();

        assert_eq!(
            request.headers().get("content-type").unwrap(),
            "application/json"
        );
        assert!(request.headers().get("content-length").is_none());
    }

    #[test]
    fn route_prefix_is_applied_to_upstream_url() {
        let mut provider = default_provider();
        provider.base_url = "https://proxy.example.com/gateway".into();
        provider.route_prefix = Some("/openai/".into());

        assert_eq!(
            upstream_url(&provider, "/v1/responses"),
            "https://proxy.example.com/gateway/openai/v1/responses"
        );

        provider.base_url = "https://api.example.com/v1".into();
        provider.route_prefix = None;
        assert_eq!(
            upstream_url(&provider, "/v1/responses"),
            "https://api.example.com/v1/responses"
        );
    }

    #[test]
    fn auth_keys_are_deduped_and_masked_values_ignored() {
        let provider = Provider {
            api_key: "sk-live".into(),
            api_keys: vec!["sk-live".into(), "sk-next".into(), "sk-li••••ive".into()],
            ..default_provider()
        };

        assert_eq!(provider.auth_keys(), vec!["sk-live", "sk-next"]);
        assert_eq!(provider.primary_key().as_deref(), Some("sk-live"));
    }

    #[test]
    fn provider_selection_prefers_requested_model_mapping() {
        let db = Database::memory().unwrap();
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "current".into(),
            name: "Current".into(),
            service_type: "responses".into(),
            status: "connected".into(),
            priority: 0,
            is_current: true,
            ..default_provider()
        })
        .unwrap();
        let mut mapped = Provider {
            id: "mapped".into(),
            name: "Mapped".into(),
            service_type: "responses".into(),
            status: "connected".into(),
            priority: 10,
            ..default_provider()
        };
        mapped
            .model_mapping
            .insert("codex-mini".into(), "gpt-4o-mini".into());
        db.add_provider(&mapped).unwrap();

        let providers = db
            .providers_for_protocol("responses", Some("codex-mini"))
            .unwrap();
        assert_eq!(
            providers.first().map(|provider| provider.id.as_str()),
            Some("mapped")
        );
    }

    #[test]
    fn provider_failures_open_circuit_after_threshold() {
        let db = Database::memory().unwrap();
        db.initialize().unwrap();

        db.record_provider_failure("unit-provider", "first")
            .unwrap();
        db.record_provider_failure("unit-provider", "second")
            .unwrap();
        db.record_provider_failure("unit-provider", "third")
            .unwrap();

        let health = db
            .provider_health()
            .unwrap()
            .into_iter()
            .find(|item| item.provider_id == "unit-provider")
            .unwrap();
        assert!(!health.is_healthy);
        assert_eq!(
            health.consecutive_failures,
            CIRCUIT_BREAKER_FAILURE_THRESHOLD
        );
        assert!(health.circuit_open_until.is_some());

        db.record_provider_success("unit-provider", 12).unwrap();
        let health = db
            .provider_health()
            .unwrap()
            .into_iter()
            .find(|item| item.provider_id == "unit-provider")
            .unwrap();
        assert!(health.is_healthy);
        assert_eq!(health.consecutive_failures, 0);
        assert!(health.circuit_open_until.is_none());
    }

    #[test]
    fn provider_selection_skips_open_circuit_and_respects_failover_flag() {
        let db = Database::memory().unwrap();
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "current-unit".into(),
            name: "Current Unit".into(),
            service_type: "unitproto".into(),
            status: "connected".into(),
            is_current: true,
            priority: 0,
            ..default_provider()
        })
        .unwrap();
        db.add_provider(&Provider {
            id: "backup-unit".into(),
            name: "Backup Unit".into(),
            service_type: "unitproto".into(),
            status: "connected".into(),
            failover_enabled: true,
            priority: 1,
            ..default_provider()
        })
        .unwrap();
        db.add_provider(&Provider {
            id: "non-failover-unit".into(),
            name: "Non Failover Unit".into(),
            service_type: "unitproto".into(),
            status: "connected".into(),
            failover_enabled: false,
            priority: 2,
            ..default_provider()
        })
        .unwrap();
        db.force_open_circuit_for_test("current-unit").unwrap();

        let providers = db.providers_for_protocol("unitproto", None).unwrap();
        assert_eq!(
            providers
                .iter()
                .map(|provider| provider.id.as_str())
                .collect::<Vec<_>>(),
            vec!["backup-unit"]
        );

        db.force_open_circuit_for_test("backup-unit").unwrap();
        let providers = db.providers_for_protocol("unitproto", None).unwrap();
        assert_eq!(
            providers
                .iter()
                .map(|provider| provider.id.as_str())
                .collect::<Vec<_>>(),
            vec!["current-unit", "backup-unit"]
        );
    }

    #[test]
    fn provider_selection_excludes_disconnected_providers_without_keys() {
        let db = Database::memory().unwrap();
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "disconnected-claude".into(),
            name: "Disconnected Claude".into(),
            service_type: "claude".into(),
            status: "disconnected".into(),
            priority: 0,
            ..default_provider()
        })
        .unwrap();
        db.add_provider(&Provider {
            id: "connected-openai".into(),
            name: "Connected OpenAI".into(),
            service_type: "openai".into(),
            status: "connected".into(),
            priority: 1,
            ..default_provider()
        })
        .unwrap();

        let providers = db.providers_for_protocol("claude", None).unwrap();
        assert_eq!(
            providers
                .iter()
                .map(|provider| provider.id.as_str())
                .collect::<Vec<_>>(),
            vec!["connected-openai"]
        );
    }

    #[tokio::test]
    async fn proxy_streaming_sse_passthrough_delivers_first_chunk_before_completion() {
        let tmp = TempDir::new().unwrap();

        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/responses",
            post(|| async {
                let stream = async_stream::stream! {
                    yield Ok::<Bytes, std::io::Error>(Bytes::from_static(b"event: response.output_text.delta\ndata: first\n\n"));
                    tokio::time::sleep(std::time::Duration::from_millis(450)).await;
                    yield Ok::<Bytes, std::io::Error>(Bytes::from_static(b"event: response.completed\ndata: done\n\n"));
                };
                Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "text/event-stream")
                    .body(Body::from_stream(stream))
                    .unwrap()
            }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = {
            let _guard = test_env_lock();
            std::env::set_var("CODEXSWITCH_TEST_HOME", tmp.path());
            let db = Database::memory().unwrap();
            db.initialize().unwrap();
            std::env::remove_var("CODEXSWITCH_TEST_HOME");
            db
        };
        db.add_provider(&Provider {
            id: "stream-provider".into(),
            app_type: "codex".into(),
            name: "Stream Provider".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-stream".into(),
            api_keys: vec!["sk-stream".into()],
            status: "connected".into(),
            is_current: true,
            ..default_provider()
        })
        .unwrap();
        db.set_proxy_enabled(true).unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: Arc::new(db),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/v1/responses"))
            .json(&json!({ "model": "gpt-stream", "input": "hello" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .contains("text/event-stream"));

        let mut stream = response.bytes_stream();
        let first = tokio::time::timeout(std::time::Duration::from_millis(200), stream.next())
            .await
            .expect("first SSE chunk should arrive before upstream completes")
            .expect("stream should yield first chunk")
            .unwrap();
        assert!(String::from_utf8_lossy(&first).contains("data: first"));

        let second = tokio::time::timeout(std::time::Duration::from_secs(2), stream.next())
            .await
            .expect("second SSE chunk should arrive")
            .expect("stream should yield second chunk")
            .unwrap();
        assert!(String::from_utf8_lossy(&second).contains("response.completed"));

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn claude_messages_proxy_adapts_to_openai_chat_provider() {
        let seen_request = StdArc::new(StdMutex::new(json!({})));
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/chat/completions",
            post({
                let seen_request = seen_request.clone();
                move |headers: HeaderMap, body: Bytes| {
                    let seen_request = seen_request.clone();
                    async move {
                        let parsed: Value = serde_json::from_slice(&body).unwrap();
                        *seen_request.lock().unwrap() = json!({
                            "authorization": headers
                                .get("authorization")
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or_default(),
                            "body": parsed
                        });
                        Json(json!({
                            "id": "chatcmpl-test",
                            "model": "gpt-4o",
                            "choices": [{
                                "message": { "role": "assistant", "content": "adapted openai reply" }
                            }],
                            "usage": {
                                "prompt_tokens": 11,
                                "completion_tokens": 3,
                                "total_tokens": 14
                            }
                        }))
                    }
                }
            }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "openai-chat".into(),
            app_type: "codex".into(),
            name: "OpenAI Chat".into(),
            provider_type: "openai".into(),
            service_type: "openai".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-upstream".into(),
            api_keys: vec!["sk-upstream".into()],
            status: "connected".into(),
            ..default_provider()
        })
        .unwrap();
        db.add_model(&Model {
            id: "openai-chat-gpt-4o".into(),
            name: "gpt-4o".into(),
            provider_id: "openai-chat".into(),
            tags: vec!["Default".into()],
            is_default: true,
            context_window: "128K".into(),
            latency: "—".into(),
            max_output: "4096".into(),
            supports_images: false,
        })
        .unwrap();
        db.set_proxy_enabled(true).unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/v1/messages"))
            .bearer_auth("codexswitch-local")
            .json(&json!({
                "model": "claude-3-5-sonnet-20241022",
                "messages": [{
                    "role": "user",
                    "content": [{ "type": "text", "text": "hello from claude code" }]
                }],
                "system": [{ "type": "text", "text": "system prompt" }],
                "max_tokens": 123,
                "temperature": 0.2
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body: Value = response.json().await.unwrap();
        assert_eq!(body["type"], "message");
        assert_eq!(body["content"][0]["text"], "adapted openai reply");
        assert_eq!(body["usage"]["input_tokens"], 11);
        assert_eq!(body["usage"]["output_tokens"], 3);

        let seen = seen_request.lock().unwrap().clone();
        assert_eq!(seen["authorization"], "Bearer sk-upstream");
        assert_eq!(seen["body"]["model"], "gpt-4o");
        assert_eq!(seen["body"]["stream"], false);
        assert_eq!(seen["body"]["messages"][0]["role"], "system");
        assert_eq!(
            seen["body"]["messages"][1]["content"],
            "hello from claude code"
        );
        assert_eq!(seen["body"]["max_tokens"], 123);

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn claude_messages_proxy_adapts_openai_chat_to_claude_sse() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/chat/completions",
            post(|| async {
                Json(json!({
                    "id": "chatcmpl-stream-test",
                    "model": "gpt-4o",
                    "choices": [{
                        "message": { "role": "assistant", "content": "stream adapted reply" }
                    }],
                    "usage": {
                        "prompt_tokens": 7,
                        "completion_tokens": 4,
                        "total_tokens": 11
                    }
                }))
            }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "openai-chat-stream".into(),
            app_type: "codex".into(),
            name: "OpenAI Chat Stream".into(),
            provider_type: "openai".into(),
            service_type: "openai".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-upstream".into(),
            api_keys: vec!["sk-upstream".into()],
            status: "connected".into(),
            ..default_provider()
        })
        .unwrap();
        db.add_model(&Model {
            id: "openai-chat-stream-gpt-4o".into(),
            name: "gpt-4o".into(),
            provider_id: "openai-chat-stream".into(),
            tags: vec!["Default".into()],
            is_default: true,
            context_window: "128K".into(),
            latency: "—".into(),
            max_output: "4096".into(),
            supports_images: false,
        })
        .unwrap();
        db.set_proxy_enabled(true).unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/v1/messages"))
            .json(&json!({
                "model": "claude-3-5-sonnet-20241022",
                "messages": [{
                    "role": "user",
                    "content": [{ "type": "text", "text": "hello stream" }]
                }],
                "stream": true
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response
            .headers()
            .get("content-type")
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .contains("text/event-stream"));
        let text = response.text().await.unwrap();
        assert!(text.contains("event: message_start"));
        assert!(text.contains("event: content_block_delta"));
        assert!(text.contains("stream adapted reply"));
        assert!(text.contains("event: message_stop"));

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn chat_send_calls_real_upstream_and_extracts_reply() {
        let seen_request = StdArc::new(StdMutex::new(json!({})));
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/chat/completions",
            post({
                let seen_request = seen_request.clone();
                move |headers: HeaderMap, body: Bytes| {
                    let seen_request = seen_request.clone();
                    async move {
                        let parsed: Value = serde_json::from_slice(&body).unwrap();
                        *seen_request.lock().unwrap() = json!({
                            "authorization": headers
                                .get("authorization")
                                .and_then(|value| value.to_str().ok())
                                .unwrap_or_default(),
                            "body": parsed
                        });
                        Json(json!({
                            "choices": [{
                                "message": { "content": "real upstream sandbox reply" }
                            }]
                        }))
                    }
                }
            }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        let mut provider = Provider {
            id: "chat-provider".into(),
            app_type: "codex".into(),
            name: "Chat Provider".into(),
            provider_type: "openai".into(),
            service_type: "openai".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-chat".into(),
            api_keys: vec!["sk-chat".into()],
            status: "connected".into(),
            ..default_provider()
        };
        provider
            .model_mapping
            .insert("local-chat".into(), "upstream-chat".into());
        db.add_provider(&provider).unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response: Value = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/api/chat/send"))
            .json(&json!({
                "input": "hello sandbox",
                "selectedProviderId": "chat-provider",
                "selectedModelId": "local-chat",
                "temperature": 0.3,
                "maxTokens": 777,
                "lang": "en"
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(response["isSuccess"], true);
        assert_eq!(response["replyContent"], "real upstream sandbox reply");

        let seen = seen_request.lock().unwrap().clone();
        assert_eq!(seen["authorization"], "Bearer sk-chat");
        assert_eq!(seen["body"]["model"], "upstream-chat");
        assert_eq!(seen["body"]["messages"][0]["content"], "hello sandbox");
        assert_eq!(seen["body"]["max_tokens"], 777);

        let health = db
            .provider_health()
            .unwrap()
            .into_iter()
            .find(|item| item.provider_id == "chat-provider")
            .unwrap();
        assert!(health.is_healthy);

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn chat_send_resolves_internal_model_id_to_provider_model_name() {
        let seen_request = StdArc::new(StdMutex::new(json!({})));
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/chat/completions",
            post({
                let seen_request = seen_request.clone();
                move |body: Bytes| {
                    let seen_request = seen_request.clone();
                    async move {
                        *seen_request.lock().unwrap() = serde_json::from_slice(&body).unwrap();
                        Json(json!({
                            "choices": [{
                                "message": { "content": "ok" }
                            }]
                        }))
                    }
                }
            }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "custom-1780571167784".into(),
            app_type: "codex".into(),
            name: "Bailian TokenPlan".into(),
            provider_type: "openai".into(),
            service_type: "openai".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-bailian".into(),
            api_keys: vec!["sk-bailian".into()],
            status: "connected".into(),
            ..default_provider()
        })
        .unwrap();
        db.add_model(&Model {
            id: "custom-1780571167784-deepseek-v4-pro".into(),
            name: "deepseek-v4-pro".into(),
            provider_id: "custom-1780571167784".into(),
            tags: vec!["Standard".into()],
            is_default: true,
            context_window: "128K tokens".into(),
            latency: "".into(),
            max_output: "8192 tokens".into(),
            supports_images: false,
        })
        .unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response: Value = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/api/chat/send"))
            .json(&json!({
                "input": "hello sandbox",
                "selectedProviderId": "custom-1780571167784",
                "selectedModelId": "custom-1780571167784-deepseek-v4-pro",
                "lang": "en"
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(response["isSuccess"], true);
        let seen = seen_request.lock().unwrap().clone();
        assert_eq!(seen["model"], "deepseek-v4-pro");

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn proxy_models_exposes_provider_model_names_not_internal_ids() {
        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_model(&Model {
            id: "custom-1780571167784-deepseek-v4-pro".into(),
            name: "deepseek-v4-pro".into(),
            provider_id: "custom-1780571167784".into(),
            tags: vec!["Standard".into()],
            is_default: true,
            context_window: "128K tokens".into(),
            latency: "".into(),
            max_output: "8192 tokens".into(),
            supports_images: false,
        })
        .unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response: Value = reqwest::Client::new()
            .get(format!("http://{proxy_addr}/v1/models"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let ids = response["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"deepseek-v4-pro"));
        assert!(!ids.contains(&"custom-1780571167784-deepseek-v4-pro"));

        proxy_handle.abort();
    }

    #[tokio::test]
    async fn proxy_models_requires_gateway_key_when_keys_are_issued() {
        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_model(&Model {
            id: "tenant-model-internal".into(),
            name: "tenant-model".into(),
            provider_id: "tenant-provider".into(),
            tags: vec!["Standard".into()],
            is_default: true,
            context_window: "128K tokens".into(),
            latency: "".into(),
            max_output: "8192 tokens".into(),
            supports_images: false,
        })
        .unwrap();
        let user = db
            .add_gateway_user(json!({ "displayName": "Model User" }))
            .unwrap();
        let project = db
            .add_gateway_project(json!({ "name": "Model Project", "userId": user.id }))
            .unwrap();
        let (_key, raw_key) = db
            .add_gateway_key(json!({
                "name": "Model List Key",
                "userId": project.user_id,
                "projectId": project.id,
                "quotaRequests": 10,
                "quotaTokens": 1000
            }))
            .unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });
        let client = reqwest::Client::new();

        let blocked = client
            .get(format!("http://{proxy_addr}/v1/models"))
            .send()
            .await
            .unwrap();
        assert_eq!(blocked.status(), StatusCode::UNAUTHORIZED);

        let response: Value = client
            .get(format!("http://{proxy_addr}/v1/models"))
            .bearer_auth(raw_key)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let ids = response["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect::<Vec<_>>();
        assert!(ids.contains(&"tenant-model"));

        proxy_handle.abort();
    }

    #[tokio::test]
    async fn provider_model_sync_imports_openai_compatible_model_list() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/models",
            get(|| async {
                Json(json!({
                    "object": "list",
                    "data": [
                        { "id": "deepseek-v4-pro", "object": "model", "owned_by": "bailian" },
                        { "id": "qwen3-plus", "object": "model", "owned_by": "bailian" }
                    ]
                }))
            }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "bailian-tokenplan".into(),
            app_type: "codex".into(),
            name: "Bailian TokenPlan".into(),
            provider_type: "openai".into(),
            service_type: "openai".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-bailian".into(),
            api_keys: vec!["sk-bailian".into()],
            status: "connected".into(),
            ..default_provider()
        })
        .unwrap();
        db.add_model(&Model {
            id: "bailian-tokenplan-legacy-deepseek".into(),
            name: "Deepseek-v4-pro".into(),
            provider_id: "bailian-tokenplan".into(),
            tags: vec!["Standard".into()],
            is_default: true,
            context_window: "128K tokens".into(),
            latency: "".into(),
            max_output: "8192 tokens".into(),
            supports_images: false,
        })
        .unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response: Value = reqwest::Client::new()
            .post(format!(
                "http://{proxy_addr}/api/providers/bailian-tokenplan/models/sync"
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(response["success"], true);
        assert_eq!(response["upstreamCount"], 2);
        let imported = response["imported"].as_array().unwrap();
        assert_eq!(imported[0]["name"], "deepseek-v4-pro");
        assert_eq!(imported[0]["id"], "bailian-tokenplan-legacy-deepseek");

        let models = db.models().unwrap();
        assert_eq!(
            models
                .iter()
                .filter(|model| model.provider_id == "bailian-tokenplan"
                    && model.name.eq_ignore_ascii_case("deepseek-v4-pro"))
                .count(),
            1
        );
        assert!(models.iter().any(
            |model| model.provider_id == "bailian-tokenplan" && model.name == "deepseek-v4-pro"
        ));

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn proxy_records_lightweight_project_trace_without_body_capture_by_default() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/responses",
            post(|| async { Json(json!({ "output_text": "ok" })) }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "trace-provider".into(),
            app_type: "codex".into(),
            name: "Trace Provider".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-trace".into(),
            api_keys: vec!["sk-trace".into()],
            status: "connected".into(),
            is_current: true,
            ..default_provider()
        })
        .unwrap();
        db.set_proxy_enabled(true).unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/v1/responses"))
            .header("x-codexswitch-project", "alpha-project")
            .header("x-codexswitch-workspace", "/tmp/alpha-project")
            .json(&json!({ "model": "gpt-trace", "input": "hello" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let _ = response.text().await.unwrap();

        let trace = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT project_id, workspace_path, route_path, request_model, status_code,
                            success, request_bytes, request_body_preview, response_body_preview
                     FROM request_traces LIMIT 1",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, Option<String>>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, i64>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, Option<String>>(7)?,
                            row.get::<_, Option<String>>(8)?,
                        ))
                    },
                )
                .map_err(CoreError::Database)
            })
            .unwrap();

        assert_eq!(trace.0, "alpha-project");
        assert_eq!(trace.1.as_deref(), Some("/tmp/alpha-project"));
        assert_eq!(trace.2, "/v1/responses");
        assert_eq!(trace.3.as_deref(), Some("gpt-trace"));
        assert_eq!(trace.4, 200);
        assert_eq!(trace.5, 1);
        assert!(trace.6 > 0);
        assert_eq!(trace.7, None);
        assert_eq!(trace.8, None);

        let summary: Value = reqwest::Client::new()
            .get(format!("http://{proxy_addr}/api/telemetry/summary"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(summary["totalRequests"], 1);
        assert_eq!(summary["byProject"][0]["projectId"], "alpha-project");

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn telemetry_body_capture_is_opt_in_and_redacts_sensitive_fields() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/chat/completions",
            post(|| async { Json(json!({ "choices": [{ "message": { "content": "ok" } }] })) }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "capture-provider".into(),
            app_type: "codex".into(),
            name: "Capture Provider".into(),
            provider_type: "openai".into(),
            service_type: "openai".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-capture".into(),
            api_keys: vec!["sk-capture".into()],
            status: "connected".into(),
            is_current: true,
            ..default_provider()
        })
        .unwrap();
        db.set_proxy_enabled(true).unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let config_response: Value = reqwest::Client::new()
            .put(format!("http://{proxy_addr}/api/telemetry/config"))
            .json(&json!({ "captureBodies": true, "retentionDays": 14 }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(config_response["captureBodies"], true);

        let response = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/v1/chat/completions"))
            .header("x-codexswitch-project", "capture-project")
            .json(&json!({
                "model": "gpt-capture",
                "api_key": "secret-value",
                "messages": [{ "role": "user", "content": "hello" }]
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let _ = response.text().await.unwrap();

        let preview: String = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT request_body_preview FROM request_traces WHERE project_id = 'capture-project'",
                    [],
                    |row| row.get(0),
                )
                .map_err(CoreError::Database)
            })
            .unwrap();
        assert!(preview.contains("gpt-capture"));
        assert!(preview.contains("[REDACTED]"));
        assert!(!preview.contains("secret-value"));

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn telemetry_returns_project_request_response_pairs_in_sequence() {
        let response_count = StdArc::new(StdMutex::new(0usize));
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/responses",
            post({
                let response_count = response_count.clone();
                move |body: Bytes| {
                    let response_count = response_count.clone();
                    async move {
                        let parsed: Value = serde_json::from_slice(&body).unwrap();
                        let mut count = response_count.lock().unwrap();
                        *count += 1;
                        Json(json!({
                            "output_text": format!("ordered-response-{}", *count),
                            "echo": parsed.get("input").cloned().unwrap_or(Value::Null),
                            "api_key": format!("upstream-secret-{}", *count)
                        }))
                    }
                }
            }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "ordered-provider".into(),
            app_type: "codex".into(),
            name: "Ordered Provider".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-ordered".into(),
            api_keys: vec!["sk-ordered".into()],
            status: "connected".into(),
            is_current: true,
            ..default_provider()
        })
        .unwrap();
        db.set_proxy_enabled(true).unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let client = reqwest::Client::new();
        let config_response: Value = client
            .put(format!("http://{proxy_addr}/api/telemetry/config"))
            .json(&json!({ "captureBodies": true, "retentionDays": 14 }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(config_response["captureBodies"], true);

        for index in 1..=3 {
            let response = client
                .post(format!("http://{proxy_addr}/v1/responses"))
                .header("x-codexswitch-project", "ordered-project")
                .header("x-codexswitch-workspace", "/tmp/ordered-project")
                .json(&json!({
                    "model": "ordered-model",
                    "input": format!("ordered-request-{index}"),
                    "api_key": format!("request-secret-{index}")
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let body = response.text().await.unwrap();
            assert!(body.contains(&format!("ordered-response-{index}")));
        }

        let traces: Value = client
            .get(format!(
                "http://{proxy_addr}/api/telemetry/traces?projectId=ordered-project&order=asc"
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let traces = traces.as_array().unwrap();
        assert_eq!(traces.len(), 3);
        for (offset, trace) in traces.iter().enumerate() {
            let index = offset + 1;
            assert_eq!(trace["projectSequence"], index);
            assert_eq!(trace["routePath"], "/v1/responses");
            assert!(trace["requestBodyPreview"]
                .as_str()
                .unwrap()
                .contains(&format!("ordered-request-{index}")));
            assert!(trace["requestBodyPreview"]
                .as_str()
                .unwrap()
                .contains("[REDACTED]"));
            assert!(!trace["requestBodyPreview"]
                .as_str()
                .unwrap()
                .contains("request-secret"));
            assert!(trace["responseBodyPreview"]
                .as_str()
                .unwrap()
                .contains(&format!("ordered-response-{index}")));
            assert!(trace["responseBodyPreview"]
                .as_str()
                .unwrap()
                .contains("[REDACTED]"));
            assert!(!trace["responseBodyPreview"]
                .as_str()
                .unwrap()
                .contains("upstream-secret"));
        }

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn issued_gateway_key_required_and_attributed_for_proxy_requests() {
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/responses",
            post(|| async {
                Json(json!({
                    "output_text": "tenant ok",
                    "usage": { "input_tokens": 7, "output_tokens": 3, "total_tokens": 10 }
                }))
            }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "tenant-provider".into(),
            app_type: "codex".into(),
            name: "Tenant Provider".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-upstream".into(),
            api_keys: vec!["sk-upstream".into()],
            status: "connected".into(),
            is_current: true,
            ..default_provider()
        })
        .unwrap();
        db.set_proxy_enabled(true).unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });
        let client = reqwest::Client::new();

        let user: Value = client
            .post(format!("http://{proxy_addr}/api/admin/users"))
            .json(&json!({ "displayName": "Ada Lovelace", "email": "ada@example.com" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let user_id = user["user"]["id"].as_str().unwrap();
        let project: Value = client
            .post(format!("http://{proxy_addr}/api/admin/projects"))
            .json(&json!({ "name": "Compiler Lab", "slug": "compiler-lab", "userId": user_id }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let project_id = project["project"]["id"].as_str().unwrap();
        let key_response: Value = client
            .post(format!("http://{proxy_addr}/api/admin/keys"))
            .json(&json!({
                "name": "Ada Codex",
                "userId": user_id,
                "projectId": project_id,
                "quotaRequests": 10,
                "quotaTokens": 1000
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let raw_key = key_response["rawKey"].as_str().unwrap();
        assert!(raw_key.starts_with("csw_"));

        let blocked = client
            .post(format!("http://{proxy_addr}/v1/responses"))
            .json(&json!({ "model": "tenant-model", "input": "without key" }))
            .send()
            .await
            .unwrap();
        assert_eq!(blocked.status(), StatusCode::UNAUTHORIZED);

        let response = client
            .post(format!("http://{proxy_addr}/v1/responses"))
            .bearer_auth(raw_key)
            .header("x-codexswitch-project", "runtime-project")
            .json(&json!({ "model": "tenant-model", "input": "hello tenant" }))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response.text().await.unwrap().contains("tenant ok"));

        let traces: Value = client
            .get(format!(
                "http://{proxy_addr}/api/telemetry/traces?projectId=compiler-lab&order=asc"
            ))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let trace = &traces.as_array().unwrap()[0];
        assert_eq!(trace["gatewayUserName"], "Ada Lovelace");
        assert_eq!(trace["gatewayProjectName"], "Compiler Lab");
        assert_eq!(trace["gatewayKeyName"], "Ada Codex");
        assert_eq!(trace["promptTokens"], 7);
        assert_eq!(trace["completionTokens"], 3);
        assert_eq!(trace["totalTokens"], 10);
        assert!(trace["estimatedCostUsd"].as_f64().unwrap() > 0.0);

        let usage: Value = client
            .get(format!("http://{proxy_addr}/api/admin/usage/summary"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(usage["totalRequests"], 1);
        assert_eq!(usage["byUser"][0]["userId"], user_id);
        assert_eq!(usage["byProject"][0]["projectId"], project_id);
        assert_eq!(usage["byKey"][0]["usedRequests"], 1);
        assert_eq!(usage["byKey"][0]["usedTokens"], 10);

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn issued_gateway_key_request_quota_blocks_before_upstream_call() {
        let upstream_calls = StdArc::new(StdMutex::new(0usize));
        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/responses",
            post({
                let upstream_calls = upstream_calls.clone();
                move || {
                    let upstream_calls = upstream_calls.clone();
                    async move {
                        *upstream_calls.lock().unwrap() += 1;
                        Json(json!({
                            "output_text": "quota ok",
                            "usage": { "input_tokens": 2, "output_tokens": 1, "total_tokens": 3 }
                        }))
                    }
                }
            }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        db.add_provider(&Provider {
            id: "quota-provider".into(),
            app_type: "codex".into(),
            name: "Quota Provider".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-quota".into(),
            api_keys: vec!["sk-quota".into()],
            status: "connected".into(),
            is_current: true,
            ..default_provider()
        })
        .unwrap();
        db.set_proxy_enabled(true).unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });
        let client = reqwest::Client::new();

        let user: Value = client
            .post(format!("http://{proxy_addr}/api/admin/users"))
            .json(&json!({ "displayName": "Grace Hopper" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let user_id = user["user"]["id"].as_str().unwrap();
        let project: Value = client
            .post(format!("http://{proxy_addr}/api/admin/projects"))
            .json(&json!({ "name": "Quota Project", "userId": user_id }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let project_id = project["project"]["id"].as_str().unwrap();
        let key_response: Value = client
            .post(format!("http://{proxy_addr}/api/admin/keys"))
            .json(&json!({
                "name": "Single Request Key",
                "userId": user_id,
                "projectId": project_id,
                "quotaRequests": 1,
                "quotaTokens": 100
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let raw_key = key_response["rawKey"].as_str().unwrap();

        let first = client
            .post(format!("http://{proxy_addr}/v1/responses"))
            .bearer_auth(raw_key)
            .json(&json!({ "model": "quota-model", "input": "first" }))
            .send()
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);

        let second = client
            .post(format!("http://{proxy_addr}/v1/responses"))
            .bearer_auth(raw_key)
            .json(&json!({ "model": "quota-model", "input": "second" }))
            .send()
            .await
            .unwrap();
        assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(*upstream_calls.lock().unwrap(), 1);

        let usage: Value = client
            .get(format!("http://{proxy_addr}/api/admin/usage/summary"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(usage["byKey"][0]["usedRequests"], 1);
        assert_eq!(usage["byKey"][0]["quotaRequests"], 1);

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn distillation_job_exports_redacted_request_response_pairs() {
        let tmp = TempDir::new().unwrap();
        let db = Arc::new(Database {
            conn: Mutex::new(Connection::open_in_memory().unwrap()),
            app_dir: tmp.path().join(APP_DIR),
        });
        fs::create_dir_all(&db.app_dir).unwrap();
        db.initialize().unwrap();

        let upstream_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let upstream_addr = upstream_listener.local_addr().unwrap();
        let upstream_app = Router::new().route(
            "/v1/responses",
            post(|| async {
                Json(json!({
                    "output_text": "distill answer",
                    "secret": "must-not-export",
                    "usage": { "input_tokens": 5, "output_tokens": 4, "total_tokens": 9 }
                }))
            }),
        );
        let upstream_handle = tokio::spawn(async move {
            let _ = axum::serve(upstream_listener, upstream_app).await;
        });

        db.add_provider(&Provider {
            id: "distill-provider".into(),
            app_type: "codex".into(),
            name: "Distill Provider".into(),
            provider_type: "openai".into(),
            service_type: "responses".into(),
            base_url: format!("http://{upstream_addr}"),
            base_urls: vec![format!("http://{upstream_addr}")],
            api_key: "sk-distill".into(),
            api_keys: vec!["sk-distill".into()],
            status: "connected".into(),
            is_current: true,
            ..default_provider()
        })
        .unwrap();
        db.set_proxy_enabled(true).unwrap();

        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });
        let client = reqwest::Client::new();
        let _config: Value = client
            .put(format!("http://{proxy_addr}/api/telemetry/config"))
            .json(&json!({ "captureBodies": true, "retentionDays": 30 }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        let user: Value = client
            .post(format!("http://{proxy_addr}/api/admin/users"))
            .json(&json!({ "displayName": "Katherine Johnson" }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let user_id = user["user"]["id"].as_str().unwrap();
        let project: Value = client
            .post(format!("http://{proxy_addr}/api/admin/projects"))
            .json(
                &json!({ "name": "Distill Project", "slug": "distill-project", "userId": user_id }),
            )
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let project_id = project["project"]["id"].as_str().unwrap();
        let key_response: Value = client
            .post(format!("http://{proxy_addr}/api/admin/keys"))
            .json(&json!({ "name": "Distill Key", "userId": user_id, "projectId": project_id }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let raw_key = key_response["rawKey"].as_str().unwrap();

        for index in 1..=2 {
            let response = client
                .post(format!("http://{proxy_addr}/v1/responses"))
                .bearer_auth(raw_key)
                .json(&json!({
                    "model": "distill-model",
                    "input": format!("training request {index}"),
                    "api_key": "must-not-export"
                }))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let _ = response.text().await.unwrap();
        }

        let job_response: Value = client
            .post(format!("http://{proxy_addr}/api/admin/distillation/jobs"))
            .json(&json!({
                "name": "Distill Export",
                "projectId": project_id,
                "limit": 10,
                "includeFailures": false
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(job_response["job"]["status"], "completed");
        assert_eq!(job_response["job"]["traceCount"], 2);

        let export_path = job_response["job"]["exportPath"].as_str().unwrap();
        let export_text = fs::read_to_string(export_path).unwrap();
        assert!(export_text.contains("training request 1"));
        assert!(export_text.contains("distill answer"));
        assert!(export_text.contains("[REDACTED]"));
        assert!(!export_text.contains("must-not-export"));

        proxy_handle.abort();
        upstream_handle.abort();
    }

    #[tokio::test]
    async fn gemini_agent_invoke_runs_cli_process_and_returns_output() {
        let _guard = test_env_lock();
        let tmp = TempDir::new().unwrap();
        let fake_gemini = tmp.path().join("gemini");
        let args_path = tmp.path().join("gemini-args.txt");
        fs::write(
            &fake_gemini,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf '{{\"response\":\"fake gemini process ok\",\"stats\":{{\"tools\":{{\"totalCalls\":0}}}}}}\\n'\n",
                args_path.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&fake_gemini, fs::Permissions::from_mode(0o755)).unwrap();
        env::set_var("CODEXSWITCH_GEMINI_CLI_PATH", &fake_gemini);

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response: Value = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/api/agents/gemini-cli/invoke"))
            .json(&json!({
                "prompt": "Return fake process smoke",
                "model": "gemini-test-model"
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(response["success"], true);
        assert_eq!(response["invocation"]["command"], "gemini");
        assert_eq!(response["invocation"]["exitCode"], 0);
        assert_eq!(response["invocation"]["response"], "fake gemini process ok");
        let args = fs::read_to_string(&args_path).unwrap();
        assert!(args.contains("--model"));
        assert!(args.contains("gemini-test-model"));
        assert!(args.contains("--output-format"));
        assert!(args.contains("json"));
        assert!(args.contains("-p"));
        assert!(args.contains("Return fake process smoke"));
        let agent = db
            .agents()
            .unwrap()
            .into_iter()
            .find(|agent| agent.id == "gemini-cli")
            .unwrap();
        assert_eq!(agent.status, "idle");
        assert!(agent.last_run.is_some());

        env::remove_var("CODEXSWITCH_GEMINI_CLI_PATH");
        proxy_handle.abort();
    }

    #[tokio::test]
    async fn claude_agent_invoke_runs_cli_process_and_returns_output() {
        let _guard = test_env_lock();
        let tmp = TempDir::new().unwrap();
        let fake_claude = tmp.path().join("claude");
        let args_path = tmp.path().join("claude-args.txt");
        let env_path = tmp.path().join("claude-env.txt");
        fs::write(
            &fake_claude,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n%s\\n%s\\n' \"${{ANTHROPIC_AUTH_TOKEN:-}}\" \"${{ANTHROPIC_API_KEY:-}}\" \"${{ANTHROPIC_BASE_URL:-}}\" > '{}'\nprintf '{{\"type\":\"result\",\"result\":\"fake claude process ok\",\"session_id\":\"test-session\"}}\\n'\n",
                args_path.display(),
                env_path.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&fake_claude, fs::Permissions::from_mode(0o755)).unwrap();
        env::set_var("CODEXSWITCH_CLAUDE_CLI_PATH", &fake_claude);
        env::set_var("ANTHROPIC_AUTH_TOKEN", "bad-parent-auth-token");
        env::set_var("ANTHROPIC_API_KEY", "bad-parent-api-key");
        env::set_var("ANTHROPIC_BASE_URL", "https://api.anthropic.com");

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response: Value = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/api/agents/claude-code/invoke"))
            .json(&json!({
                "prompt": "Return fake Claude process smoke",
                "model": "claude-test-model"
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(response["success"], true);
        assert_eq!(response["invocation"]["command"], "claude");
        assert_eq!(response["invocation"]["exitCode"], 0);
        assert_eq!(response["invocation"]["response"], "fake claude process ok");
        let args = fs::read_to_string(&args_path).unwrap();
        assert!(args.contains("-p"));
        assert!(args.contains("Return fake Claude process smoke"));
        assert!(args.contains("--output-format"));
        assert!(args.contains("json"));
        assert!(args.contains("--model"));
        assert!(args.contains("claude-test-model"));
        let inherited_env = fs::read_to_string(&env_path).unwrap();
        assert_eq!(
            inherited_env,
            "codexswitch-local\n\nhttp://127.0.0.1:3900\n"
        );
        let agent = db
            .agents()
            .unwrap()
            .into_iter()
            .find(|agent| agent.id == "claude-code")
            .unwrap();
        assert_eq!(agent.status, "idle");
        assert!(agent.last_run.is_some());

        env::remove_var("CODEXSWITCH_CLAUDE_CLI_PATH");
        env::remove_var("ANTHROPIC_AUTH_TOKEN");
        env::remove_var("ANTHROPIC_API_KEY");
        env::remove_var("ANTHROPIC_BASE_URL");
        proxy_handle.abort();
    }

    #[tokio::test]
    async fn claude_agent_invoke_treats_json_is_error_as_failure() {
        let _guard = test_env_lock();
        let tmp = TempDir::new().unwrap();
        let fake_claude = tmp.path().join("claude");
        fs::write(
            &fake_claude,
            "#!/bin/sh\nprintf '{\"type\":\"result\",\"is_error\":true,\"result\":\"fake claude auth failure\"}\\n'\n",
        )
        .unwrap();
        fs::set_permissions(&fake_claude, fs::Permissions::from_mode(0o755)).unwrap();
        env::set_var("CODEXSWITCH_CLAUDE_CLI_PATH", &fake_claude);

        let db = Arc::new(Database::memory().unwrap());
        db.initialize().unwrap();
        let proxy_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_listener.local_addr().unwrap();
        let proxy_state = SharedState {
            database: db.clone(),
        };
        let proxy_handle = tokio::spawn(async move {
            let _ = axum::serve(proxy_listener, router(proxy_state)).await;
        });

        let response: Value = reqwest::Client::new()
            .post(format!("http://{proxy_addr}/api/agents/claude-code/invoke"))
            .json(&json!({
                "prompt": "Return fake Claude auth failure"
            }))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();

        assert_eq!(response["success"], false);
        assert_eq!(response["invocation"]["exitCode"], 0);
        assert_eq!(response["invocation"]["isError"], true);
        assert_eq!(
            response["invocation"]["response"],
            "fake claude auth failure"
        );
        let agent = db
            .agents()
            .unwrap()
            .into_iter()
            .find(|agent| agent.id == "claude-code")
            .unwrap();
        assert_eq!(agent.status, "error");

        env::remove_var("CODEXSWITCH_CLAUDE_CLI_PATH");
        proxy_handle.abort();
    }
}
