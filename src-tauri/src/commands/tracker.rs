use crate::error::AppError;
use serde::{Deserialize, Serialize};

/// A single tracker source URL that failed to fetch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedTrackerSource {
    pub url: String,
    pub reason: String,
}

/// Structured result from fetching multiple tracker source URLs.
/// Captures both successful response bodies and per-URL failure details
/// so the frontend can show granular feedback to the user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchTrackerSourcesResult {
    pub data: Vec<String>,
    pub failures: Vec<FailedTrackerSource>,
}

/// Largest accepted tracker list.
const MAX_TRACKER_LIST_SIZE: u64 = 4 * 1024 * 1024;

/// Fetches tracker lists from external source URLs through the engine.
///
/// Each URL is fetched sequentially (typical source count is 1–5).
/// Failed URLs are logged individually and captured in `failures`;
/// successful response bodies are returned in `data`.
#[tauri::command]
pub async fn fetch_tracker_sources(
    app: tauri::AppHandle,
    urls: Vec<String>,
    proxy_server: Option<String>,
) -> Result<FetchTrackerSourcesResult, AppError> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    collect_sources(urls, |url| {
        let request = crate::services::engine_fetch::FetchRequest::small(
            format!("{url}?t={now}"),
            proxy_server.clone(),
            MAX_TRACKER_LIST_SIZE,
        );
        let app = app.clone();
        async move { crate::services::engine_fetch::fetch(&app, &request).await }
    })
    .await
}

async fn collect_sources<F, Fut>(
    urls: Vec<String>,
    fetch: F,
) -> Result<FetchTrackerSourcesResult, AppError>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = Result<Vec<u8>, AppError>>,
{
    if urls.is_empty() {
        return Ok(FetchTrackerSourcesResult {
            data: vec![],
            failures: vec![],
        });
    }

    log::info!("TrackerSync: fetching from {} source(s)", urls.len());

    let mut data = Vec::new();
    let mut failures = Vec::new();

    for url in &urls {
        match fetch(url.clone()).await {
            Ok(body) => data.push(String::from_utf8_lossy(&body).into_owned()),
            Err(e) => {
                let reason = e.to_string();
                log::warn!("TrackerSync: failed to fetch {}: {}", url, reason);
                failures.push(FailedTrackerSource {
                    url: url.clone(),
                    reason,
                });
            }
        }
    }

    log::info!(
        "TrackerSync: completed {}/{} succeeded",
        data.len(),
        urls.len()
    );

    Ok(FetchTrackerSourcesResult { data, failures })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── collect_sources ─────────────────────────────────────────────

    #[test]
    fn test_fetch_empty_urls_returns_empty_result() {
        let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");
        let result = rt
            .block_on(collect_sources(vec![], |_| async { Ok(Vec::new()) }))
            .expect("should succeed");
        assert!(result.data.is_empty());
        assert!(result.failures.is_empty());
    }

    #[test]
    fn test_fetch_failures_are_captured_per_source() {
        let urls = vec![
            "http://192.0.2.1:1/trackers.txt".to_string(),
            "https://example.com/best.txt".to_string(),
        ];
        let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");
        let result = rt
            .block_on(collect_sources(urls, |url| async move {
                if url.starts_with("http://192.0.2.1") {
                    Err(AppError::Io("connection refused".into()))
                } else {
                    Ok(b"udp://tracker.example:80/announce\n".to_vec())
                }
            }))
            .expect("should succeed — individual failures are captured, not errors");
        assert_eq!(result.data, vec!["udp://tracker.example:80/announce\n"]);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].url, "http://192.0.2.1:1/trackers.txt");
        assert!(
            !result.failures[0].reason.is_empty(),
            "failure reason must not be empty"
        );
    }

    #[test]
    fn test_fetch_result_serializes_to_camel_case() {
        let result = FetchTrackerSourcesResult {
            data: vec!["body1".to_string()],
            failures: vec![FailedTrackerSource {
                url: "http://example.com".to_string(),
                reason: "timeout".to_string(),
            }],
        };
        let json = serde_json::to_value(&result).expect("should serialize");
        assert_eq!(json["data"][0], "body1");
        assert_eq!(json["failures"][0]["url"], "http://example.com");
        assert_eq!(json["failures"][0]["reason"], "timeout");
    }

    #[test]
    fn test_fetch_result_empty_serializes_correctly() {
        let result = FetchTrackerSourcesResult {
            data: vec![],
            failures: vec![],
        };
        let json = serde_json::to_value(&result).expect("should serialize");
        assert!(json["data"]
            .as_array()
            .expect("data should be array")
            .is_empty());
        assert!(json["failures"]
            .as_array()
            .expect("failures should be array")
            .is_empty());
    }
}
