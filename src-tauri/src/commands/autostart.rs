//! Start at login, registered under the product name with `--autostart`.
//!
//! Written here rather than through `tauri-plugin-autostart`: its `auto-launch`
//! backend joins the executable and its arguments without quoting, so an
//! install path with spaces reached Windows as `D:\Program` (#716), and every
//! `enable()` also cleared the user's "disabled" choice in Task Manager.
//!
//! [`set_autostart`] applies a choice made in the settings. [`sync_autostart`]
//! runs at every launch: it repairs a missing or outdated registration (a
//! moved install, an unquoted path from an older version) but leaves the
//! system's own on/off switch alone.

use crate::error::AppError;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

const ARGUMENT: &str = "--autostart";

/// Applies the user's choice in the settings.
#[tauri::command]
pub async fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    apply(&app, enabled, true)
}

/// Keeps the registration in line with the preference at launch.
#[tauri::command]
pub async fn sync_autostart(app: AppHandle, enabled: bool) -> Result<(), AppError> {
    apply(&app, enabled, false)
}

fn apply(app: &AppHandle, enabled: bool, chosen: bool) -> Result<(), AppError> {
    let name = app.package_info().name.clone();
    let result = if enabled {
        executable(app).and_then(|path| platform::register(&name, &path, chosen))
    } else {
        platform::unregister(&name)
    };
    result.map_err(|error| AppError::Io(format!("autostart: {error}")))
}

fn executable(app: &AppHandle) -> std::io::Result<PathBuf> {
    // An AppImage runs from a temporary mount; login must start the image.
    #[cfg(target_os = "linux")]
    if let Some(appimage) = tauri::Manager::env(app).appimage {
        return Ok(PathBuf::from(appimage));
    }
    let _ = app;
    let path = tauri::utils::platform::current_exe()?;
    // macOS: through symlinks, so login finds the bundle.
    #[cfg(target_os = "macos")]
    let path = path.canonicalize()?;
    Ok(path)
}

/// Rewrites `path` only when its content differs.
#[cfg(not(windows))]
fn write_if_changed(path: &Path, content: &str) -> std::io::Result<()> {
    if std::fs::read_to_string(path).is_ok_and(|current| current == content) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)
}

#[cfg(not(windows))]
fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

/// The Run value: the executable quoted, as Windows splits an unquoted command
/// line at the first space that leaves a file name it can open, and without
/// the `\\?\` prefix of a canonical path, which Explorer cannot start (#716).
#[cfg_attr(not(windows), allow(dead_code))]
fn windows_command(executable: &Path) -> String {
    format!("\"{}\" {ARGUMENT}", dunce::simplified(executable).display())
}

