//! Native StatusNotifierItem activation, menus and panel reconnection.
use ksni::TrayMethods;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::AppHandle;

use super::FloatMode;

const ACTIONS: [(&str, &str); 5] = [
    ("show", "Show Rayburst"),
    ("tray-new-task", "New Task"),
    ("tray-resume-all", "Resume All"),
    ("tray-pause-all", "Pause All"),
    ("tray-quit", "Quit"),
];

#[derive(Default)]
struct Model {
    title: String,
    tooltip: String,
    active: bool,
    hidden: bool,
    labels: HashMap<String, String>,
    float_mode: FloatMode,
}

/// A panel restarting, after resume or unlock, brings the watcher back within
/// moments; only a tray that stays gone leaves the window as the way back in.
const WATCHER_GRACE: Duration = Duration::from_secs(10);

struct NativeTray {
    app: AppHandle,
    model: Arc<Mutex<Model>>,
    /// Counts watcher changes, so a return cancels a pending window.
    watcher: Arc<AtomicU64>,
}

impl ksni::Tray for NativeTray {
    fn id(&self) -> String {
        crate::APP_ID.into()
    }
    fn title(&self) -> String {
        let model = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if model.title.is_empty() {
            "Rayburst".into()
        } else {
            model.title.clone()
        }
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        let description = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .tooltip
            .clone();
        ksni::ToolTip {
            title: self.title(),
            description,
            ..Default::default()
        }
    }
    fn status(&self) -> ksni::Status {
        let hidden = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .hidden;
        // Hosts leave passive items out of the panel.
        if hidden {
            ksni::Status::Passive
        } else {
            ksni::Status::Active
        }
    }
    fn activate(&mut self, _: i32, _: i32) {
        super::request_main_window(&self.app, "tray-activate", true);
    }
    fn watcher_online(&self) {
        self.watcher.fetch_add(1, Ordering::SeqCst);
        log::info!("tray:watcher-online");
    }
    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        log::warn!("tray:watcher-offline reason={reason:?}");
        let change = self.watcher.fetch_add(1, Ordering::SeqCst) + 1;
        let (app, watcher) = (self.app.clone(), self.watcher.clone());
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(WATCHER_GRACE).await;
            if watcher.load(Ordering::SeqCst) == change {
                super::request_main_window(&app, "tray-watcher-offline", true);
            }
        });
        true
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let active = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .active;
        let image = super::tray_icon_image_with_activity(active);
        let data = image
            .rgba()
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[r, g, b, a]| [a, r, g, b])
            .collect();
        vec![ksni::Icon {
            width: image.width() as i32,
            height: image.height() as i32,
            data,
        }]
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let model = self
            .model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let label = |id: &str, fallback: &str| {
            model
                .labels
                .get(id)
                .cloned()
                .unwrap_or_else(|| fallback.into())
        };
        let mut menu: Vec<ksni::MenuItem<Self>> = ACTIONS
            .iter()
            .map(|&(id, fallback)| {
                ksni::menu::StandardItem {
                    label: label(id, fallback),
                    activate: Box::new(move |tray: &mut Self| {
                        super::dispatch_menu_action(&tray.app, id)
                    }),
                    ..Default::default()
                }
                .into()
            })
            .collect();
        let float_menu = ksni::menu::SubMenu {
            label: label("tray-float", super::FLOAT_MENU_LABEL),
            submenu: vec![ksni::menu::RadioGroup {
                selected: FloatMode::ALL
                    .iter()
                    .position(|mode| *mode == model.float_mode)
                    .unwrap_or_default(),
                select: Box::new(|tray: &mut Self, index| {
                    let Some(&mode) = FloatMode::ALL.get(index) else {
                        return;
                    };
                    tray.model
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .float_mode = mode;
                    if let Err(error) = crate::services::float_window::set_mode(&tray.app, mode) {
                        log::warn!("tray:float-mode-failed error={error}");
                    }
                }),
                options: FloatMode::ALL
                    .iter()
                    .map(|&mode| ksni::menu::RadioItem {
                        label: label(mode.menu_id(), super::float_mode_label(mode)),
                        ..Default::default()
                    })
                    .collect(),
            }
            .into()],
            ..Default::default()
        };
        // Before Quit, as in the other platforms' menus.
        menu.insert(menu.len() - 1, float_menu.into());
        menu
    }
}

pub struct TrayMenuState {
    model: Arc<Mutex<Model>>,
    handle: Arc<Mutex<Option<ksni::Handle<NativeTray>>>>,
}

impl TrayMenuState {
    fn refresh(&self) {
        let handle = self
            .handle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        if let Some(handle) = handle {
            tauri::async_runtime::spawn(async move {
                let _ = handle.update(|_| {}).await;
            });
        }
    }
    pub fn set_title(&self, title: &str) {
        self.model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .title = title.into();
        self.refresh();
    }
    pub fn set_visible(&self, visible: bool) {
        self.model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .hidden = !visible;
        self.refresh();
    }
    pub fn set_activity(&self, active: bool, tooltip: &str) {
        {
            let mut model = self
                .model
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            model.active = active;
            model.tooltip = tooltip.into();
        }
        self.refresh();
    }
    pub fn set_float_mode(&self, mode: FloatMode) {
        self.model
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .float_mode = mode;
        self.refresh();
    }
    pub fn set_labels(&self, labels: &serde_json::Value) {
        if let Some(labels) = labels.as_object() {
            let mut model = self
                .model
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for (id, text) in labels {
                if let Some(text) = text.as_str() {
                    model.labels.insert(id.clone(), text.into());
                }
            }
        }
        self.refresh();
    }
}

pub fn setup_tray(app: &AppHandle) -> Result<TrayMenuState, Box<dyn std::error::Error>> {
    let state = TrayMenuState {
        model: Arc::new(Mutex::new(Model {
            hidden: !crate::read_pref_bool(app, "showTrayIcon", true),
            float_mode: crate::services::float_window::saved_mode(app),
            ..Model::default()
        })),
        handle: Arc::default(),
    };
    let tray = NativeTray {
        app: app.clone(),
        model: state.model.clone(),
        watcher: Arc::default(),
    };
    let destination = state.handle.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match tray.assume_sni_available(true).spawn().await {
            Ok(handle) => {
                *destination
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(handle);
            }
            Err(error) => {
                log::error!("tray:registration-failed error={error}");
                super::request_main_window(&app, "tray-unavailable", true);
            }
        }
    });
    Ok(state)
}
