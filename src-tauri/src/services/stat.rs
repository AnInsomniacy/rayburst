//! Native effects of the global statistics: tray title, Dock badge and
//! progress, taskbar progress and the keep-awake assertion.
//!
//! The pump reads the statistics with each snapshot and applies them here
//! directly from Rust, so they keep working while the WebView is destroyed in
//! lightweight mode.

use super::config::RuntimeConfig;
use super::power::{PowerGuard, RETRY_DELAY as POWER_RETRY_DELAY};
#[cfg(target_os = "windows")]
use tauri::Manager;

/// Global statistics as sent to the frontend with each snapshot.
#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatUpdate {
    pub download_speed: u64,
    pub upload_speed: u64,
    pub num_active: u64,
    pub num_waiting: u64,
    pub num_stopped: u64,
    pub num_stopped_total: u64,
}

/// Computes a human-readable compact size string (e.g., "1.5M", "200K").
///
/// Matches the frontend `compactSize` output used for tray/dock display.
fn compact_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.1}G", b / GB)
    } else if b >= MB {
        format!("{:.1}M", b / MB)
    } else if b >= KB {
        format!("{:.0}K", b / KB)
    } else {
        format!("{b}B")
    }
}

fn tray_title_for_speed(tray_speedometer: bool, download_speed: u64, upload_speed: u64) -> String {
    if !tray_speedometer || (download_speed == 0 && upload_speed == 0) {
        return String::new();
    }

    if download_speed > 0 {
        format!("↓{}", compact_size(download_speed))
    } else {
        format!("↑{}", compact_size(upload_speed))
    }
}

/// Tooltip of the tray icon: the speeds while anything is active.
fn tray_tooltip(num_active: u64, download_speed: u64, upload_speed: u64) -> String {
    if num_active == 0 {
        return "Rayburst".into();
    }
    format!(
        "Rayburst\n↓ {}/s  ↑ {}/s",
        compact_size(download_speed),
        compact_size(upload_speed)
    )
}

fn tray_title_needs_update(last_title: &Option<String>, next_title: &str) -> bool {
    last_title.as_deref() != Some(next_title)
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn parse_length(value: Option<&str>) -> u64 {
    value.and_then(|v| v.parse::<u64>().ok()).unwrap_or(0)
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn completed_length(task: &crate::aria2::types::Aria2Task) -> u64 {
    parse_length(Some(&task.completed_length))
}

/// Use task fractions for mixed media/file work; their raw units cannot be summed.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn task_progress(tasks: &[crate::aria2::types::Aria2Task]) -> Option<u64> {
    let tasks: Vec<_> = tasks
        .iter()
        .filter(|task| task.seeder.as_deref() != Some("true"))
        .collect();
    if tasks.is_empty() {
        return None;
    }
    if tasks.iter().any(|task| task.media.is_some()) {
        let mut sum = 0.0;
        for task in &tasks {
            let fraction = if let Some(media) = &task.media {
                if media.live == "true" || media.state == "finalizing" {
                    return None;
                }
                media.progress.as_deref()?.parse::<f64>().ok()?
            } else {
                let total = parse_length(Some(&task.total_length));
                if total == 0 {
                    return None;
                }
                completed_length(task) as f64 / total as f64
            };
            if !fraction.is_finite() {
                return None;
            }
            sum += fraction.clamp(0.0, 1.0);
        }
        return Some((sum / tasks.len() as f64 * 100.0) as u64);
    }
    let total: u64 = tasks
        .iter()
        .map(|task| parse_length(Some(&task.total_length)))
        .sum();
    let completed: u64 = tasks.iter().map(|task| completed_length(task)).sum();
    (total > 0).then(|| ((completed as f64 / total as f64).clamp(0.0, 1.0) * 100.0) as u64)
}

/// Sets the macOS Dock badge label using `NSApp().dockTile().setBadgeLabel()`.
///
/// This is an **app-level** API that accesses `NSApplication.sharedApplication()`
/// directly — it does NOT require a `WebviewWindow` handle.  This is critical
/// because in lightweight mode `window.destroy()` kills the WebView, making
/// `get_webview_window("main")` return `None`.  The previous implementation
/// used `window.set_badge_label()` which silently failed in that case.
///
/// Pass `None` to clear the badge.
///
/// # Safety
///
/// Must be called from the main thread (macOS UI thread requirement).
/// The caller is responsible for dispatching via `app.run_on_main_thread()`
/// when invoking from an async tokio worker.
///
/// # Platform
///
/// Compiles only on macOS (`#[cfg(target_os = "macos")]`).  On other
/// platforms, callers gate the call with `#[cfg(...)]` at the call site.
#[cfg(target_os = "macos")]
pub(crate) fn set_dock_badge(label: Option<&str>) {
    use objc2_app_kit::NSApplication;
    use objc2_foundation::{MainThreadMarker, NSString};

    // SAFETY: Caller must ensure this runs on the main thread.
    // MainThreadMarker::new() returns None on background threads,
    // so we try it first and bail with a warning if not on main.
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!("set_dock_badge called from non-main thread — badge not updated");
        return;
    };

    let app = NSApplication::sharedApplication(mtm);
    let dock_tile = app.dockTile();
    let ns_label = label.map(NSString::from_str);
    dock_tile.setBadgeLabel(ns_label.as_deref());
    dock_tile.display();
}

