//! Files the application itself downloads: update metadata and packages,
//! remote torrents, tracker lists, the BT peer blocklist and the ED2K
//! bootstrap files.
//!
//! They go through the engine as hidden tasks, so they share its connection
//! handling, retries, proxy and TLS with the user's downloads, and a large
//! file such as an update package gets several connections. Each fetch writes
//! into its own directory under the cache, is read back into memory and leaves
//! nothing behind: neither the file nor the task's result.
//!
//! When the engine is not running, cannot take the request, or would queue it
//! behind the user's downloads, the same request is made directly instead.
use crate::engine::EngineState;
use crate::error::AppError;
use crate::services::tasks::{TaskService, TaskServiceState};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// Directory under the application cache that holds in-flight fetches.
const CACHE_DIR: &str = "engine-fetch";
/// Name of the downloaded file inside a fetch's directory.
const BODY: &str = "body";
/// How often a running fetch is read from the engine.
const POLL: Duration = Duration::from_millis(150);
/// How long a fetch may wait for a free download slot before it is made directly.
const QUEUE_WAIT: Duration = Duration::from_secs(2);

/// One file to download into memory.
#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: String,
    /// Proxy server for this request; `None` connects directly.
    pub proxy: Option<String>,
    /// Extra request headers as `Name: value` lines.
    pub headers: Vec<String>,
    pub user_agent: Option<String>,
    /// Largest accepted body, in bytes.
    pub max_size: u64,
    /// Time the whole fetch may take; `None` for large files that only need
    /// the engine's own stall detection.
    pub deadline: Option<Duration>,
    /// Small files take one connection; large ones the engine's default split.
    pub single_connection: bool,
}

impl FetchRequest {
    /// A small text or metadata file, fetched over one connection within 30 seconds.
    pub fn small(url: impl Into<String>, proxy: Option<String>, max_size: u64) -> Self {
        Self {
            url: url.into(),
            proxy: proxy
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
            headers: Vec::new(),
            user_agent: None,
            max_size,
            deadline: Some(Duration::from_secs(30)),
            single_connection: true,
        }
    }
}

/// Progress of a fetch: bytes received and, once known, the total.
pub type Progress<'a> = &'a mut (dyn FnMut(u64, Option<u64>) + Send);
/// Polled while a fetch runs; returning `true` abandons it.
pub type Cancelled<'a> = &'a (dyn Fn() -> bool + Send + Sync);

fn cancelled_error() -> AppError {
    AppError::Conflict("Download cancelled".into())
}

/// Downloads a small file.
pub async fn fetch(app: &AppHandle, request: &FetchRequest) -> Result<Vec<u8>, AppError> {
    fetch_with(app, request, &mut |_, _| {}, &|| false).await
}

/// Downloads a file, reporting progress and stopping once `cancelled` is true.
pub async fn fetch_with(
    app: &AppHandle,
    request: &FetchRequest,
    progress: Progress<'_>,
    cancelled: Cancelled<'_>,
) -> Result<Vec<u8>, AppError> {
    let url = url::Url::parse(request.url.trim())
        .map_err(|error| AppError::InvalidInput(format!("Invalid URL: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::InvalidInput(
            "Only HTTP and HTTPS URLs can be fetched".into(),
        ));
    }
    if let Some(engine) = running_engine(app) {
        let root = cache_root(app)?;
        match via_engine(&engine, &root, request, progress, cancelled).await {
            Ok(Fetched::Body(bytes)) => return Ok(bytes),
            Ok(Fetched::Unavailable(reason)) => {
                log::info!("engine_fetch: direct request reason={reason}");
            }
            Err(error) => return Err(error),
        }
    }
    direct(request, progress, cancelled).await
}

fn running_engine(app: &AppHandle) -> Option<std::sync::Arc<TaskService>> {
    let running = app
        .try_state::<EngineState>()
        .is_some_and(|state| state.is_running());
    if !running {
        return None;
    }
    app.try_state::<TaskServiceState>()
        .map(|state| state.0.clone())
}

fn cache_root(app: &AppHandle) -> Result<PathBuf, AppError> {
    app.path()
        .app_cache_dir()
        .map(|dir| dir.join(CACHE_DIR))
        .map_err(|error| AppError::Io(format!("Failed to resolve cache directory: {error}")))
}

fn new_gid() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..16].to_string()
}

enum Fetched {
    Body(Vec<u8>),
    /// The engine could not serve the request; the reason is logged.
    Unavailable(String),
}

/// Engine options for a hidden fetch task writing `dir/body`.
fn task_options(gid: &str, dir: &Path, request: &FetchRequest) -> serde_json::Value {
    let mut options = serde_json::json!({
        "gid": gid,
        "dir": crate::engine::path_to_safe_string(dir),
        "out": BODY,
        "allow-overwrite": "true",
        "auto-file-renaming": "false",
        "continue": "false",
        "follow-torrent": "false",
        "follow-metalink": "false",
        "file-allocation": "none",
        "force-save": "false",
        "remote-time": "false",
        "max-tries": "3",
        "retry-wait": "2",
        "connect-timeout": "15",
        "timeout": "30",
        "all-proxy": request.proxy.clone().unwrap_or_default(),
    });
    if request.single_connection {
        options["split"] = "1".into();
        options["max-connection-per-server"] = "1".into();
    }
    if !request.headers.is_empty() {
        options["header"] = serde_json::json!(request.headers);
    }
    if let Some(user_agent) = request
        .user_agent
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        options["user-agent"] = user_agent.into();
    }
    options
}

