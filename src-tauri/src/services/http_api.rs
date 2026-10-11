//! Extension HTTP API micro-service.
//!
//! Embeds an Axum HTTP server inside the Tauri process, sharing the existing
//! tokio runtime.  Provides a local REST API for browser extension → desktop
//! communication: download handoff (`/add`), the popup's task list
//! (`/tasks`) and controls, and media capture (`/media/v2`).
use crate::commands::remote_file::summarize_url_for_log;
use crate::error::AppError;
use crate::services::config::{RuntimeConfigState, DEFAULT_EXTENSION_API_PORT};
use crate::services::tasks::TaskServiceState;
use axum::{
    extract::{Request, State},
    http::{header, HeaderMap, HeaderName, Method, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_store::StoreExt;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;

// ── Request / Response Types ────────────────────────────────────────

pub use super::downloads::contracts::{AddRequest, AddResponse};

/// GET /ping response.
#[derive(Debug, Serialize)]
pub struct PingResponse {
    pub product: &'static str,
    pub status: String,
    pub version: String,
}

/// GET /version response.
#[derive(Debug, Serialize)]
pub struct VersionResponse {
    pub app: String,
    pub engine: crate::engine::supervisor::EnginePhase,
}

/// GET /stat response — mirrors aria2's getGlobalStat for the extension popup.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StatResponse {
    pub download_speed: String,
    pub upload_speed: String,
    pub num_active: String,
    pub num_waiting: String,
    pub num_stopped: String,
    pub num_stopped_total: String,
}

impl From<crate::aria2::types::Aria2GlobalStat> for StatResponse {
    fn from(stat: crate::aria2::types::Aria2GlobalStat) -> Self {
        Self {
            download_speed: stat.download_speed,
            upload_speed: stat.upload_speed,
            num_active: stat.num_active,
            num_waiting: stat.num_waiting,
            num_stopped: stat.num_stopped,
            num_stopped_total: stat.num_stopped_total,
        }
    }
}

/// One row of the extension popup's task list.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopupTask {
    pub gid: String,
    pub name: String,
    pub status: String,
    pub total_length: String,
    pub completed_length: String,
    pub download_speed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// GET /tasks response.
#[derive(Debug, Serialize)]
pub struct TasksResponse {
    pub stat: StatResponse,
    pub tasks: Vec<PopupTask>,
}

/// Finished tasks the popup lists after the unfinished ones, newest first.
const POPUP_FINISHED_TASKS: usize = 10;

/// Generic action response for control endpoints (pause-all, resume-all).
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct ActionResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

// ── Auth Extraction ─────────────────────────────────────────────────

/// Extract and validate the Bearer token from the Authorization header.
///
/// Returns `Ok(())` if:
/// - The server secret is empty (authentication disabled)
/// - The header matches `Bearer {secret}`
///
/// Returns `Err(StatusCode::UNAUTHORIZED)` otherwise.
pub fn validate_bearer_token(headers: &HeaderMap, expected_secret: &str) -> Result<(), StatusCode> {
    // Empty secret = auth disabled (matches aria2 behavior)
    if expected_secret.is_empty() {
        return Ok(());
    }

    let header_value = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let expected = format!("Bearer {expected_secret}");
    if header_value == expected {
        Ok(())
    } else {
        log::warn!("http_api: 401 Unauthorized (invalid or missing Bearer token)");
        Err(StatusCode::UNAUTHORIZED)
    }
}

// ── Axum State ──────────────────────────────────────────────────────

/// Shared state passed to Axum handlers via `State<Arc<ApiContext>>`.
pub struct ApiContext {
    pub app: AppHandle,
}

// ── Router Builder ──────────────────────────────────────────────────

/// Build the Axum router with all routes.
pub fn build_router(ctx: Arc<ApiContext>) -> Router {
    let limits = Arc::new(Limits::new(WRITE_BURST));
    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::AllowOrigin::predicate(|origin, _| {
            origin.to_str().is_ok_and(extension_origin)
        }))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::CONTENT_TYPE,
            header::AUTHORIZATION,
            HeaderName::from_static("x-rayburst-client"),
        ])
        .allow_private_network(true);

    Router::new()
        .route("/ping", get(handle_ping))
        .route("/add", post(handle_add))
        .route("/downloads/capabilities", get(handle_download_capabilities))
        .route("/version", get(handle_version))
        .route("/stat", get(handle_stat))
        .route("/tasks", get(handle_tasks))
        .route("/tasks/{gid}/pause", post(handle_task_pause))
        .route("/tasks/{gid}/resume", post(handle_task_resume))
        .route("/tasks/{gid}/reveal", post(handle_task_reveal))
        .route("/pause-all", post(handle_pause_all))
        .route("/resume-all", post(handle_resume_all))
        .layer(axum::extract::DefaultBodyLimit::max(256 * 1024))
        .nest("/media/v2", super::media::routes::router())
        .layer(middleware::from_fn(move |request, next| {
            limit(Arc::clone(&limits), request, next)
        }))
        .layer(middleware::from_fn(require_product_client))
        .layer(cors)
        .with_state(ctx)
}