/// Sets the macOS Dock progress bar via `NSDockTile` + custom `NSProgressIndicator`.
///
/// A plain `NSProgressIndicator` does not render inside `NSDockTile` because
/// dock tiles use image-based compositing: `display()` calls `drawRect:` to
/// capture a bitmap snapshot, but `NSProgressIndicator`'s `drawRect:` relies on
/// the window compositor's CALayer tree which doesn't exist for dock tiles.
///
/// The fix: register a custom `NSProgressIndicator` subclass (`RayburstProgressIndicator`)
/// with a `drawRect:` override that manually paints using `NSBezierPath`.  This is
/// the same approach used by tao's `TaoProgressIndicator` — the industry-standard
/// workaround for dock tile progress rendering.
///
/// Pass `None` to hide the progress bar, or `Some(0..=100)` to show it.
///
/// # Safety
///
/// Must be called from the main thread (macOS UI thread requirement).
#[cfg(target_os = "macos")]
pub(crate) fn set_dock_progress(progress: Option<u64>) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    // SAFETY: All NSApplication/NSDockTile APIs must be called from the main thread.
    unsafe {
        let ns_app: *mut AnyObject = msg_send![objc2::class!(NSApplication), sharedApplication];
        let dock_tile: *mut AnyObject = msg_send![ns_app, dockTile];
        if dock_tile.is_null() {
            return;
        }

        // Find existing progress indicator or create one
        let indicator: *mut AnyObject = get_or_create_progress_indicator(ns_app, dock_tile);
        if indicator.is_null() {
            return;
        }

        match progress {
            Some(pct) => {
                let value = pct.clamp(0, 100) as f64;
                let _: () = msg_send![indicator, setDoubleValue: value];
                let _: () = msg_send![indicator, setHidden: false];
            }
            None => {
                let _: () = msg_send![indicator, setHidden: true];
            }
        }

        let _: () = msg_send![dock_tile, display];
    }
}

/// Finds an existing `NSProgressIndicator` subclass in the dock tile's content view,
/// or creates a new `RayburstProgressIndicator` (custom subclass with `drawRect:` override).
///
/// A plain `NSProgressIndicator` is invisible in dock tiles because `NSDockTile.display()`
/// captures a bitmap by calling `drawRect:` on the content view hierarchy, and the stock
/// `NSProgressIndicator.drawRect:` depends on the window compositor's CALayer tree.
///
/// Our custom subclass overrides `drawRect:` to manually paint the progress bar using
/// `NSBezierPath` — the same technique used by tao's `TaoProgressIndicator`.
#[cfg(target_os = "macos")]
unsafe fn get_or_create_progress_indicator(
    ns_app: *mut objc2::runtime::AnyObject,
    dock_tile: *mut objc2::runtime::AnyObject,
) -> *mut objc2::runtime::AnyObject {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    // Try to find existing indicator (ours or tao's — both are NSProgressIndicator subclasses)
    let content_view: *mut AnyObject = msg_send![dock_tile, contentView];
    if !content_view.is_null() {
        let subviews: *mut AnyObject = msg_send![content_view, subviews];
        if !subviews.is_null() {
            let count: usize = msg_send![subviews, count];
            for i in 0..count {
                let subview: *mut AnyObject = msg_send![subviews, objectAtIndex: i];
                let is_progress: bool =
                    msg_send![subview, isKindOfClass: objc2::class!(NSProgressIndicator)];
                if is_progress {
                    return subview;
                }
            }
        }
    }

    // No existing indicator — create one with custom drawRect:
    let mut image_view: *mut AnyObject = msg_send![dock_tile, contentView];
    if image_view.is_null() {
        let app_icon: *mut AnyObject = msg_send![ns_app, applicationIconImage];
        image_view = msg_send![objc2::class!(NSImageView), imageViewWithImage: app_icon];
        let _: () = msg_send![dock_tile, setContentView: image_view];
    }

    let dock_size: NSSize = msg_send![dock_tile, size];
    let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(dock_size.width, 15.0));

    let progress_class = register_progress_indicator_class();
    let indicator: *mut AnyObject = msg_send![progress_class, alloc];
    let indicator: *mut AnyObject = msg_send![indicator, initWithFrame: frame];
    let _: *mut AnyObject = msg_send![indicator, autorelease];

    let _: () = msg_send![image_view, addSubview: indicator];
    indicator
}

