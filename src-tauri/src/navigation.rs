//! Keeps every webview on the application's own pages.
//!
//! A link in rendered content, such as release notes, would otherwise replace
//! the page and strand the window without its controls (#764). Web links open
//! in the system browser instead; any other destination is refused.

use tauri::{plugin::TauriPlugin, Runtime};
use tauri_plugin_opener::OpenerExt;
use url::Url;

#[derive(Debug, PartialEq, Eq)]
enum Destination {
    App,
    Browser,
    Refused,
}

fn destination(url: &Url) -> Destination {
    match url.scheme() {
        // Bundled pages: tauri:// on macOS and Linux, http://tauri.localhost
        // on Windows, and the Vite server during development.
        "tauri" | "asset" | "about" | "data" | "blob" => Destination::App,
        "http" | "https"
            if matches!(
                url.host_str(),
                Some("tauri.localhost" | "localhost" | "127.0.0.1")
            ) =>
        {
            Destination::App
        }
        "http" | "https" | "mailto" => Destination::Browser,
        _ => Destination::Refused,
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::new("navigation")
        .on_navigation(|webview, url| match destination(url) {
            Destination::App => true,
            Destination::Browser => {
                if let Err(error) = webview.opener().open_url(url.as_str(), None::<&str>) {
                    log::warn!("navigation:open-external-failed url={url} error={error}");
                }
                false
            }
            Destination::Refused => {
                log::warn!("navigation:refused url={url}");
                false
            }
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(url: &str) -> Destination {
        destination(&Url::parse(url).expect("valid URL"))
    }

    #[test]
    fn app_pages_stay_and_web_links_leave() {
        assert_eq!(of("tauri://localhost/index.html"), Destination::App);
        assert_eq!(of("http://tauri.localhost/"), Destination::App);
        assert_eq!(of("http://localhost:1420/"), Destination::App);
        assert_eq!(of("about:blank"), Destination::App);
        assert_eq!(
            of("https://github.com/AnInsomniacy/rayburst/releases"),
            Destination::Browser
        );
        assert_eq!(of("mailto:someone@example.com"), Destination::Browser);
        assert_eq!(of("file:///C:/Windows/notepad.exe"), Destination::Refused);
        assert_eq!(of("javascript:alert(1)"), Destination::Refused);
    }
}