pub(super) fn extension_origin(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        matches!(url.scheme(), "chrome-extension" | "moz-extension")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.path().is_empty()
            && url.query().is_none()
            && url.fragment().is_none()
    })
}

async fn require_product_client(request: Request, next: Next) -> Response {
    let public = matches!(request.uri().path(), "/ping" | "/version");
    let origin = request.headers().get(header::ORIGIN);
    if origin.is_some_and(|value| !value.to_str().is_ok_and(extension_origin)) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let browser = origin.is_some();
    let product_client = request
        .headers()
        .get("x-rayburst-client")
        .and_then(|value| value.to_str().ok())
        == Some("rayburst-connect");
    if browser && !public && !product_client && request.method() != Method::OPTIONS {
        return StatusCode::BAD_REQUEST.into_response();
    }
    next.run(request).await
}

// ── Resource limits ─────────────────────────────────────────────────

/// Requests in flight per kind. A held confirmation keeps its `/add` open for
/// up to two minutes, so writes allow far more than a page of downloads.
const MAX_READS_IN_FLIGHT: usize = 64;
const MAX_WRITES_IN_FLIGHT: usize = 512;
/// Mutating requests may come in a burst of this size, refilled per second.
const WRITE_BURST: f64 = 256.0;
const WRITE_REFILL_PER_SECOND: f64 = 32.0;

/// Bounds the work clients can start, whatever they send.
struct Limits {
    reads: Arc<tokio::sync::Semaphore>,
    writes: Arc<tokio::sync::Semaphore>,
    budget: std::sync::Mutex<(f64, std::time::Instant)>,
}

impl Limits {
    fn new(burst: f64) -> Self {
        Self {
            reads: Arc::new(tokio::sync::Semaphore::new(MAX_READS_IN_FLIGHT)),
            writes: Arc::new(tokio::sync::Semaphore::new(MAX_WRITES_IN_FLIGHT)),
            budget: std::sync::Mutex::new((burst, std::time::Instant::now())),
        }
    }

    /// Takes one mutating request from the refilling budget.
    fn spend(&self, burst: f64) -> bool {
        let mut budget = self
            .budget
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let (tokens, at) = &mut *budget;
        let now = std::time::Instant::now();
        *tokens =
            (*tokens + now.duration_since(*at).as_secs_f64() * WRITE_REFILL_PER_SECOND).min(burst);
        *at = now;
        if *tokens < 1.0 {
            return false;
        }
        *tokens -= 1.0;
        true
    }
}

async fn limit(limits: Arc<Limits>, request: Request, next: Next) -> Response {
    if request.method() == Method::OPTIONS {
        return next.run(request).await;
    }
    let write = request.method() == Method::POST;
    if write && !limits.spend(WRITE_BURST) {
        log::warn!(
            "http_api: write budget exhausted path={}",
            request.uri().path()
        );
        return too_many_requests();
    }
    let permits = if write { &limits.writes } else { &limits.reads };
    let Ok(_permit) = Arc::clone(permits).try_acquire_owned() else {
        log::warn!("http_api: too many requests in flight write={write}");
        return too_many_requests();
    };
    next.run(request).await
}

