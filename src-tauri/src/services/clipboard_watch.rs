//! Offers links copied in other applications while Rayburst stays in the
//! background (#793, #797). The main window otherwise reads the clipboard
//! only when it gains focus. Watching every copy is opt-in, as in other
//! download managers.
//!
//! The add window takes the link: opened for manual entry, it reads the
//! clipboard and applies the same filter as here (`detectResource`).
use super::config::{ClipboardConfig, RuntimeConfigState};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

const TICK: Duration = Duration::from_millis(800);
const MAX_CHARS: usize = 100_000;
const MAX_LINES: usize = 200;
const ALL_TAGS: [&str; 6] = [
    "http://",
    "https://",
    "sftp://",
    "magnet:",
    "ed2k://",
    "thunder://",
];
const WEB_PAGE_EXTENSIONS: [&str; 13] = [
    "html", "htm", "xhtml", "shtml", "php", "asp", "aspx", "jsp", "jspx", "cfm", "cgi", "do",
    "action",
];

/// A number that changes with every copy, where the system keeps one; reading
/// it is far cheaper than reading the text.
#[cfg(windows)]
fn change_count() -> Option<u64> {
    // SAFETY: takes no arguments and only reads a counter.
    Some(unsafe { windows_sys::Win32::System::DataExchange::GetClipboardSequenceNumber() }.into())
}

#[cfg(target_os = "macos")]
fn change_count() -> Option<u64> {
    let board = objc2_app_kit::NSPasteboard::generalPasteboard();
    u64::try_from(board.changeCount()).ok()
}

#[cfg(not(any(windows, target_os = "macos")))]
fn change_count() -> Option<u64> {
    None
}

pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(TICK);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // What the clipboard held when watching began is not offered (#738).
        let mut watching = false;
        let mut count = None;
        loop {
            interval.tick().await;
            let config = app
                .state::<RuntimeConfigState>()
                .0
                .read()
                .await
                .clipboard
                .clone();
            if !config.enable || !config.background {
                watching = false;
                continue;
            }
            let now = change_count();
            if watching && now.is_some() && now == count {
                continue;
            }
            count = now;
            let Ok(text) = app.clipboard().read_text() else {
                watching = true;
                continue;
            };
            if !watching {
                watching = true;
                crate::commands::clipboard_offer(text);
                continue;
            }
            // A copy inside Rayburst is not a link to download.
            let focused = app
                .webview_windows()
                .values()
                .any(|window| window.is_focused().unwrap_or(false));
            if focused || !detect(&text, &config) || !crate::commands::clipboard_offer(text) {
                continue;
            }
            log::info!("clipboard: a copied link opens the add window");
            super::add_window::route(
                &app,
                super::add_window::AddWindowInputs {
                    manual: true,
                    ..Default::default()
                },
                "clipboard",
            );
        }
    });
}

fn is_file_url(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    let name = url.path().rsplit('/').next().unwrap_or_default();
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        (1..=8).contains(&extension.len())
            && extension.chars().all(|ch| ch.is_ascii_alphanumeric())
            && !WEB_PAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
    })
}

fn info_hash(line: &str) -> bool {
    match line.len() {
        40 | 64 => line.chars().all(|ch| ch.is_ascii_hexdigit()),
        32 => line
            .chars()
            .all(|ch| ch.is_ascii_uppercase() || ('2'..='7').contains(&ch)),
        _ => false,
    }
}

fn allowed_tags(config: &ClipboardConfig) -> Vec<&'static str> {
    [
        (config.http, &["http://", "https://"][..]),
        (config.sftp, &["sftp://"]),
        (config.magnet, &["magnet:"]),
        (config.ed2k, &["ed2k://"]),
        (config.thunder, &["thunder://"]),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .flat_map(|(_, tags)| tags.iter().copied())
    .collect()
}

fn resource(line: &str, tags: &[&str], hash: bool, files_only: bool) -> bool {
    let lower = line.to_ascii_lowercase();
    if files_only
        && (lower.starts_with("http://") || lower.starts_with("https://"))
        && !is_file_url(line)
    {
        return false;
    }
    tags.iter()
        .any(|tag| lower.starts_with(tag) && line.len() > tag.len())
        || (hash && info_hash(line))
}

/// Whether the text is only links, as `detectResource` decides: one link or
/// hash, or an aria2 input list whose lines are links and indented options.
pub(crate) fn detect(text: &str, config: &ClipboardConfig) -> bool {
    if !config.enable || text.is_empty() || text.chars().count() > MAX_CHARS {
        return false;
    }
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if lines.is_empty() || lines.len() > MAX_LINES {
        return false;
    }
    if let [line] = lines[..] {
        return resource(
            line,
            &allowed_tags(config),
            config.bt_hash,
            config.http_files_only,
        );
    }
    let mut entry = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let indented = raw.starts_with([' ', '\t']);
        let option = line.split_once('=').is_some_and(|(key, _)| {
            !key.is_empty()
                && key
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        });
        if indented && option && entry {
            continue;
        }
        let uris: Vec<&str> = line
            .split('\t')
            .map(str::trim)
            .filter(|uri| !uri.is_empty())
            .collect();
        if uris.is_empty()
            || !uris.iter().all(|uri| {
                resource(uri, &ALL_TAGS, true, false)
                    && (!config.http_files_only
                        || !uri.to_ascii_lowercase().starts_with("http")
                        || is_file_url(uri))
            })
        {
            return false;
        }
        entry = true;
    }
    entry
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ClipboardConfig {
        ClipboardConfig {
            background: true,
            ..ClipboardConfig::default()
        }
    }

    #[test]
    fn detects_links_like_the_add_window() {
        let config = config();
        assert!(detect("https://example.com/a.zip", &config));
        assert!(!detect("https://example.com/page.html", &config));
        assert!(!detect("https://example.com/", &config));
        assert!(detect("magnet:?xt=urn:btih:abc", &config));
        assert!(detect(&"a".repeat(40), &config));
        assert!(!detect("see https://example.com/a.zip", &config));
        assert!(detect(
            "https://example.com/a.zip\n  out=dir/a.zip\nhttps://example.com/b.zip\thttps://mirror.example/b.zip\n",
            &config
        ));
        assert!(!detect("https://example.com/a.zip\nhello", &config));
        let off = ClipboardConfig {
            http: false,
            ..config.clone()
        };
        assert!(!detect("https://example.com/a.zip", &off));
        let pages = ClipboardConfig {
            http_files_only: false,
            ..config
        };
        assert!(detect("https://example.com/", &pages));
    }
}
