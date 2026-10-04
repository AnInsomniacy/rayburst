//! Native StatusNotifierItem activation, menus and panel reconnection.
use ksni::TrayMethods;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use tauri::AppHandle;

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
    labels: HashMap<String, String>,
}

struct NativeTray {
    app: AppHandle,
    model: Arc<Mutex<Model>>,
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
        ksni::ToolTip {
            title: self.title(),
            ..Default::default()
        }
    }
    fn activate(&mut self, _: i32, _: i32) {
        super::request_main_window(&self.app, "tray-activate", true);
    }
    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        log::warn!("tray:watcher-offline reason={reason:?}");
        super::request_main_window(&self.app, "tray-watcher-offline", true);
        true
    }
    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        let image = super::tray_icon_image();
        let data = image
            .rgba()
            .chunks_exact(4)
            .flat_map(|p| [p[3], p[0], p[1], p[2]])
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
        ACTIONS
            .iter()
            .map(|&(id, fallback)| {
                ksni::menu::StandardItem {
                    label: model
                        .labels
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| fallback.into()),
                    activate: Box::new(move |tray: &mut Self| {
                        super::dispatch_menu_action(&tray.app, id)
                    }),
                    ..Default::default()
                }
                .into()
            })
            .collect()
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
        model: Arc::default(),
        handle: Arc::default(),
    };
    let tray = NativeTray {
        app: app.clone(),
        model: state.model.clone(),
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