fn too_many_requests() -> Response {
    (StatusCode::TOO_MANY_REQUESTS, [(header::RETRY_AFTER, "1")]).into_response()
}

// ── Handlers ────────────────────────────────────────────────────────

async fn handle_ping(State(ctx): State<Arc<ApiContext>>) -> impl IntoResponse {
    let version = ctx.app.package_info().version.to_string();
    Json(PingResponse {
        product: "rayburst",
        status: "ok".to_string(),
        version,
    })
}

async fn handle_download_capabilities(
    State(ctx): State<Arc<ApiContext>>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    validate_bearer_token(&headers, &read_api_secret(&ctx.app))
        .map_err(|status| (status, Json(serde_json::Value::Null)))?;
    let engine = ctx
        .app
        .try_state::<TaskServiceState>()
        .ok_or_else(|| engine_unavailable(&ctx.app))?;
    engine
        .0
        .get_version()
        .await
        .map_err(|_| engine_unavailable(&ctx.app))?;
    // 3: browser downloads are verified by the engine and held until /add
    // answers; no receipts.
    Ok(Json(
        serde_json::json!({"product":"rayburst","protocolVersion":3}),
    ))
}

async fn handle_add(
    State(ctx): State<Arc<ApiContext>>,
    headers: HeaderMap,
    Json(body): Json<AddRequest>,
) -> Result<Json<AddResponse>, StatusCode> {
    let secret = read_api_secret(&ctx.app);
    validate_bearer_token(&headers, &secret)?;

    log::info!(
        "http_api: POST /add url={} final_url={} header_count={} has_user_agent={} has_cookie={} verify={} filename={}",
        summarize_url_for_log(&body.url),
        body.final_url
            .as_deref()
            .map(summarize_url_for_log)
            .unwrap_or_else(|| "none".to_string()),
        body.request_headers.len(),
        body.user_agent.as_ref().is_some_and(|v| !v.is_empty()),
        body.cookie.as_ref().is_some_and(|v| !v.is_empty()),
        body.verify,
        if body.filename.as_ref().is_some_and(|v| !v.is_empty()) {
            "present"
        } else {
            "none"
        },
    );

    super::downloads::dispatch(&ctx.app, body)
        .await
        .map(Json)
        .map_err(|error| {
            log::warn!("http_api: download handoff failed: {error}");
            match error {
                AppError::InvalidInput(_) => StatusCode::BAD_REQUEST,
                AppError::Conflict(_) => StatusCode::CONFLICT,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            }
        })
}

async fn handle_version(State(ctx): State<Arc<ApiContext>>) -> impl IntoResponse {
    let app_version = ctx.app.package_info().version.to_string();

    Json(VersionResponse {
        app: app_version,
        engine: engine_phase(&ctx.app),
    })
}

/// GET /stat — global download/upload statistics.
///
/// Returns the same shape as aria2's `getGlobalStat`, allowing the
/// extension popup to display speed and task counts without needing
/// a direct aria2 RPC connection.
async fn handle_stat(
    State(ctx): State<Arc<ApiContext>>,
    headers: HeaderMap,
) -> Result<Json<StatResponse>, (StatusCode, Json<serde_json::Value>)> {
    let secret = read_api_secret(&ctx.app);
    validate_bearer_token(&headers, &secret)
        .map_err(|status| (status, Json(serde_json::Value::Null)))?;

    let aria2 = ctx
        .app
        .try_state::<TaskServiceState>()
        .ok_or_else(|| engine_unavailable(&ctx.app))?;

    match aria2.0.get_global_stat().await {
        Ok(stat) => Ok(Json(stat.into())),
        Err(e) => {
            log::error!("http_api: get_global_stat failed: {e}");
            Err(engine_unavailable(&ctx.app))
        }
    }
}

