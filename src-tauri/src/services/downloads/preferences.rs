//! Apply desktop preferences to browser download intent.
use super::{
    category::{self, Candidate, Category},
    contracts::AddRequest,
};
use crate::error::AppError;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct Preferences {
    pub(super) extension_download_behavior: ExtensionDownloadBehavior,
    pub(crate) dir: String,
    remember_save_location: bool,
    last_save_location: String,
    pub(crate) file_category_enabled: bool,
    pub(crate) file_categories: Vec<Category>,
    pub(crate) magnet_file_selection_policy: crate::database::MagnetSelectionPolicy,
    user_agent: String,
    user_agent_profiles: Vec<Profile>,
    user_agent_rules: Vec<Rule>,
    site_rules: Vec<SiteRule>,
    proxy: ProxySettings,
    #[serde(default = "enabled")]
    pub(super) bittorrent_enabled: bool,
}

fn enabled() -> bool {
    true
}
#[derive(Default, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum ExtensionDownloadBehavior {
    #[default]
    Confirm,
    Background,
    Show,
}
#[derive(Deserialize)]
struct Profile {
    id: String,
    value: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Rule {
    enabled: bool,
    host_pattern: String,
    profile_id: String,
    override_plugin: bool,
}

/// Per-site download settings: a connection cap for sites that ban parallel
/// downloads and a proxy choice that overrides the global download scope.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SiteRule {
    enabled: bool,
    host_pattern: String,
    #[serde(default)]
    max_connection_per_server: u32,
    #[serde(default)]
    proxy: SiteProxy,
}

#[derive(Default, Deserialize, PartialEq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
enum SiteProxy {
    #[default]
    Default,
    Proxy,
    Direct,
}

/// The configured proxy server; a site rule may use it while downloads go direct.
#[derive(Default, Deserialize)]
#[serde(default)]
struct ProxySettings {
    server: String,
    username: Option<String>,
    password: Option<String>,
}

pub(crate) fn load(app: &AppHandle) -> Result<Preferences, AppError> {
    let value = app
        .store("config.json")
        .map_err(|error| AppError::Store(error.to_string()))?
        .get("preferences")
        .ok_or_else(|| AppError::Store("Preferences are unavailable".into()))?;
    Ok(serde_json::from_value(value)?)
}

pub(super) fn validate_save_location(app: &AppHandle, options: &Value) -> Result<(), AppError> {
    let prefs = load(app)?;
    if prefs.remember_save_location
        && !prefs.last_save_location.is_empty()
        && options["dir"].as_str() == Some(&prefs.last_save_location)
        && !std::fs::metadata(&prefs.last_save_location)
            .map_err(AppError::from)?
            .is_dir()
    {
        return Err(AppError::InvalidInput(
            "The remembered save location is not a directory".into(),
        ));
    }
    Ok(())
}

fn matches(pattern: &str, value: &str) -> bool {
    let expression = format!("^{}$", regex::escape(pattern).replace("\\*", ".*"));
    regex::RegexBuilder::new(&expression)
        .case_insensitive(true)
        .build()
        .is_ok_and(|matcher| matcher.is_match(value))
}

/// A pattern without `*` names a domain and its subdomains.
fn host_matches(pattern: &str, host: &str) -> bool {
    let pattern = pattern.trim().trim_end_matches('.');
    if pattern.contains('*') {
        return matches(pattern, host);
    }
    host.eq_ignore_ascii_case(pattern)
        || host.len() > pattern.len()
            && host.as_bytes()[host.len() - pattern.len() - 1] == b'.'
            && host[host.len() - pattern.len()..].eq_ignore_ascii_case(pattern)
}