/// Registers the `RayburstProgressIndicator` ObjC class (once) — a custom
/// `NSProgressIndicator` subclass with a `drawRect:` override for dock tile rendering.
///
/// The class draws a rounded progress bar using `NSBezierPath`:
/// - Gray track (semi-transparent white)
/// - Blue fill proportional to `doubleValue / 100.0`
#[cfg(target_os = "macos")]
fn register_progress_indicator_class() -> *const objc2::runtime::AnyClass {
    use objc2::runtime::{AnyClass, ClassBuilder, Sel};
    use objc2_foundation::NSRect;

    use std::sync::Once;

    static mut CLASS: *const AnyClass = std::ptr::null();
    static INIT: Once = Once::new();

    INIT.call_once(|| unsafe {
        let superclass = objc2::class!(NSProgressIndicator);
        let mut decl = ClassBuilder::new(c"RayburstProgressIndicator", superclass)
            .expect("Failed to create RayburstProgressIndicator class");

        // Register the custom drawRect: method.
        // Uses raw pointer (*mut AnyObject) to satisfy the HRTB lifetime
        // bound on objc2's MethodImplementation trait.
        decl.add_method(
            objc2::sel!(drawRect:),
            draw_progress_bar as unsafe extern "C" fn(*mut objc2::runtime::AnyObject, Sel, NSRect),
        );

        CLASS = decl.register();
    });

    unsafe { CLASS }
}

/// Custom `drawRect:` implementation for the dock tile progress indicator.
///
/// Draws a rounded progress bar at the bottom of the dock icon:
/// - Semi-transparent white background track
/// - Blue fill bar proportional to progress
///
/// This is necessary because `NSDockTile.display()` uses image-based compositing —
/// it calls `drawRect:` to capture a bitmap, but the stock `NSProgressIndicator`
/// rendering depends on the window compositor's CALayer tree which doesn't exist
/// for dock tiles.
#[cfg(target_os = "macos")]
unsafe extern "C" fn draw_progress_bar(
    this: *mut objc2::runtime::AnyObject,
    _sel: objc2::runtime::Sel,
    rect: objc2_foundation::NSRect,
) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSInsetRect, NSPoint, NSRect, NSSize};

    // Track bar: 8px tall, positioned 4px from the bottom
    let bar = NSRect::new(
        NSPoint { x: 0.0, y: 4.0 },
        NSSize {
            width: rect.size.width,
            height: 8.0,
        },
    );
    let bar_inner = NSInsetRect(bar, 0.5, 0.5);
    let mut bar_progress = NSInsetRect(bar, 1.0, 1.0);

    // Scale progress width
    let current_progress: f64 = msg_send![this, doubleValue];
    let normalized = (current_progress / 100.0).clamp(0.0, 1.0);
    bar_progress.size.width *= normalized;

    // Draw background track (semi-transparent white)
    let bg_color: *mut AnyObject =
        msg_send![objc2::class!(NSColor), colorWithWhite: 1.0_f64, alpha: 0.05_f64];
    let _: () = msg_send![bg_color, set];
    draw_rounded_rect(bar);
    draw_rounded_rect(bar_inner);

    // Draw progress fill (system blue)
    let fill_color: *mut AnyObject = msg_send![objc2::class!(NSColor), systemBlueColor];
    let _: () = msg_send![fill_color, set];
    draw_rounded_rect(bar_progress);
}

/// Helper: draw a filled rounded rectangle using NSBezierPath.
#[cfg(target_os = "macos")]
unsafe fn draw_rounded_rect(rect: objc2_foundation::NSRect) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    let radius = rect.size.height / 2.0;
    let bezier_path: *mut AnyObject = msg_send![
        objc2::class!(NSBezierPath),
        bezierPathWithRoundedRect: rect,
        xRadius: radius,
        yRadius: radius
    ];
    let _: () = msg_send![bezier_path, fill];
}