/// GET /tasks — unfinished tasks in queue order, then the latest finished.
async fn handle_tasks(
    State(ctx): State<Arc<ApiContext>>,
    headers: HeaderMap,
) -> Result<Json<TasksResponse>, (StatusCode, Json<serde_json::Value>)> {
    validate_bearer_token(&headers, &read_api_secret(&ctx.app))
        .map_err(|status| (status, Json(serde_json::Value::Null)))?;
    let aria2 = ctx
        .app
        .try_state::<TaskServiceState>()
        .ok_or_else(|| engine_unavailable(&ctx.app))?;
    let (stat, tasks) = aria2
        .0
        .snapshot(true)
        .await
        .map_err(|_| engine_unavailable(&ctx.app))?;
    let (finished, unfinished): (Vec<_>, Vec<_>) = tasks
        .into_iter()
        .partition(|task| matches!(task.status.as_str(), "complete" | "error" | "removed"));
    let tasks = unfinished
        .into_iter()
        .chain(finished.into_iter().rev().take(POPUP_FINISHED_TASKS))
        .map(|task| PopupTask {
            name: super::monitor::TaskEvent::extract_name(&task),
            gid: task.gid,
            status: task.status,
            total_length: task.total_length,
            completed_length: task.completed_length,
            download_speed: task.download_speed,
            error_message: task.error_message.filter(|message| !message.is_empty()),
        })
        .collect();
    Ok(Json(TasksResponse {
        stat: stat.into(),
        tasks,
    }))
}

/// POST /tasks/{gid}/pause
async fn handle_task_pause(
    State(ctx): State<Arc<ApiContext>>,
    headers: HeaderMap,
    axum::extract::Path(gid): axum::extract::Path<String>,
) -> Result<Json<ActionResponse>, (StatusCode, Json<serde_json::Value>)> {
    task_action(&ctx, &headers, &gid, false).await
}

/// POST /tasks/{gid}/resume
async fn handle_task_resume(
    State(ctx): State<Arc<ApiContext>>,
    headers: HeaderMap,
    axum::extract::Path(gid): axum::extract::Path<String>,
) -> Result<Json<ActionResponse>, (StatusCode, Json<serde_json::Value>)> {
    task_action(&ctx, &headers, &gid, true).await
}