/// Applies the site rules naming a host of the URIs: the strictest
/// connection cap (an explicit lower task value stays) and the first proxy
/// choice. A task that already names a proxy keeps it unless a rule says direct.
pub(super) fn apply_site_rules(prefs: &Preferences, uris: &[String], options: &mut Value) {
    if !options.is_object() {
        return;
    }
    let hosts: Vec<String> = uris
        .iter()
        .filter_map(|uri| url::Url::parse(uri).ok())
        .filter_map(|url| url.host_str().map(str::to_owned))
        .collect();
    let matching: Vec<&SiteRule> = prefs
        .site_rules
        .iter()
        .filter(|rule| rule.enabled)
        .filter(|rule| {
            hosts
                .iter()
                .any(|host| host_matches(&rule.host_pattern, host))
        })
        .collect();
    if let Some(proxy) = matching
        .iter()
        .map(|rule| rule.proxy)
        .find(|proxy| *proxy != SiteProxy::Default)
    {
        apply_site_proxy(&prefs.proxy, proxy, options);
    }
    let Some(limit) = matching
        .iter()
        .map(|rule| rule.max_connection_per_server)
        .filter(|limit| *limit > 0)
        .min()
    else {
        return;
    };
    let current = options
        .get("max-connection-per-server")
        .and_then(|value| match value {
            Value::String(value) => value.parse::<u32>().ok(),
            value => value.as_u64().and_then(|value| u32::try_from(value).ok()),
        })
        .filter(|value| *value > 0);
    let limit = current.map_or(limit, |current| current.min(limit));
    options["max-connection-per-server"] = limit.to_string().into();
}

fn apply_site_proxy(settings: &ProxySettings, proxy: SiteProxy, options: &mut Value) {
    let named = options
        .get("all-proxy")
        .and_then(Value::as_str)
        .is_some_and(|server| !server.trim().is_empty());
    match proxy {
        SiteProxy::Direct => {
            for key in ["all-proxy", "all-proxy-user", "all-proxy-passwd"] {
                options[key] = "".into();
            }
        }
        SiteProxy::Proxy if !named && !settings.server.trim().is_empty() => {
            options["all-proxy"] = settings.server.trim().into();
            let username = settings.username.as_deref().unwrap_or("").trim();
            let password = settings.password.as_deref().unwrap_or("");
            if !username.is_empty() || !password.is_empty() {
                options["all-proxy-user"] = username.into();
                options["all-proxy-passwd"] = password.into();
            }
        }
        _ => {}
    }
}

