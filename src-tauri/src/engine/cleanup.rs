use std::path::{Path, PathBuf};

use tauri::Manager;

const DOWNLOAD_SESSION_FILE: &str = "download.session";
const ENGINE_STATE_DIR: &str = "state";

fn engine_runtime_paths(data_dir: &Path) -> [PathBuf; 2] {
    [
        data_dir.join(DOWNLOAD_SESSION_FILE),
        data_dir.join("engine").join(ENGINE_STATE_DIR),
    ]
}

fn remove_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
    .map_err(|error| format!("Failed to remove {}: {error}", path.display()))
}

fn clear_runtime_paths(data_dir: &Path) -> Result<(), String> {
    for path in engine_runtime_paths(data_dir) {
        remove_path(&path)?;
    }
    Ok(())
}

pub(crate) fn clear_engine_runtime_state(app: &tauri::AppHandle) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("Failed to get app data directory: {error}"))?;
    clear_runtime_paths(&data_dir)?;
    log::info!("engine: cleared resumable runtime state");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_runtime_fixture(data_dir: &Path) {
        let state_dir = data_dir.join("engine").join(ENGINE_STATE_DIR);
        std::fs::create_dir_all(state_dir.join("bittorrent").join("torrents")).unwrap();
        std::fs::create_dir_all(state_dir.join("ed2k")).unwrap();
        std::fs::write(data_dir.join(DOWNLOAD_SESSION_FILE), "session").unwrap();
        std::fs::write(state_dir.join("bittorrent").join("session"), "bt-state").unwrap();
        std::fs::write(
            state_dir
                .join("bittorrent")
                .join("torrents")
                .join("resume.dat"),
            "resume",
        )
        .unwrap();
        std::fs::write(state_dir.join("ed2k").join("state.db"), "ed2k-state").unwrap();
    }

    #[test]
    fn clear_runtime_paths_removes_only_resumable_engine_state() {
        let temp = tempfile::tempdir().unwrap();
        create_runtime_fixture(temp.path());
        std::fs::write(temp.path().join("history.db"), "history").unwrap();
        std::fs::write(temp.path().join("config.json"), "settings").unwrap();
        clear_runtime_paths(temp.path()).unwrap();

        for path in engine_runtime_paths(temp.path()) {
            assert!(!path.exists());
        }
        assert!(temp.path().join("history.db").exists());
        assert!(temp.path().join("config.json").exists());
    }
}