/// POST /tasks/{gid}/reveal — shows the task's file, or its folder, on the desktop.
async fn handle_task_reveal(
    State(ctx): State<Arc<ApiContext>>,
    headers: HeaderMap,
    axum::extract::Path(gid): axum::extract::Path<String>,
) -> Result<Json<ActionResponse>, (StatusCode, Json<serde_json::Value>)> {
    validate_bearer_token(&headers, &read_api_secret(&ctx.app))
        .map_err(|status| (status, Json(serde_json::Value::Null)))?;
    let aria2 = ctx
        .app
        .try_state::<TaskServiceState>()
        .ok_or_else(|| engine_unavailable(&ctx.app))?;
    let result = match aria2.0.tell_status(&gid).await {
        Ok(task) => {
            let output = super::monitor::TaskEvent::from_aria2(&task).output_path();
            let folder = std::path::PathBuf::from(&task.dir);
            let target = [output, folder].into_iter().find(|path| path.exists());
            match target {
                Some(path) => tauri::async_runtime::spawn_blocking(move || {
                    crate::commands::show_item_in_dir(path.to_string_lossy().into_owned())
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result.map_err(|error| error.to_string())),
                None => Err("The download is no longer on disk".to_string()),
            }
        }
        Err(error) => Err(error.to_string()),
    };
    Ok(Json(match result {
        Ok(()) => ActionResponse {
            status: "ok".to_string(),
            error: None,
        },
        Err(error) => ActionResponse {
            status: "error".to_string(),
            error: Some(error),
        },
    }))
}

async fn task_action(
    ctx: &ApiContext,
    headers: &HeaderMap,
    gid: &str,
    resume: bool,
) -> Result<Json<ActionResponse>, (StatusCode, Json<serde_json::Value>)> {
    validate_bearer_token(headers, &read_api_secret(&ctx.app))
        .map_err(|status| (status, Json(serde_json::Value::Null)))?;
    let aria2 = ctx
        .app
        .try_state::<TaskServiceState>()
        .ok_or_else(|| engine_unavailable(&ctx.app))?;
    let result = if resume {
        super::tasks::bittorrent::resume(&ctx.app, &aria2.0, gid).await
    } else {
        super::tasks::bittorrent::pause(&ctx.app, &aria2.0, Some(gid), false).await
    };
    super::pump::wake(&ctx.app);
    Ok(Json(match result {
        Ok(_) => ActionResponse {
            status: "ok".to_string(),
            error: None,
        },
        Err(error) => ActionResponse {
            status: "error".to_string(),
            error: Some(error.to_string()),
        },
    }))
}

fn engine_phase(app: &AppHandle) -> crate::engine::supervisor::EnginePhase {
    app.try_state::<crate::engine::supervisor::EngineSupervisor>()
        .map(|supervisor| supervisor.snapshot().phase)
        .unwrap_or(crate::engine::supervisor::EnginePhase::Stopped)
}

fn engine_unavailable(app: &AppHandle) -> (StatusCode, Json<serde_json::Value>) {
    use crate::engine::supervisor::EnginePhase;
    let error = match engine_phase(app) {
        EnginePhase::Starting | EnginePhase::Probing | EnginePhase::Recovering => "engine_starting",
        _ => "engine_unavailable",
    };
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"error": error})),
    )
}

/// POST /pause-all — pause all active downloads.
async fn handle_pause_all(
    State(ctx): State<Arc<ApiContext>>,
    headers: HeaderMap,
) -> Result<Json<ActionResponse>, StatusCode> {
    let secret = read_api_secret(&ctx.app);
    validate_bearer_token(&headers, &secret)?;

    log::info!("http_api: POST /pause-all");

    let aria2 = match ctx.app.try_state::<TaskServiceState>() {
        Some(s) => s,
        None => {
            return Ok(Json(ActionResponse {
                status: "error".to_string(),
                error: Some("Engine not running".to_string()),
            }));
        }
    };

    match super::tasks::bittorrent::pause(&ctx.app, &aria2.0, None, true).await {
        Ok(_) => Ok(Json(ActionResponse {
            status: "ok".to_string(),
            error: None,
        })),
        Err(e) => Ok(Json(ActionResponse {
            status: "error".to_string(),
            error: Some(e.to_string()),
        })),
    }
}

/// POST /resume-all — resume all paused downloads.
async fn handle_resume_all(
    State(ctx): State<Arc<ApiContext>>,
    headers: HeaderMap,
) -> Result<Json<ActionResponse>, StatusCode> {
    let secret = read_api_secret(&ctx.app);
    validate_bearer_token(&headers, &secret)?;

    log::info!("http_api: POST /resume-all");

    let aria2 = match ctx.app.try_state::<TaskServiceState>() {
        Some(s) => s,
        None => {
            return Ok(Json(ActionResponse {
                status: "error".to_string(),
                error: Some("Engine not running".to_string()),
            }));
        }
    };

    match super::tasks::bittorrent::resume_all(&ctx.app, &aria2.0).await {
        Ok(result) => {
            log::info!(
                "http_api: POST /resume-all resumed={} blocked={}",
                result.resumed,
                result.blocked
            );
            Ok(Json(ActionResponse {
                status: "ok".to_string(),
                error: None,
            }))
        }
        Err(e) => Ok(Json(ActionResponse {
            status: "error".to_string(),
            error: Some(e.to_string()),
        })),
    }
}

// ── Helper Functions ────────────────────────────────────────────────

