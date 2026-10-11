//! Stops Rayburst processes running from an installation directory before the
//! installer replaces its files.
//!
//! Processes are matched by their full image path through process handles; no
//! `taskkill` or other shell command is involved.

use std::ffi::OsString;
use std::mem::size_of;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
};

const PROCESS_EXIT_TIMEOUT_MS: u32 = 5_000;
const MAX_WINDOWS_PATH_CHARS: usize = 32_768;

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: OwnedHandle is created only from a non-null OpenProcess result
        // and is never duplicated, so this is the single matching close.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn process_path(handle: HANDLE) -> Result<PathBuf, String> {
    let mut buffer = vec![0_u16; MAX_WINDOWS_PATH_CHARS];
    let mut len = buffer.len() as u32;
    // SAFETY: handle is owned and valid, and buffer/len describe writable
    // storage for the UTF-16 path exactly as required by the API.
    let succeeded = unsafe { QueryFullProcessImageNameW(handle, 0, buffer.as_mut_ptr(), &mut len) };
    if succeeded == 0 {
        return Err(format!(
            "QueryFullProcessImageNameW failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    buffer.truncate(len as usize);
    Ok(PathBuf::from(OsString::from_wide(&buffer)))
}

fn normalized_path(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| dunce::simplified(path).to_path_buf())
}

fn paths_match(actual: &Path, expected: &Path) -> bool {
    normalized_path(actual)
        .to_string_lossy()
        .eq_ignore_ascii_case(&normalized_path(expected).to_string_lossy())
}

fn terminate_process_at_path(
    pid: u32,
    expected_path: &Path,
    grace_ms: u32,
) -> Result<bool, String> {
    let access = PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE;
    // SAFETY: OpenProcess takes value parameters only and returns an owned
    // handle, which is immediately wrapped for deterministic cleanup.
    let handle = unsafe { OpenProcess(access, 0, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(87) {
            return Ok(false); // The process exited after enumeration.
        }
        return Err(format!("OpenProcess failed for PID {pid}: {error}"));
    }
    let handle = OwnedHandle(handle);
    // A process whose image cannot be read (for example while it is exiting)
    // is not provably ours, so it is left alone.
    let Ok(actual_path) = process_path(handle.0) else {
        return Ok(false);
    };
    if !paths_match(&actual_path, expected_path) {
        return Ok(false);
    }

    // SAFETY: handle remains valid for the complete wait/terminate sequence.
    if unsafe { WaitForSingleObject(handle.0, grace_ms) } == WAIT_OBJECT_0 {
        return Ok(false);
    }
    // SAFETY: The full executable path was verified on this exact process
    // handle, avoiding PID-reuse and unrelated-process termination races.
    if unsafe { TerminateProcess(handle.0, 1) } == 0 {
        let error = std::io::Error::last_os_error();
        // Exit can race the terminate call; Windows reports access denied for
        // an already terminated process. The owned handle identifies that process.
        if unsafe { WaitForSingleObject(handle.0, 0) } == WAIT_OBJECT_0 {
            return Ok(false);
        }
        return Err(format!("TerminateProcess failed for PID {pid}: {error}"));
    }
    // SAFETY: handle remains owned and valid until this function returns.
    match unsafe { WaitForSingleObject(handle.0, PROCESS_EXIT_TIMEOUT_MS) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Err(format!(
            "Timed out waiting for engine PID {pid} to terminate"
        )),
        status => Err(format!(
            "WaitForSingleObject failed for engine PID {pid} with status {status}"
        )),
    }
}

/// Run from the installer's temporary copy, before replacing installed files.
/// Stop the supervisor first so it cannot restart a sidecar during installation.
pub fn prepare_install(directory: &Path) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    if !directory.is_absolute() {
        return Err("Installation directory must be absolute".into());
    }
    for name in [
        "rayburst.exe",
        "aria2-next.exe",
        "rayburst-browser-launcher.exe",
    ] {
        // SAFETY: Snapshot handles are owned and closed exactly once below.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let snapshot = OwnedHandle(snapshot);
        // SAFETY: PROCESSENTRY32W contains integers and a UTF-16 array; zero is valid for every field.
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        // SAFETY: entry is correctly sized writable storage and snapshot is live.
        let mut found = unsafe { Process32FirstW(snapshot.0, &mut entry) };
        while found != 0 {
            let end = entry
                .szExeFile
                .iter()
                .position(|ch| *ch == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..end]).eq_ignore_ascii_case(name) {
                terminate_process_at_path(
                    entry.th32ProcessID,
                    &directory.join(name),
                    if name == "aria2-next.exe" {
                        PROCESS_EXIT_TIMEOUT_MS
                    } else {
                        0
                    },
                )?;
            }
            // SAFETY: Same live snapshot and initialized output structure.
            found = unsafe { Process32NextW(snapshot.0, &mut entry) };
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
            return Err(format!("Process enumeration failed: {error}"));
        }
    }
    crate::association_cleanup::cleanup(directory)
        .map_err(|error| format!("Association cleanup failed: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_matching_is_case_insensitive_but_requires_the_full_path() {
        let expected = Path::new(r"C:\Program Files\Rayburst\aria2-next.exe");
        assert!(paths_match(
            Path::new(r"c:\program files\rayburst\ARIA2-NEXT.EXE"),
            expected
        ));
        assert!(!paths_match(
            Path::new(r"C:\Other\aria2-next.exe"),
            expected
        ));
    }
}
