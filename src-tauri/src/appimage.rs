//! A program running as an AppImage, such as a browser, exports `APPIMAGE` and
//! `APPDIR` to everything it starts. Rayburst started by it, from a package
//! install, must not take them for its own: associations, the native messaging
//! launcher, login items and updates would all target the other program
//! (rayburst-connect#106).

#[cfg(any(target_os = "linux", test))]
use std::{ffi::OsStr, path::Path};

/// Variables the AppImage runtime exports to the program it mounted.
#[cfg(target_os = "linux")]
const RUNTIME_VARIABLES: [&str; 2] = ["APPIMAGE", "APPDIR"];

/// Whether `executable` runs from inside the AppImage mount `appdir`.
#[cfg(any(target_os = "linux", test))]
fn runs_from(executable: &Path, appdir: Option<&OsStr>) -> bool {
    appdir.filter(|dir| !dir.is_empty()).is_some_and(|dir| {
        let dir = Path::new(dir);
        executable.starts_with(dir)
            || dir
                .canonicalize()
                .is_ok_and(|dir| executable.starts_with(dir))
    })
}

/// Drops AppImage variables this process inherited instead of being mounted
/// by. Must run before any thread starts.
pub fn prepare() {
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("APPIMAGE").is_none() && std::env::var_os("APPDIR").is_none() {
            return;
        }
        let Ok(executable) = std::env::current_exe() else {
            return;
        };
        if runs_from(&executable, std::env::var_os("APPDIR").as_deref()) {
            return;
        }
        // SAFETY: called first in `run`, before Tauri starts any thread.
        unsafe {
            for name in RUNTIME_VARIABLES {
                std::env::remove_var(name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::runs_from;
    use std::ffi::OsStr;
    use std::path::Path;

    #[test]
    fn only_a_mounted_executable_owns_the_appimage_variables() {
        let mount = OsStr::new("/tmp/.mount_RayburXYZ");
        assert!(runs_from(
            Path::new("/tmp/.mount_RayburXYZ/usr/bin/rayburst"),
            Some(mount)
        ));
        // A package install started by a browser AppImage.
        assert!(!runs_from(
            Path::new("/usr/bin/rayburst"),
            Some(OsStr::new("/tmp/.mount_heliumAB"))
        ));
        assert!(!runs_from(Path::new("/usr/bin/rayburst"), None));
        assert!(!runs_from(
            Path::new("/usr/bin/rayburst"),
            Some(OsStr::new(""))
        ));
        // A sibling mount whose name merely starts the same way.
        assert!(!runs_from(
            Path::new("/tmp/.mount_RayburXYZ2/usr/bin/rayburst"),
            Some(mount)
        ));
    }
}