/// Reads the `extensionApiSecret` for HTTP API authentication.
/// This secret is fully independent from `rpcSecret` (used for aria2 RPC).
/// Returns empty string if not configured (auth disabled).
pub(crate) fn read_api_secret(app: &AppHandle) -> String {
    app.store("config.json")
        .ok()
        .and_then(|s| s.get("preferences"))
        .and_then(|p| {
            p.get("extensionApiSecret")
                .and_then(|v| v.as_str().map(String::from))
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_default()
}

// ── Server Lifecycle ────────────────────────────────────────────────

/// Handle for a running HTTP API server.  Allows graceful shutdown.
pub struct HttpApiHandle {
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    join_handle: tokio::task::JoinHandle<()>,
    port: u16,
    allow_remote_access: bool,
}

impl HttpApiHandle {
    /// The port this server is currently bound to.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Whether this server is bound to all network interfaces.
    pub fn allow_remote_access(&self) -> bool {
        self.allow_remote_access
    }

    /// Signal the server to shut down and wait for it to finish.
    pub async fn stop(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.join_handle.await;
    }
}

/// Tauri managed state for the HTTP API server handle.
pub struct HttpApiState(pub Mutex<Option<HttpApiHandle>>);

impl HttpApiState {
    pub fn new() -> Self {
        Self(Mutex::new(None))
    }
}

/// Spawn the HTTP API server on the given port.
///
/// The server binds locally by default and runs until the returned
/// handle is stopped or the application exits.
pub async fn spawn_http_api(
    app: AppHandle,
    port: u16,
    allow_remote_access: bool,
) -> Result<HttpApiHandle, AppError> {
    let ctx = Arc::new(ApiContext { app });
    let router = build_router(ctx);

    let host = if allow_remote_access {
        [0, 0, 0, 0]
    } else {
        [127, 0, 0, 1]
    };
    let addr = std::net::SocketAddr::from((host, port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| AppError::Io(format!("Failed to bind HTTP API on port {port}: {e}")))?;

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    let join_handle = tokio::spawn(async move {
        let graceful = axum::serve(listener, router).with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        });
        if let Err(e) = graceful.await {
            log::error!("http_api: server error: {e}");
        }
    });

    log::info!("http_api: listening on {addr}");

    Ok(HttpApiHandle {
        shutdown_tx,
        join_handle,
        port,
        allow_remote_access,
    })
}

/// Stop the current HTTP API server (if running) and respawn on `new_port`.
///
/// Used by:
/// - `on_engine_ready()` during startup (idempotent — skipped if already
///   bound to the correct port by the caller)
/// - `restart_http_api` command when the user changes the port at runtime
///
/// The old server is stopped *before* binding the new one because the old
/// and new port may be identical (user changed and reverted), so the
/// listener must be released first.
pub async fn restart_on_port(app: &AppHandle, new_port: u16) -> Result<u16, AppError> {
    let api_state = app
        .try_state::<HttpApiState>()
        .ok_or_else(|| AppError::Engine("HttpApiState not managed".into()))?;

    let mut guard = api_state.0.lock().await;

    // Stop existing server (if any)
    if let Some(handle) = guard.take() {
        log::info!(
            "http_api: stopping server on port {} for rebind to {new_port}",
            handle.port()
        );
        handle.stop().await;
    }

    let allow_remote_access = read_extension_api_allow_remote_access(app).await;

    // The extension knows only the configured port, so a taken port is
    // reported rather than replaced.
    let handle = spawn_http_api(app.clone(), new_port, allow_remote_access).await?;
    let port = handle.port();
    *guard = Some(handle);
    Ok(port)
}

/// Tells the UI that the extension API could not listen on its port.
pub fn emit_bind_failed(app: &AppHandle, port: u16) {
    let _ = app.emit(
        "extension-api:bind-failed",
        serde_json::json!({ "port": port }),
    );
}

// ── Read extension API port from RuntimeConfig ─────────────────────

/// Read the extension API port from RuntimeConfigState.
/// Falls back to store read, then to the default extension API port if neither is available.
pub async fn read_extension_api_port(app: &AppHandle) -> u16 {
    // Primary: RuntimeConfigState (cached, always in sync)
    if let Some(rc_state) = app.try_state::<RuntimeConfigState>() {
        return rc_state.0.read().await.extension_api_port;
    }
    // Fallback: direct store read (during early startup before state is managed)
    read_extension_api_port_from_store(app)
}

pub async fn read_extension_api_allow_remote_access(app: &AppHandle) -> bool {
    if let Some(rc_state) = app.try_state::<RuntimeConfigState>() {
        return rc_state.0.read().await.allow_remote_access;
    }
    read_extension_api_allow_remote_access_from_store(app)
}

/// Direct store read — used only as a fallback during early startup.
fn read_extension_api_port_from_store(app: &AppHandle) -> u16 {
    app.store("config.json")
        .ok()
        .and_then(|s| s.get("preferences"))
        .and_then(|p| {
            p.get("extensionApiPort").and_then(|v| {
                v.as_u64()
                    .map(|n| n as u16)
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
        })
        .unwrap_or(DEFAULT_EXTENSION_API_PORT)
}

fn read_extension_api_allow_remote_access_from_store(app: &AppHandle) -> bool {
    app.store("config.json")
        .ok()
        .and_then(|s| s.get("preferences"))
        .and_then(|p| {
            p.get("allowRemoteAccess")
                .and_then(serde_json::Value::as_bool)
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn write_budget_allows_a_burst_then_refills() {
        let limits = super::Limits::new(4.0);
        assert!((0..4).all(|_| limits.spend(4.0)));
        assert!(!limits.spend(4.0));
        limits.budget.lock().unwrap().1 -= std::time::Duration::from_millis(100);
        assert!(limits.spend(4.0));
    }

    use super::*;
    use axum::http::HeaderValue;

    #[tokio::test]
    async fn product_boundary_rejects_unrelated_browser_origins_and_missing_identity() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("local listener");
        let address = listener.local_addr().expect("local address");
        let router = Router::new()
            .route("/add", post(|| async { StatusCode::OK }))
            .layer(middleware::from_fn(require_product_client));
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        let client = reqwest::Client::new();
        for (origin, identity, expected) in [
            (Some("https://example.test"), true, 403),
            (Some("chrome-extension://test-extension"), false, 400),
            (Some("chrome-extension://test-extension"), true, 200),
            (Some("moz-extension://test-extension"), true, 200),
            (None, false, 200),
        ] {
            let mut request = client.post(format!("http://{address}/add"));
            if let Some(origin) = origin {
                request = request.header("origin", origin);
            }
            if identity {
                request = request.header("x-rayburst-client", "rayburst-connect");
            }
            assert_eq!(
                request
                    .send()
                    .await
                    .expect("local response")
                    .status()
                    .as_u16(),
                expected
            );
        }
        server.abort();
    }

    // ── validate_bearer_token ───────────────────────────────────────

    #[test]
    fn auth_accepts_correct_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer my-secret"),
        );
        assert!(validate_bearer_token(&headers, "my-secret").is_ok());
    }

    #[test]
    fn auth_rejects_wrong_bearer_token() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "authorization",
            HeaderValue::from_static("Bearer wrong-secret"),
        );
        assert_eq!(
            validate_bearer_token(&headers, "my-secret"),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn auth_rejects_missing_header() {
        let headers = HeaderMap::new();
        assert_eq!(
            validate_bearer_token(&headers, "my-secret"),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn auth_rejects_non_bearer_scheme() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Basic my-secret"));
        assert_eq!(
            validate_bearer_token(&headers, "my-secret"),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn auth_allows_any_request_when_secret_is_empty() {
        let headers = HeaderMap::new();
        assert!(validate_bearer_token(&headers, "").is_ok());
    }

    #[test]
    fn auth_allows_with_header_when_secret_is_empty() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("Bearer anything"));
        assert!(validate_bearer_token(&headers, "").is_ok());
    }
}
