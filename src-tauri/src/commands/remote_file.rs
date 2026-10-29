//! Look ahead at remote downloads and fetch small torrent files for native
//! metainfo inspection.
use crate::error::AppError;
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;

/// At most this many downloads are looked at together.
const MAX_PREVIEWS: usize = 4;

#[derive(Default)]
pub struct DownloadPreviewState(Mutex<HashMap<String, tokio::sync::oneshot::Sender<()>>>);

#[tauri::command]
pub fn cancel_download_preview(
    state: tauri::State<'_, DownloadPreviewState>,
    id: String,
) -> Result<(), AppError> {
    if let Some(cancel) = state
        .0
        .lock()
        .map_err(|error| AppError::Io(error.to_string()))?
        .remove(&id)
    {
        let _ = cancel.send(());
    }
    Ok(())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadPreviewRequest {
    id: String,
    url: String,
    /// The engine options the download would be created with.
    options: serde_json::Value,
}

/// The size and name a download would get, read from its first byte; the
/// engine makes the request, so it goes out as the download's would.
#[tauri::command]
pub async fn preview_download(
    app: tauri::AppHandle,
    request: DownloadPreviewRequest,
) -> Result<crate::services::downloads::Preview, AppError> {
    let DownloadPreviewRequest { id, url, options } = request;
    let parsed =
        url::Url::parse(&url).map_err(|error| AppError::InvalidInput(error.to_string()))?;
    if !matches!(parsed.scheme(), "http" | "https" | "thunder") || !options.is_object() {
        return Err(AppError::InvalidInput(
            "A download preview takes an HTTP or Thunder URL and its options".into(),
        ));
    }
    let state = app.state::<DownloadPreviewState>();
    let (cancel, cancelled) = tokio::sync::oneshot::channel();
    {
        let mut previews = state
            .0
            .lock()
            .map_err(|error| AppError::Io(error.to_string()))?;
        if previews.len() >= MAX_PREVIEWS || previews.contains_key(&id) {
            return Err(AppError::Conflict(
                "Too many download previews are running".into(),
            ));
        }
        previews.insert(id.clone(), cancel);
    }
    let result = tokio::select! {
        value = crate::services::downloads::preview(&app, &url, options) => value,
        _ = cancelled => Err(AppError::Conflict("Download preview cancelled".into())),
    };
    state
        .0
        .lock()
        .map_err(|error| AppError::Io(error.to_string()))?
        .remove(&id);
    result
}

const MAX_TORRENT_SIZE: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRequestHeader {
    pub name: String,
    pub value: String,
}

/// Header lines for the request, the browser's own first. A header the
/// browser sent as well as named here (Referer, Cookie, User-Agent) is sent
/// once, with the named value. Lines that would not form one header are dropped.
fn header_lines(
    referer: Option<&str>,
    cookie: Option<&str>,
    user_agent: Option<&str>,
    request_headers: &[RemoteRequestHeader],
) -> Vec<String> {
    let named = |value: Option<&str>| value.filter(|v| !v.trim().is_empty()).map(str::to_owned);
    let (referer, cookie, user_agent) = (named(referer), named(cookie), named(user_agent));
    let overridden = |name: &str| {
        (name.eq_ignore_ascii_case("referer") && referer.is_some())
            || (name.eq_ignore_ascii_case("cookie") && cookie.is_some())
            || (name.eq_ignore_ascii_case("user-agent") && user_agent.is_some())
    };
    let valid = |name: &str, value: &str| {
        reqwest::header::HeaderName::from_bytes(name.as_bytes()).is_ok()
            && reqwest::header::HeaderValue::from_str(value).is_ok()
    };
    let mut lines: Vec<String> = request_headers
        .iter()
        .filter(|header| !overridden(&header.name) && valid(&header.name, &header.value))
        .map(|header| format!("{}: {}", header.name, header.value))
        .collect();
    for (name, value) in [("Referer", referer), ("Cookie", cookie)] {
        if let Some(value) = value.filter(|value| valid(name, value)) {
            lines.push(format!("{name}: {value}"));
        }
    }
    lines
}

/// The request for a remote torrent, made the way its download would be.
fn remote_request(
    url: String,
    proxy: Option<String>,
    referer: Option<&str>,
    cookie: Option<&str>,
    user_agent: Option<&str>,
    request_headers: &[RemoteRequestHeader],
) -> crate::services::engine_fetch::FetchRequest {
    let mut request =
        crate::services::engine_fetch::FetchRequest::small(url, proxy, MAX_TORRENT_SIZE);
    request.headers = header_lines(referer, cookie, user_agent, request_headers);
    request.user_agent = user_agent
        .filter(|value| reqwest::header::HeaderValue::from_str(value).is_ok())
        .map(str::to_owned);
    request
}

#[tauri::command]
pub async fn fetch_remote_bytes(
    app: tauri::AppHandle,
    url: String,
    proxy: Option<String>,
    referer: Option<String>,
    cookie: Option<String>,
    user_agent: Option<String>,
    request_headers: Vec<RemoteRequestHeader>,
) -> Result<Vec<u8>, AppError> {
    log::info!("fetch_remote_bytes: url={}", summarize_url_for_log(&url));
    let request = remote_request(
        url,
        proxy,
        referer.as_deref(),
        cookie.as_deref(),
        user_agent.as_deref(),
        &request_headers,
    );
    let bytes = crate::services::engine_fetch::fetch(&app, &request).await?;
    log::info!("fetch_remote_bytes: downloaded {} bytes", bytes.len());
    Ok(bytes)
}

pub(crate) fn summarize_url_for_log(value: &str) -> String {
    let lower = value.to_lowercase();
    if lower.starts_with("magnet:") {
        return format!("scheme=magnet length={}", value.len());
    }
    if lower.starts_with("ed2k://") {
        return format!("scheme=ed2k length={}", value.len());
    }
    if lower.starts_with("thunder://") {
        return format!("scheme=thunder length={}", value.len());
    }

    match url::Url::parse(value) {
        Ok(parsed) => {
            let scheme = parsed.scheme();
            let host = parsed.host_str().unwrap_or("none");
            let ext = parsed
                .path_segments()
                .and_then(|mut segments| segments.next_back())
                .and_then(|name| name.rsplit_once('.').map(|(_, ext)| ext))
                .filter(|ext| !ext.is_empty() && ext.len() <= 16)
                .unwrap_or("none");
            format!(
                "scheme={scheme} host={host} ext={} has_query={} length={}",
                ext.to_ascii_lowercase(),
                parsed.query().is_some(),
                value.len()
            )
        }
        Err(_) => format!("parseable=false length={}", value.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn remote_torrent_requests_send_browser_context_headers() {
        use axum::{extract::State, http::HeaderMap, routing::get, Router};
        use std::net::SocketAddr;
        use std::sync::{Arc, Mutex};
        use tokio::net::TcpListener;

        #[derive(Default)]
        struct CapturedHeaders {
            referer: Option<String>,
            cookie: Option<String>,
            user_agent: Option<String>,
            accept_language: Option<String>,
        }

        async fn handle_get(
            State(captured): State<Arc<Mutex<CapturedHeaders>>>,
            headers: HeaderMap,
        ) -> &'static [u8] {
            let mut captured = captured.lock().expect("captured headers mutex poisoned");
            captured.referer = headers
                .get(reqwest::header::REFERER)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            captured.cookie = headers
                .get(reqwest::header::COOKIE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            captured.user_agent = headers
                .get(reqwest::header::USER_AGENT)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            captured.accept_language = headers
                .get(reqwest::header::ACCEPT_LANGUAGE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);

            b"d4:infode"
        }

        let captured = Arc::new(Mutex::new(CapturedHeaders::default()));
        let app = Router::new()
            .route("/linux.torrent", get(handle_get))
            .with_state(Arc::clone(&captured));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr: SocketAddr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let request = remote_request(
            format!("http://{addr}/linux.torrent"),
            None,
            Some("https://example.com/page"),
            Some("sid=1"),
            Some("Mozilla/5.0"),
            &[
                RemoteRequestHeader {
                    name: "Accept-Language".to_string(),
                    value: "en-US".to_string(),
                },
                RemoteRequestHeader {
                    name: "Cookie".to_string(),
                    value: "stale=1".to_string(),
                },
            ],
        );
        let bytes = crate::services::engine_fetch::fetch_direct(&request)
            .await
            .unwrap();

        assert_eq!(bytes, b"d4:infode");
        let captured = captured.lock().expect("captured headers mutex poisoned");
        assert_eq!(
            captured.referer.as_deref(),
            Some("https://example.com/page")
        );
        assert_eq!(captured.cookie.as_deref(), Some("sid=1"));
        assert_eq!(captured.user_agent.as_deref(), Some("Mozilla/5.0"));
        assert_eq!(captured.accept_language.as_deref(), Some("en-US"));
    }
}