pub(super) fn options(prefs: &Preferences, request: &AddRequest) -> Result<Value, AppError> {
    let urls: Vec<&str> = [
        request.final_url.as_deref(),
        Some(request.url.as_str()),
        request.referer.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect();
    let mut options = json!({});
    if !prefs.dir.is_empty() {
        options["dir"] = prefs.dir.clone().into();
    }
    if let Some(name) = request.filename.as_ref().filter(|name| !name.is_empty()) {
        options["filename-hint"] = name.clone().into();
        options["filename-hint-source"] = serde_json::to_value(request.filename_source)?;
    }
    if prefs.remember_save_location && !prefs.last_save_location.is_empty() {
        let path = std::path::Path::new(&prefs.last_save_location);
        if !std::fs::metadata(path).map_err(AppError::from)?.is_dir() {
            return Err(AppError::InvalidInput(
                "The remembered save location is not a directory".into(),
            ));
        }
        options["dir"] = prefs.last_save_location.clone().into();
    } else if prefs.file_category_enabled
        && !url::Url::parse(&request.url).is_ok_and(|url| url.scheme() == "magnet")
    {
        let name = request.filename.clone().unwrap_or_else(|| {
            url::Url::parse(urls[0])
                .ok()
                .map(|url| {
                    urlencoding::decode(url.path())
                        .map(std::borrow::Cow::into_owned)
                        .unwrap_or_else(|_| url.path().to_owned())
                })
                .unwrap_or_default()
        });
        if let Some(category) = category::resolve(
            &[Candidate {
                path: name,
                urls: urls.iter().map(|url| (*url).to_owned()).collect(),
            }],
            &prefs.file_categories,
            &prefs.dir,
        )? {
            options["dir"] = category.directory.into();
        }
    }
    let rule = prefs
        .user_agent_rules
        .iter()
        .filter(|rule| rule.enabled)
        .find_map(|rule| {
            let matched = urls
                .iter()
                .filter_map(|value| url::Url::parse(value).ok())
                .any(|url| {
                    url.host_str()
                        .is_some_and(|host| matches(&rule.host_pattern, host))
                });
            matched
                .then(|| {
                    prefs
                        .user_agent_profiles
                        .iter()
                        .find(|profile| profile.id == rule.profile_id)
                        .map(|profile| (rule, profile))
                })
                .flatten()
        });
    let user_agent = match rule {
        Some((rule, profile))
            if rule.override_plugin || request.user_agent.as_deref().is_none_or(str::is_empty) =>
        {
            &profile.value
        }
        _ => request
            .user_agent
            .as_ref()
            .filter(|value| !value.is_empty())
            .unwrap_or(&prefs.user_agent),
    };
    let mut headers = reqwest::header::HeaderMap::new();
    for header in &request.request_headers {
        let name = reqwest::header::HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|error| AppError::InvalidInput(error.to_string()))?;
        let value = reqwest::header::HeaderValue::from_str(&header.value)
            .map_err(|error| AppError::InvalidInput(error.to_string()))?;
        headers.insert(name, value);
    }
    for (name, value) in [
        ("cookie", request.cookie.as_deref()),
        ("referer", request.referer.as_deref()),
        ("user-agent", Some(user_agent.as_str())),
    ] {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            let value = reqwest::header::HeaderValue::from_str(value)
                .map_err(|error| AppError::InvalidInput(error.to_string()))?;
            headers.insert(reqwest::header::HeaderName::from_static(name), value);
        }
    }
    if !headers.is_empty() {
        let lines = headers
            .iter()
            .map(|(name, value)| {
                value
                    .to_str()
                    .map(|value| format!("{name}: {value}"))
                    .map_err(|error| AppError::InvalidInput(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        options["header"] = lines.join("\n").into();
    }
    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_intent_uses_matching_preferences_and_one_value_per_header() {
        let prefs: Preferences = serde_json::from_value(json!({
            "dir":std::env::temp_dir().to_string_lossy(),
            "fileCategoryEnabled":true,
            "fileCategories":[{
                "directory":"documents", "directoryMode":"relative", "label":"Documents", "extensions":["pdf"],
                "urlPatterns":["https://*.example.test/*"]
            }],
            "userAgentProfiles":[{"id":"site", "value":"Site agent"}],
            "userAgentRules":[{
                "enabled":true,"hostPattern":"*.example.test",
                "profileId":"site","overridePlugin":true
            }]
        }))
        .unwrap();
        let request: AddRequest = serde_json::from_value(json!({
            "id":"intent", "url":"https://cdn.example.test/download",
            "filename":"report%20.pdf", "filenameSource":"browser",
            "userAgent":"Browser agent", "cookie":"session=current",
            "requestHeaders":[
                {"name":"User-Agent","value":"Captured agent"},
                {"name":"Cookie","value":"session=old"},
                {"name":"X-Token","value":"opaque"}
            ]
        }))
        .unwrap();
        let options = options(&prefs, &request).unwrap();
        assert_eq!(options["filename-hint"], "report%20.pdf");
        assert_eq!(options["filename-hint-source"], "browser");
        assert_eq!(
            std::path::Path::new(options["dir"].as_str().unwrap()),
            std::env::temp_dir().join("documents")
        );
        let headers: Vec<_> = options["header"].as_str().unwrap().lines().collect();
        assert_eq!(headers.len(), 3);
        assert!(headers.contains(&"user-agent: Site agent"));
        assert!(headers.contains(&"cookie: session=current"));
        assert!(headers.contains(&"x-token: opaque"));
    }

    #[test]
    fn site_rules_choose_the_proxy_for_matching_hosts() {
        let prefs: Preferences = serde_json::from_value(json!({
            "proxy":{"mode":"direct","server":"socks5://127.0.0.1:1080","username":"me","password":"pw"},
            "siteRules":[
                {"enabled":true,"hostPattern":"github.com","proxy":"proxy"},
                {"enabled":true,"hostPattern":"mirror.cn","maxConnectionPerServer":0,"proxy":"direct"}
            ]
        }))
        .unwrap();
        let apply = |uri: &str, options: Value| {
            let mut options = options;
            apply_site_rules(&prefs, &[uri.to_owned()], &mut options);
            options
        };
        let proxied = apply("https://objects.github.com/a.zip", json!({}));
        assert_eq!(proxied["all-proxy"], "socks5://127.0.0.1:1080");
        assert_eq!(proxied["all-proxy-user"], "me");
        assert!(proxied.get("max-connection-per-server").is_none());
        let own = apply(
            "https://github.com/a.zip",
            json!({"all-proxy":"http://task:8080"}),
        );
        assert_eq!(own["all-proxy"], "http://task:8080");
        let direct = apply(
            "https://mirror.cn/a.iso",
            json!({"all-proxy":"http://global:8080"}),
        );
        assert_eq!(direct["all-proxy"], "");
        assert_eq!(apply("https://other.org/a", json!({})), json!({}));
    }

    #[test]
    fn site_rules_cap_connections_for_matching_hosts() {
        let prefs: Preferences = serde_json::from_value(json!({
            "siteRules":[
                {"enabled":true,"hostPattern":"amd.com","maxConnectionPerServer":2},
                {"enabled":true,"hostPattern":"*.cdn.test","maxConnectionPerServer":4},
                {"enabled":false,"hostPattern":"example.test","maxConnectionPerServer":1}
            ]
        }))
        .unwrap();
        let apply = |uri: &str, options: Value| {
            let mut options = options;
            apply_site_rules(&prefs, &[uri.to_owned()], &mut options);
            options
        };
        let capped = apply(
            "https://drivers.AMD.com/a.exe",
            json!({"max-connection-per-server":"16"}),
        );
        assert_eq!(capped["max-connection-per-server"], "2");
        assert_eq!(
            apply("https://amd.com/a.exe", json!({}))["max-connection-per-server"],
            "2"
        );
        assert_eq!(apply("https://notamd.com/a.exe", json!({})), json!({}));
        assert_eq!(
            apply(
                "https://x.cdn.test/a",
                json!({"max-connection-per-server":"1"})
            )["max-connection-per-server"],
            "1"
        );
        assert_eq!(apply("https://example.test/a", json!({})), json!({}));
    }

    #[test]
    fn invalid_browser_headers_cannot_inject_another_request_header() {
        let request: AddRequest = serde_json::from_value(json!({
            "id":"intent", "url":"https://example.test/file",
            "referer":"https://example.test/\r\nX-Injected: value"
        }))
        .unwrap();
        assert!(matches!(
            options(&Preferences::default(), &request),
            Err(AppError::InvalidInput(_))
        ));
    }

    #[test]
    fn remembered_location_precedes_classification_only_when_enabled() {
        let root = tempfile::tempdir().unwrap();
        let remembered = root.path().join("chosen");
        std::fs::create_dir(&remembered).unwrap();
        let mut prefs: Preferences = serde_json::from_value(json!({
            "dir":root.path(), "rememberSaveLocation":true, "lastSaveLocation":remembered,
            "fileCategoryEnabled":true, "fileCategories":[{"directory":"documents", "directoryMode":"relative", "label":"Documents", "extensions":["pdf"], "urlPatterns":[]}]
        })).unwrap();
        let request: AddRequest = serde_json::from_value(
            json!({"id":"location", "url":"https://example.test/report.pdf"}),
        )
        .unwrap();
        assert_eq!(
            options(&prefs, &request).unwrap()["dir"],
            remembered.to_string_lossy().as_ref()
        );
        prefs.remember_save_location = false;
        assert_eq!(
            std::path::Path::new(options(&prefs, &request).unwrap()["dir"].as_str().unwrap()),
            root.path().join("documents")
        );
        prefs.remember_save_location = true;
        std::fs::remove_dir(&remembered).unwrap();
        assert!(options(&prefs, &request).is_err());
    }
}
