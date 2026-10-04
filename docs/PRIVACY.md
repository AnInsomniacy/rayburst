# Privacy Policy

**Last updated:** 2026-09-17

Rayburst is an open-source desktop download manager licensed under the [MIT License](https://opensource.org/licenses/MIT). This document describes what data the application handles and what network connections it makes.

## Data Collection

Rayburst does **not** collect, store, or transmit telemetry, analytics, usage profiles, account data, or advertising identifiers. There is no account system and no third-party analytics SDK.

## Local Data Storage

Application data is stored locally on your device and is not synced by Rayburst:

| Data                  | Location                                | Purpose                                     |
| --------------------- | --------------------------------------- | ------------------------------------------- |
| Preferences           | `config.json` (app data directory)      | User settings                               |
| Engine options        | `system.json` (app data directory)      | Aria2 Next runtime configuration            |
| Download history      | `history.db` (local SQLite database)    | Task records                                |
| Engine task manifest  | `download.session` (app data directory) | Restore the task list                       |
| Engine transfer state | `engine/state` (app data directory)     | Restore protocol progress and sharing state |
| Media receipts        | `media-operations.db` (app local-data directory) | Reconcile media submissions |
| Browser captures      | `media-captures/` (app local-data directory) | Store captured media for native processing |
| Application logs      | app log directory                       | Diagnostics and troubleshooting             |
| Download files        | User-specified directory                | Downloaded content                          |

Diagnostic log exports are created only when the user chooses **Advanced Settings → Export Diagnostic Logs**. The exported ZIP contains the Rayburst and Aria2 Next logs plus `diagnostics.json`, which combines system/runtime metadata with sanitized configuration. RPC secrets, Extension API secrets, cookies, and proxy credentials are redacted before export.

## Automatic Network Connections

Rayburst can make the following automatic network connections. They can be disabled in Settings.

### 1. Update Check

|                   |                                                                                                   |
| ----------------- | ------------------------------------------------------------------------------------------------- |
| **Default**       | Enabled, every application startup                                                                |
| **Contacts**      | GitHub-hosted updater metadata and release assets                                                 |
| **Purpose**       | Check if a newer version of Rayburst is available                                                 |
| **Data sent**     | Standard HTTPS request metadata, including client IP as seen by GitHub                            |
| **Data received** | Version metadata, release notes, signatures, and update package when the user downloads an update |
| **Disable**       | Settings → General → uncheck "Check for updates automatically"                                    |

Local and release builds use the configured update endpoint and public signing key. Update checks follow the selected Stable or Beta policy. Beta includes stable releases; neither policy offers a downgrade. If a proxy is configured for app updates, update checks use that proxy.

### 2. BT Tracker List Sync

|                   |                                                                                                   |
| ----------------- | ------------------------------------------------------------------------------------------------- |
| **Default**       | Enabled, at most once every 12 hours                                                              |
| **Contacts**      | Configured community tracker list URLs, such as `cdn.jsdelivr.net` or `raw.githubusercontent.com` |
| **Purpose**       | Update BitTorrent tracker lists for better peer discovery                                         |
| **Data sent**     | Standard HTTP GET request (no user data)                                                          |
| **Data received** | Plain-text tracker URL list                                                                       |
| **Disable**       | Settings → BitTorrent → uncheck "Auto-update tracker list"                                        |

## User-Initiated Network Connections

When you add a download task, Rayburst and its Aria2 Next sidecar connect to the servers or peers needed for that task. This can include HTTP, HTTPS, or SFTP servers, BitTorrent trackers, DHT nodes, peers, ED2K servers, and media manifests, segments and key endpoints.

The engine resolves filenames from the actual download response. The desktop does not issue separate requests just to guess filenames. Torrent file inspection and media manifest inspection may fetch metadata before content selection.

If UPnP is enabled, the app may contact your local network gateway to map BitTorrent ports. If system proxy detection is used, the app reads operating-system proxy settings locally.

## Browser Extension API

Rayburst includes an embedded Extension API for browser extensions. It defaults to port `29110` and uses an Extension API secret that is independent from the aria2 RPC secret.

The Extension API can receive download URLs, referer values, cookie headers, and filename hints from the browser extension. Rust processes these according to confirmation and auto-submit settings. Pending ordinary confirmations are stored locally in `history.db` so they survive desktop restart; submission or cancellation clears their request bodies. Request IDs, fingerprints, GIDs and receipt states remain until database reset. Media operation receipts use `media-operations.db` and do not contain browser credentials. Browser captures are stored in `media-captures/`; hourly cleanup removes unclaimed captures older than 24 hours while retaining inputs needed by pending tasks. Captured manifests and supplied keys can remain in native task options for retry, but are excluded from download history.

The API accepts Rayburst Connect browser requests and authenticated native clients. Regular web-page origins are rejected. Users can change the port or clear the secret in Advanced Settings. Clearing the secret disables authentication for ordinary download endpoints and prevents use of the media API.

## Website Privacy

The static website uses local assets and stores language and theme preferences in browser local storage. It requests public repository statistics and release metadata from the GitHub API, which receives standard HTTPS request metadata including the visitor's IP address. Download links lead to GitHub release assets. The website has no analytics SDK. Opening the English film loads YouTube’s privacy-enhanced embedded player, which sends request metadata, including the visitor’s IP address, to Google. No YouTube resources are loaded before this action. Privacy-enhanced mode does not eliminate data transmission. Watching on YouTube is subject to Google’s privacy policy.

## Third-Party Components

| Component                                                               | Purpose                    | Network behavior                                                          |
| ----------------------------------------------------------------------- | -------------------------- | ------------------------------------------------------------------------- |
| [Aria2 Next](https://github.com/AnInsomniacy/aria2-next)                | Download engine            | Connects to download servers and BitTorrent peers as directed by the user |
| [DB-IP](https://db-ip.com/)                                             | GeoIP database (CC BY 4.0) | **Offline only** — bundled database, no network requests                  |
| [Tauri Updater Plugin](https://github.com/tauri-apps/plugins-workspace) | Auto-update framework      | Used for update checks and update installation                            |

## Children's Privacy

Rayburst has no accounts or age profiling. The local data described above is used to manage downloads.

## Changes to This Policy

Updates to this privacy policy will be posted in this file within the project's GitHub repository. The "Last updated" date at the top will be revised accordingly.

## Contact

For privacy-related questions, please open an issue on GitHub:
https://github.com/AnInsomniacy/rayburst/issues