async fn via_engine(
    engine: &TaskService,
    root: &Path,
    request: &FetchRequest,
    progress: Progress<'_>,
    cancelled: Cancelled<'_>,
) -> Result<Fetched, AppError> {
    let gid = new_gid();
    let dir = root.join(&gid);
    // Hidden before the engine knows it, so it never appears as a user task,
    // and before its directory exists, so a purge keeps it.
    engine.tasks.set_internal(&gid, true).await;
    if let Err(error) = std::fs::create_dir_all(&dir) {
        engine.tasks.set_internal(&gid, false).await;
        return Err(error.into());
    }
    let result = run_task(engine, &gid, &dir, request, progress, cancelled).await;
    let finished = matches!(&result, Ok(Fetched::Body(_)));
    if !finished {
        let _ = engine.force_remove(&gid).await;
    }
    if forget(engine, &gid).await {
        engine.tasks.set_internal(&gid, false).await;
    } else {
        // Still hidden, so it never shows up among the user's tasks.
        log::warn!("engine_fetch: result of gid={gid} could not be removed");
    }
    if let Err(error) = std::fs::remove_dir_all(&dir) {
        log::debug!("engine_fetch: cleanup gid={gid} failed: {error}");
    }
    result
}

/// Removes the task's result. A removed task reaches the results only once
/// the engine has closed its connections, so this waits for it briefly.
async fn forget(engine: &TaskService, gid: &str) -> bool {
    for _ in 0..40 {
        if engine.remove_download_result(gid).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

async fn run_task(
    engine: &TaskService,
    gid: &str,
    dir: &Path,
    request: &FetchRequest,
    progress: Progress<'_>,
    cancelled: Cancelled<'_>,
) -> Result<Fetched, AppError> {
    if let Err(error) = engine
        .add_uri(
            vec![request.url.trim().to_owned()],
            task_options(gid, dir, request),
        )
        .await
    {
        return Ok(Fetched::Unavailable(format!("add failed: {error}")));
    }
    let started = Instant::now();
    let mut reported = None;
    loop {
        tokio::time::sleep(POLL).await;
        if cancelled() {
            return Err(cancelled_error());
        }
        if request
            .deadline
            .is_some_and(|deadline| started.elapsed() > deadline)
        {
            return Err(AppError::Io("Download timed out".into()));
        }
        let task = match engine.tell_status(gid).await {
            Ok(task) => task,
            Err(error) => return Ok(Fetched::Unavailable(format!("status failed: {error}"))),
        };
        let completed = task.completed_length.parse::<u64>().unwrap_or(0);
        let total = task
            .total_length
            .parse::<u64>()
            .ok()
            .filter(|total| *total > 0);
        if let Some(total) = total.filter(|total| *total > request.max_size) {
            return Err(too_large(total, request.max_size));
        }
        if reported != Some((completed, total)) {
            reported = Some((completed, total));
            progress(completed, total);
        }
        match task.status.as_str() {
            "complete" => {
                let bytes = std::fs::read(dir.join(BODY))?;
                if bytes.len() as u64 > request.max_size {
                    return Err(too_large(bytes.len() as u64, request.max_size));
                }
                return Ok(Fetched::Body(bytes));
            }
            "error" | "removed" => {
                let message = task
                    .error_message
                    .filter(|message| !message.trim().is_empty())
                    .unwrap_or_else(|| {
                        format!("engine error {}", task.error_code.unwrap_or_default())
                    });
                return Err(AppError::Io(message));
            }
            // Every download slot is taken by the user's own downloads.
            "waiting" | "paused" if completed == 0 && started.elapsed() > QUEUE_WAIT => {
                return Ok(Fetched::Unavailable("engine queue is full".into()));
            }
            _ => {}
        }
    }
}

fn too_large(size: u64, max: u64) -> AppError {
    AppError::Io(format!("Response too large: {size} bytes (max {max})"))
}

/// Makes a request directly, without the engine.
#[cfg(test)]
pub(crate) async fn fetch_direct(request: &FetchRequest) -> Result<Vec<u8>, AppError> {
    direct(request, &mut |_, _| {}, &|| false).await
}

/// The same request without the engine.
async fn direct(
    request: &FetchRequest,
    progress: Progress<'_>,
    cancelled: Cancelled<'_>,
) -> Result<Vec<u8>, AppError> {
    let mut builder = reqwest::Client::builder().redirect(reqwest::redirect::Policy::limited(5));
    if let Some(deadline) = request.deadline {
        builder = builder.timeout(deadline);
    } else {
        builder = builder
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(60));
    }
    if let Some(user_agent) = request
        .user_agent
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        builder = builder.user_agent(user_agent);
    }
    builder = match request.proxy.as_deref() {
        Some(server) => builder.proxy(
            reqwest::Proxy::all(server)
                .map_err(|error| AppError::InvalidInput(format!("Invalid proxy: {error}")))?,
        ),
        None => builder.no_proxy(),
    };
    let client = builder
        .build()
        .map_err(|error| AppError::Io(format!("Failed to build HTTP client: {error}")))?;
    let mut call = client.get(request.url.trim());
    for line in &request.headers {
        if let Some((name, value)) = line.split_once(':') {
            call = call.header(name.trim(), value.trim());
        }
    }
    let response = call
        .send()
        .await
        .map_err(|error| AppError::Io(format!("Request failed: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::Io(format!(
            "The server answered HTTP {}",
            response.status().as_u16()
        )));
    }
    let total = response.content_length().filter(|total| *total > 0);
    if let Some(total) = total.filter(|total| *total > request.max_size) {
        return Err(too_large(total, request.max_size));
    }
    let mut body = Vec::with_capacity(total.unwrap_or(0).min(request.max_size) as usize);
    let mut response = response;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| AppError::Io(format!("Failed to read response: {error}")))?
    {
        if cancelled() {
            return Err(cancelled_error());
        }
        if (body.len() + chunk.len()) as u64 > request.max_size {
            return Err(too_large(
                (body.len() + chunk.len()) as u64,
                request.max_size,
            ));
        }
        body.extend_from_slice(&chunk);
        progress(body.len() as u64, total);
    }
    Ok(body)
}

