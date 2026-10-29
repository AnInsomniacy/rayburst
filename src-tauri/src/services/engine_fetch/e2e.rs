//! Fetches through the bundled engine binary against a local HTTP server.
//!
//! Ignored by default: they start the real engine. Run with
//! `cargo test engine_fetch::e2e -- --ignored --test-threads=1`.
use super::*;
use axum::{extract::State, http::HeaderMap, routing::get, Router};
use std::net::SocketAddr;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

const SECRET: &str = "engine-fetch-e2e";
const LARGE: usize = 24 * 1024 * 1024;

struct Engine {
    child: Child,
    service: TaskService,
    root: PathBuf,
    _work: tempfile::TempDir,
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn binary() -> PathBuf {
    let name = if cfg!(windows) {
        "aria2-next-x86_64-pc-windows-msvc.exe"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "aria2-next-aarch64-apple-darwin"
        } else {
            "aria2-next-x86_64-apple-darwin"
        }
    } else if cfg!(target_arch = "aarch64") {
        "aria2-next-aarch64-unknown-linux-gnu"
    } else {
        "aria2-next-x86_64-unknown-linux-gnu"
    };
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(name)
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .expect("free port")
}

async fn start_engine(max_concurrent: u32) -> Engine {
    let work = tempfile::tempdir().expect("work dir");
    let port = free_port();
    let state = work.path().join("state");
    let downloads = work.path().join("downloads");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::create_dir_all(&downloads).unwrap();
    let child = Command::new(binary())
        .args([
            "--no-conf=true".to_string(),
            "--enable-rpc=true".to_string(),
            format!("--rpc-listen-port={port}"),
            format!("--rpc-secret={SECRET}"),
            format!("--dir={}", downloads.display()),
            format!("--state-dir={}", state.display()),
            format!("--max-concurrent-downloads={max_concurrent}"),
            "--console-log-level=warn".to_string(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("start the bundled engine");
    let engine = Engine {
        child,
        service: TaskService::new(port, SECRET.into()),
        root: work.path().join(CACHE_DIR),
        _work: work,
    };
    for _ in 0..100 {
        if engine.service.get_version().await.is_ok() {
            return engine;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("the engine did not answer");
}

#[derive(Default)]
struct Seen {
    cookie: Option<String>,
    user_agent: Option<String>,
    language: Option<String>,
}

async fn remember_headers(
    State(seen): State<Arc<Mutex<Seen>>>,
    headers: HeaderMap,
) -> &'static [u8] {
    let value = |name: &str| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
    };
    *seen.lock().unwrap() = Seen {
        cookie: value("cookie"),
        user_agent: value("user-agent"),
        language: value("accept-language"),
    };
    b"d4:infode"
}

/// A body that trickles in over two minutes.
async fn slow() -> axum::body::Body {
    let stream = futures_util::stream::unfold(0u32, |sent| async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        (sent < 600).then(|| (Ok::<_, std::io::Error>(vec![0u8; 1024]), sent + 1))
    });
    axum::body::Body::from_stream(stream)
}

async fn serve() -> (SocketAddr, Arc<Mutex<Seen>>, tempfile::TempDir) {
    let files = tempfile::tempdir().expect("served dir");
    let large: Vec<u8> = (0..LARGE).map(|index| (index % 251) as u8).collect();
    std::fs::write(files.path().join("large.bin"), large).unwrap();
    std::fs::write(
        files.path().join("small.txt"),
        b"udp://tracker.example:80/announce\n",
    )
    .unwrap();
    let seen = Arc::new(Mutex::new(Seen::default()));
    let app = Router::new()
        .route("/headers.torrent", get(remember_headers))
        .route("/slow", get(slow))
        .with_state(Arc::clone(&seen))
        .fallback_service(tower_http::services::ServeDir::new(files.path()));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (address, seen, files)
}

async fn run(
    engine: &Engine,
    request: &FetchRequest,
    progress: Progress<'_>,
    cancelled: Cancelled<'_>,
) -> Result<Fetched, AppError> {
    via_engine(&engine.service, &engine.root, request, progress, cancelled).await
}

async fn body(engine: &Engine, request: &FetchRequest) -> Vec<u8> {
    match run(engine, request, &mut |_, _| {}, &|| false).await {
        Ok(Fetched::Body(bytes)) => bytes,
        Ok(Fetched::Unavailable(reason)) => panic!("the engine did not take it: {reason}"),
        Err(error) => panic!("fetch failed: {error}"),
    }
}

async fn assert_clean(engine: &Engine) {
    assert!(engine.service.tasks.internal_ids().await.is_empty());
    assert!(engine
        .service
        .tell_task_snapshot(true)
        .await
        .unwrap()
        .is_empty());
    let left = std::fs::read_dir(&engine.root)
        .map(Iterator::count)
        .unwrap_or(0);
    assert_eq!(left, 0, "fetch directories left behind");
}

#[tokio::test]
#[ignore = "starts the bundled engine"]
async fn small_files_come_back_whole_and_leave_nothing() {
    let engine = start_engine(5).await;
    let (address, _, _files) = serve().await;
    let request = FetchRequest::small(format!("http://{address}/small.txt"), None, 1024);
    assert_eq!(
        body(&engine, &request).await,
        b"udp://tracker.example:80/announce\n"
    );
    assert_clean(&engine).await;
}

#[tokio::test]
#[ignore = "starts the bundled engine"]
async fn requests_carry_the_browser_context() {
    let engine = start_engine(5).await;
    let (address, seen, _files) = serve().await;
    let mut request = FetchRequest::small(format!("http://{address}/headers.torrent"), None, 1024);
    request.headers = vec!["Cookie: sid=1".into(), "Accept-Language: en-US".into()];
    request.user_agent = Some("Mozilla/5.0 e2e".into());
    assert_eq!(body(&engine, &request).await, b"d4:infode");
    let seen = seen.lock().unwrap();
    assert_eq!(seen.cookie.as_deref(), Some("sid=1"));
    assert_eq!(seen.user_agent.as_deref(), Some("Mozilla/5.0 e2e"));
    assert_eq!(seen.language.as_deref(), Some("en-US"));
}

#[tokio::test]
#[ignore = "starts the bundled engine"]
async fn large_files_report_progress_and_arrive_intact() {
    let engine = start_engine(5).await;
    let (address, _, files) = serve().await;
    let mut request =
        FetchRequest::small(format!("http://{address}/large.bin"), None, LARGE as u64);
    request.single_connection = false;
    request.deadline = None;
    let mut reports = Vec::new();
    let fetched = run(
        &engine,
        &request,
        &mut |done, total| reports.push((done, total)),
        &|| false,
    )
    .await;
    let Ok(Fetched::Body(bytes)) = fetched else {
        panic!("the large file did not arrive");
    };
    assert_eq!(
        bytes,
        std::fs::read(files.path().join("large.bin")).unwrap()
    );
    assert_eq!(reports.last(), Some(&(LARGE as u64, Some(LARGE as u64))));
    assert!(reports.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert_clean(&engine).await;
}

#[tokio::test]
#[ignore = "starts the bundled engine"]
async fn failures_and_limits_are_errors_that_leave_nothing() {
    let engine = start_engine(5).await;
    let (address, _, _files) = serve().await;

    let missing = FetchRequest::small(format!("http://{address}/missing.txt"), None, 1024);
    assert!(run(&engine, &missing, &mut |_, _| {}, &|| false)
        .await
        .is_err());

    let too_large = FetchRequest::small(format!("http://{address}/large.bin"), None, 1024);
    let error = run(&engine, &too_large, &mut |_, _| {}, &|| false)
        .await
        .err()
        .expect("a file over the limit fails");
    assert!(error.to_string().contains("too large"), "{error}");

    let slow = FetchRequest::small(format!("http://{address}/slow"), None, 1 << 20);
    let started = Instant::now();
    let cancel = move || started.elapsed() > Duration::from_millis(600);
    let error = run(&engine, &slow, &mut |_, _| {}, &cancel)
        .await
        .err()
        .expect("a cancelled fetch fails");
    assert!(matches!(error, AppError::Conflict(_)), "{error}");

    assert_clean(&engine).await;
}

#[tokio::test]
#[ignore = "starts the bundled engine"]
async fn a_full_queue_hands_the_request_back() {
    let engine = start_engine(1).await;
    let (address, _, _files) = serve().await;
    let busy = engine
        .service
        .add_uri(
            vec![format!("http://{address}/slow")],
            serde_json::json!({}),
        )
        .await
        .unwrap();
    let request = FetchRequest::small(format!("http://{address}/small.txt"), None, 1024);
    let outcome = run(&engine, &request, &mut |_, _| {}, &|| false)
        .await
        .unwrap();
    assert!(matches!(outcome, Fetched::Unavailable(_)));
    assert!(engine.service.tasks.internal_ids().await.is_empty());
    let _ = engine.service.force_remove(&busy).await;
}