/// Applies statistics to native surfaces and remembers what it last set, so
/// unchanged values cause no platform calls.
#[derive(Default)]
pub struct StatEffects {
    awake_guard: Option<PowerGuard>,
    power_retry_at: Option<std::time::Instant>,
    last_tray_title: Option<String>,
    tray_visible: Option<bool>,
    tray_active: Option<bool>,
    last_tray_tooltip: Option<String>,
}

impl StatEffects {
    /// `active` holds the active tasks of the same snapshot as `stat`.
    pub async fn apply(
        &mut self,
        app: &tauri::AppHandle,
        cfg: &RuntimeConfig,
        stat: &StatUpdate,
        active: &[crate::aria2::types::Aria2Task],
    ) {
        self.keep_awake(cfg.keep_awake && stat.num_active > 0, stat.num_active)
            .await;

        let next_title =
            tray_title_for_speed(cfg.tray_speedometer, stat.download_speed, stat.upload_speed);
        if tray_title_needs_update(&self.last_tray_title, &next_title) {
            if let Err(error) = crate::tray::set_title(app, &next_title) {
                log::debug!("tray:title-update-failed error={error}");
            }
            self.last_tray_title = Some(next_title);
        }

        if self.tray_visible != Some(cfg.show_tray_icon) {
            if let Err(error) = crate::tray::set_visible(app, cfg.show_tray_icon) {
                log::debug!("tray:visibility-update-failed error={error}");
            }
            self.tray_visible = Some(cfg.show_tray_icon);
        }
        let downloading = stat.num_active > 0;
        let tooltip = tray_tooltip(stat.num_active, stat.download_speed, stat.upload_speed);
        if self.tray_active != Some(downloading)
            || self.last_tray_tooltip.as_deref() != Some(&tooltip)
        {
            if let Err(error) = crate::tray::set_activity(app, downloading, &tooltip) {
                log::debug!("tray:activity-update-failed error={error}");
            }
            self.tray_active = Some(downloading);
            self.last_tray_tooltip = Some(tooltip);
        }

        // NSDockTile is an app-level API (no window needed) and must be used
        // on the main thread.
        #[cfg(target_os = "macos")]
        {
            let badge_label: Option<String> = (cfg.dock_badge_speed && stat.download_speed > 0)
                .then(|| format!("{}/s", compact_size(stat.download_speed)));
            let progress = (cfg.show_progress_bar && stat.num_active > 0)
                .then(|| task_progress(active))
                .flatten();
            let _ = app.run_on_main_thread(move || {
                set_dock_badge(badge_label.as_deref());
                set_dock_progress(progress);
            });
        }

        #[cfg(target_os = "windows")]
        if let Some(window) = app.get_webview_window("main") {
            use tauri::window::{ProgressBarState, ProgressBarStatus};
            let state = if cfg.show_progress_bar && stat.num_active > 0 {
                let progress = task_progress(active);
                ProgressBarState {
                    status: Some(if progress.is_some() {
                        ProgressBarStatus::Normal
                    } else {
                        ProgressBarStatus::Indeterminate
                    }),
                    progress,
                }
            } else {
                ProgressBarState {
                    status: Some(ProgressBarStatus::None),
                    progress: None,
                }
            };
            let _ = window.set_progress_bar(state);
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let _ = active;
    }

    async fn keep_awake(&mut self, wanted: bool, num_active: u64) {
        if wanted {
            // Linux acquisition fails while the desktop portal is unavailable,
            // so retries are rate-limited.
            let retry_due = self
                .power_retry_at
                .is_none_or(|at| std::time::Instant::now() >= at);
            if self.awake_guard.is_none() && retry_due {
                match PowerGuard::acquire_download().await {
                    Ok(guard) => {
                        let backend = guard.backend_name();
                        self.awake_guard = Some(guard);
                        self.power_retry_at = None;
                        log::info!(
                            "keep_awake: assertion acquired backend={backend} active={num_active}"
                        );
                    }
                    Err(e) => {
                        self.power_retry_at = Some(std::time::Instant::now() + POWER_RETRY_DELAY);
                        log::warn!("keep_awake: failed to acquire assertion: {e}");
                    }
                }
            }
            return;
        }
        self.power_retry_at = None;
        self.release().await;
    }

    /// Releases the keep-awake assertion; called when the pump stops.
    pub async fn release(&mut self) {
        if let Some(guard) = self.awake_guard.take() {
            match guard.release().await {
                Ok(()) => log::info!("keep_awake: assertion released"),
                Err(e) => log::warn!("keep_awake: failed to release assertion: {e}"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aria2::types::{Aria2File, Aria2Task};

    #[test]
    fn tray_tooltip_shows_speeds_only_while_active() {
        assert_eq!(tray_tooltip(0, 0, 0), "Rayburst");
        assert_eq!(
            tray_tooltip(2, 1_572_864, 1024),
            "Rayburst\n↓ 1.5M/s  ↑ 1K/s"
        );
    }

    // ── compact_size ────────────────────────────────────────────────

    #[test]
    fn compact_size_bytes() {
        assert_eq!(compact_size(0), "0B");
        assert_eq!(compact_size(512), "512B");
        assert_eq!(compact_size(1023), "1023B");
    }

    #[test]
    fn compact_size_kilobytes() {
        assert_eq!(compact_size(1024), "1K");
        assert_eq!(compact_size(1536), "2K"); // 1.5K rounds to 2K
        assert_eq!(compact_size(102400), "100K");
    }

    #[test]
    fn compact_size_megabytes() {
        assert_eq!(compact_size(1_048_576), "1.0M");
        assert_eq!(compact_size(1_572_864), "1.5M");
        assert_eq!(compact_size(10_485_760), "10.0M");
    }

    #[test]
    fn compact_size_gigabytes() {
        assert_eq!(compact_size(1_073_741_824), "1.0G");
        assert_eq!(compact_size(2_684_354_560), "2.5G");
    }

    #[test]
    fn tray_title_is_empty_when_speedometer_is_disabled() {
        assert_eq!(tray_title_for_speed(false, 1_048_576, 0), "");
    }

    #[test]
    fn tray_title_updates_only_when_value_changes() {
        let mut last_title: Option<String> = None;
        assert!(tray_title_needs_update(&last_title, ""));
        last_title = Some(String::new());
        assert!(!tray_title_needs_update(&last_title, ""));
        assert!(tray_title_needs_update(&last_title, "↓1.0M"));
    }

    fn make_task(gid: &str, status: &str) -> Aria2Task {
        Aria2Task {
            gid: gid.to_string(),
            status: status.to_string(),
            total_length: "1024".to_string(),
            completed_length: "1024".to_string(),
            dir: "/tmp".to_string(),
            files: vec![Aria2File {
                index: "1".to_string(),
                path: "/tmp/test.zip".to_string(),
                length: "1024".to_string(),
                completed_length: "1024".to_string(),
                selected: "true".to_string(),
                priority: None,
                uris: vec![],
            }],
            ..Aria2Task::default()
        }
    }

    // ── StatUpdate serialization ────────────────────────────────────

    #[test]
    fn stat_update_serializes_to_camel_case() {
        let update = StatUpdate {
            download_speed: 1024,
            upload_speed: 512,
            num_active: 3,
            num_waiting: 1,
            num_stopped: 5,
            num_stopped_total: 10,
        };
        let json = serde_json::to_value(&update).unwrap();
        assert!(json.get("downloadSpeed").is_some());
        assert!(json.get("numActive").is_some());
        assert!(json.get("numStoppedTotal").is_some());
        // Not snake_case
        assert!(json.get("download_speed").is_none());
    }

    #[test]
    fn completed_length_uses_aria2_completed_length() {
        let mut task = make_task("ed2k", "active");
        task.total_length = "1000".to_string();
        task.completed_length = "200".to_string();

        assert_eq!(completed_length(&task), 200);
    }
    #[test]
    fn media_progress_uses_duration_and_keeps_live_or_finalizing_indeterminate() {
        let mut task = crate::aria2::types::Aria2Task {
            total_length: "0".into(),
            completed_length: "0".into(),
            media: Some(crate::aria2::types::Aria2Media {
                live: "false".into(),
                state: "downloading".into(),
                progress: Some("0.5".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(task_progress(&[task.clone()]), Some(50));
        task.media.as_mut().unwrap().live = "true".into();
        assert_eq!(task_progress(&[task.clone()]), None);
        task.media.as_mut().unwrap().live = "false".into();
        task.media.as_mut().unwrap().state = "finalizing".into();
        assert_eq!(task_progress(&[task]), None);
    }
}