/// One `Exec` argument of a desktop entry: quoted when it holds a reserved
/// character, with the quoting escapes and then the string escapes applied
/// (Desktop Entry Specification, "The Exec key").
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn desktop_exec_argument(value: &str) -> String {
    const RESERVED: &[char] = &[
        ' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(',
        ')', '`',
    ];
    let value = value.replace('%', "%%");
    if !value.contains(RESERVED) {
        return value;
    }
    let mut quoted = String::from("\"");
    for character in value.chars() {
        match character {
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(character);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn desktop_entry(name: &str, executable: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nVersion=1.0\nName={name}\nExec={} {ARGUMENT}\n\
         StartupNotify=false\nTerminal=false\n",
        desktop_exec_argument(&executable.to_string_lossy())
    )
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn launch_agent(name: &str, executable: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
         \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n<dict>\n\
         \x20 <key>Label</key>\n  <string>{}</string>\n\
         \x20 <key>ProgramArguments</key>\n  <array>\n    <string>{}</string>\n    <string>{ARGUMENT}</string>\n  </array>\n\
         \x20 <key>RunAtLoad</key>\n  <true/>\n</dict>\n</plist>\n",
        xml_text(name),
        xml_text(&executable.to_string_lossy())
    )
}

#[cfg(windows)]
mod platform {
    use std::io;
    use std::path::Path;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};
    use winreg::RegKey;

    // Per user only: login items never need administrator rights.
    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    // Task Manager's on/off switch for Run entries; no value means enabled.
    const APPROVED: &str =
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    pub fn register(name: &str, executable: &Path, chosen: bool) -> io::Result<()> {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let (run, _) = user.create_subkey(RUN)?;
        let command = super::windows_command(executable);
        if run.get_value::<String, _>(name).ok().as_deref() != Some(command.as_str()) {
            run.set_value(name, &command)?;
        }
        if chosen {
            delete(&user, APPROVED, name)?;
        }
        Ok(())
    }

    pub fn unregister(name: &str) -> io::Result<()> {
        let user = RegKey::predef(HKEY_CURRENT_USER);
        delete(&user, RUN, name)?;
        delete(&user, APPROVED, name)
    }

    fn delete(user: &RegKey, key: &str, name: &str) -> io::Result<()> {
        let key = match user.open_subkey_with_flags(key, KEY_SET_VALUE) {
            Ok(key) => key,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        match key.delete_value(name) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::io;
    use std::path::{Path, PathBuf};

    fn agent(name: &str) -> io::Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| io::Error::other("no home directory"))?;
        Ok(home
            .join("Library/LaunchAgents")
            .join(format!("{name}.plist")))
    }

    // The system's own switch (Login Items settings) lives outside the file.
    pub fn register(name: &str, executable: &Path, _chosen: bool) -> io::Result<()> {
        super::write_if_changed(&agent(name)?, &super::launch_agent(name, executable))
    }

    pub fn unregister(name: &str) -> io::Result<()> {
        super::remove_if_present(&agent(name)?)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::io;
    use std::path::{Path, PathBuf};

    /// `$XDG_CONFIG_HOME/autostart`, and `~/.config/autostart` where earlier
    /// versions wrote regardless of it.
    fn entries(name: &str) -> Vec<PathBuf> {
        let file = format!("{name}.desktop");
        let mut paths: Vec<PathBuf> = dirs::config_dir()
            .into_iter()
            .chain(dirs::home_dir().map(|home| home.join(".config")))
            .map(|dir| dir.join("autostart").join(&file))
            .collect();
        paths.dedup();
        paths
    }

    pub fn register(name: &str, executable: &Path, _chosen: bool) -> io::Result<()> {
        let paths = entries(name);
        let primary = paths
            .first()
            .ok_or_else(|| io::Error::other("no config directory"))?;
        super::write_if_changed(primary, &super::desktop_entry(name, executable))
    }

    pub fn unregister(name: &str) -> io::Result<()> {
        entries(name)
            .iter()
            .try_for_each(|path| super::remove_if_present(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_command_quotes_the_executable() {
        assert_eq!(
            windows_command(Path::new(r"D:\Program Files\Rayburst\rayburst.exe")),
            r#""D:\Program Files\Rayburst\rayburst.exe" --autostart"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn windows_command_drops_the_verbatim_prefix() {
        assert_eq!(
            windows_command(Path::new(r"\\?\D:\Program Files\Rayburst\rayburst.exe")),
            r#""D:\Program Files\Rayburst\rayburst.exe" --autostart"#
        );
    }

    #[test]
    fn desktop_exec_leaves_plain_paths_alone() {
        assert_eq!(
            desktop_exec_argument("/opt/Rayburst/rayburst.AppImage"),
            "/opt/Rayburst/rayburst.AppImage"
        );
    }

    #[test]
    fn desktop_exec_quotes_and_escapes_reserved_characters() {
        assert_eq!(
            desktop_exec_argument("/home/a b/Ray$burst"),
            r#""/home/a b/Ray\\$burst""#
        );
        assert_eq!(desktop_exec_argument(r"/x\y"), r#""/x\\\\y""#);
        assert_eq!(desktop_exec_argument("/100%/app"), "/100%%/app");
    }

    #[test]
    fn desktop_entry_starts_the_quoted_executable() {
        let entry = desktop_entry("Rayburst", Path::new("/home/me/My Apps/Rayburst.AppImage"));
        assert!(entry.contains("\nExec=\"/home/me/My Apps/Rayburst.AppImage\" --autostart\n"));
    }

    #[test]
    fn launch_agent_escapes_xml() {
        let plist = launch_agent(
            "Rayburst",
            Path::new("/Applications/R&D/Rayburst.app/Contents/MacOS/rayburst"),
        );
        assert!(plist.contains(
            "<string>/Applications/R&amp;D/Rayburst.app/Contents/MacOS/rayburst</string>"
        ));
        assert!(plist.contains("<string>--autostart</string>"));
    }
}
