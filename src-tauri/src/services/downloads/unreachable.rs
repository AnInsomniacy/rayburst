//! Tells the user when the browser keeps a download the engine cannot reach.
//!
//! The browser may reach a site through a proxy the engine does not use; the
//! download then stays in the browser, which the user would not notice (#784).
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// One notice per host in this time.
const QUIET: Duration = Duration::from_secs(30 * 60);

static TOLD: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);

/// Whether a notice for `host` is due at `now`, recording it when it is.
fn due(told: &mut HashMap<String, Instant>, host: &str, now: Instant) -> bool {
    told.retain(|_, at| now.duration_since(*at) < QUIET);
    if told.contains_key(host) {
        return false;
    }
    told.insert(host.to_owned(), now);
    true
}

/// `proxied` says whether Rayburst has a proxy turned on; without one the
/// notice suggests it.
pub(super) fn tell(app: &AppHandle, url: &str, proxied: bool) {
    let Some(host) = url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
    else {
        return;
    };
    {
        let mut told = TOLD.lock().unwrap_or_else(PoisonError::into_inner);
        if !due(told.get_or_insert_with(HashMap::new), &host, Instant::now()) {
            return;
        }
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let config = app
            .state::<crate::services::config::RuntimeConfigState>()
            .snapshot()
            .await;
        if !config.task_notification {
            return;
        }
        let locale = crate::i18n::resolve_preferred_locale(&config.locale);
        let message = crate::i18n::browser_kept(&locale, &host, proxied);
        if let Err(error) = crate::services::notification::send_app_notification(
            &app,
            &message.title,
            &message.body,
        )
        .await
        {
            log::warn!("downloads: unreachable notice failed error={error}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tells_each_host_once_a_while() {
        let mut told = HashMap::new();
        let start = Instant::now();
        assert!(due(&mut told, "a.example", start));
        assert!(!due(
            &mut told,
            "a.example",
            start + Duration::from_secs(60)
        ));
        assert!(due(&mut told, "b.example", start + Duration::from_secs(60)));
        assert!(due(&mut told, "a.example", start + QUIET));
    }
}