/// Removes fetches left behind by an earlier run: the engine restores a task
/// that was still running when the application quit, and its directory stays.
pub async fn purge_stale(app: &AppHandle) {
    let (Some(engine), Ok(root)) = (running_engine(app), cache_root(app)) else {
        return;
    };
    let prefix = crate::engine::path_to_safe_string(&root);
    match engine.tell_task_snapshot(true).await {
        Ok(tasks) => {
            for task in tasks.iter().filter(|task| is_under(&task.dir, &prefix)) {
                let _ = engine.force_remove(&task.gid).await;
                let _ = engine.remove_download_result(&task.gid).await;
                log::info!("engine_fetch: removed stale task gid={}", task.gid);
            }
        }
        Err(error) => log::debug!("engine_fetch: stale task scan failed: {error}"),
    }
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    let live = engine.tasks.internal_ids().await;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if live.contains(&name) {
            continue;
        }
        if let Err(error) = std::fs::remove_dir_all(entry.path()) {
            log::debug!("engine_fetch: stale directory {name} kept: {error}");
        }
    }
}

fn is_under(dir: &str, root: &str) -> bool {
    let normalize = |value: &str| {
        value
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_lowercase()
    };
    let (dir, root) = (normalize(dir), normalize(root));
    dir == root || dir.starts_with(&format!("{root}/"))
}

#[cfg(test)]
mod e2e;

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> FetchRequest {
        FetchRequest::small("https://example.com/list.txt", None, 1024)
    }

    #[test]
    fn small_requests_take_one_direct_connection() {
        let options = task_options(
            "0123456789abcdef",
            Path::new("/cache/engine-fetch/x"),
            &request(),
        );
        assert_eq!(options["gid"], "0123456789abcdef");
        assert_eq!(options["out"], BODY);
        assert_eq!(options["split"], "1");
        assert_eq!(options["max-connection-per-server"], "1");
        assert_eq!(options["follow-torrent"], "false");
        // An empty proxy overrides a global download proxy.
        assert_eq!(options["all-proxy"], "");
        assert!(options.get("header").is_none());
    }

    #[test]
    fn large_requests_keep_the_engine_split_and_carry_headers() {
        let mut request = request();
        request.single_connection = false;
        request.proxy = Some("http://127.0.0.1:7890".into());
        request.headers = vec!["Cookie: a=b".into()];
        request.user_agent = Some("Agent/1".into());
        let options = task_options("0123456789abcdef", Path::new("/tmp"), &request);
        assert!(options.get("split").is_none());
        assert_eq!(options["all-proxy"], "http://127.0.0.1:7890");
        assert_eq!(options["header"][0], "Cookie: a=b");
        assert_eq!(options["user-agent"], "Agent/1");
    }

    #[test]
    fn blank_proxy_means_direct() {
        let request = FetchRequest::small("https://example.com", Some("  ".into()), 1);
        assert!(request.proxy.is_none());
    }

    #[test]
    fn stale_tasks_are_matched_by_their_directory() {
        let root = "C:\\Users\\me\\AppData\\Local\\app\\engine-fetch";
        assert!(is_under(
            "C:/Users/me/AppData/Local/app/engine-fetch/0123",
            root
        ));
        assert!(!is_under("C:/Users/me/Downloads", root));
        assert!(!is_under(
            "C:/Users/me/AppData/Local/app/engine-fetch-other",
            root
        ));
    }

    #[test]
    fn engine_gids_are_sixteen_hex_digits() {
        let gid = new_gid();
        assert_eq!(gid.len(), 16);
        assert!(gid.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }
}
