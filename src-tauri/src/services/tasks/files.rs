//! Native file availability and exact task-owned content paths.
use crate::{
    aria2::types::{Aria2File, Aria2Task},
    database::HistoryRecord,
    error::AppError,
};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileState {
    Available,
    Missing,
    Inaccessible,
    Unknown,
}

pub fn content_paths(files: &[Aria2File], selected_only: bool) -> Vec<PathBuf> {
    files
        .iter()
        .filter(|file| !selected_only || file.selected == "true")
        .filter(|file| !file.path.is_empty())
        .map(|file| PathBuf::from(&file.path))
        .filter(|path| path.is_absolute())
        .collect::<HashSet<_>>()
        .into_iter()
        .collect()
}

pub fn history_task(record: &HistoryRecord) -> Aria2Task {
    let meta: serde_json::Value =
        serde_json::from_str(record.meta.as_deref().unwrap_or("{}")).unwrap_or_default();
    Aria2Task {
        gid: record.gid.clone(),
        status: record.status.clone(),
        dir: record.dir.clone().unwrap_or_default(),
        files: meta["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|file| Aria2File {
                path: file["path"].as_str().unwrap_or_default().into(),
                selected: file["selected"].as_str().unwrap_or("true").into(),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

fn availability(paths: &[PathBuf]) -> FileState {
    if paths.is_empty() {
        return FileState::Unknown;
    }
    let mut result = FileState::Available;
    for path in paths {
        match std::fs::File::open(path).and_then(|file| file.metadata()) {
            Ok(metadata) if metadata.is_file() => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                result = FileState::Missing
            }
            _ => return FileState::Inaccessible,
        }
    }
    result
}

struct Entry {
    paths: Vec<PathBuf>,
    state: FileState,
    seen: Instant,
    recheck: bool,
}
pub struct FileMonitor {
    watcher: Option<RecommendedWatcher>,
    dirty: Arc<AtomicBool>,
    roots: HashSet<PathBuf>,
    entries: HashMap<String, Entry>,
    visible: HashSet<String>,
    checked: Instant,
}
impl Default for FileMonitor {
    fn default() -> Self {
        let dirty = Arc::new(AtomicBool::new(true));
        let changed = dirty.clone();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            use notify::{event::ModifyKind, EventKind};
            // Ordinary writes in a shared download directory do not invalidate
            // every completed file. Periodic checks also cover permission changes
            // on platforms that report them as an unspecified modification.
            if !matches!(&event, Ok(event) if matches!(event.kind,
                EventKind::Access(_) | EventKind::Modify(ModifyKind::Data(_) | ModifyKind::Any | ModifyKind::Other))) {
                changed.store(true, Ordering::Release);
            }
        })
        .map_err(|error| {
            log::warn!("task_files: native watcher unavailable; using periodic checks: {error}")
        })
        .ok();
        Self {
            watcher,
            dirty,
            roots: HashSet::new(),
            entries: HashMap::new(),
            visible: HashSet::new(),
            checked: Instant::now(),
        }
    }
}
impl FileMonitor {
    pub fn inspect(&mut self, tasks: &[Aria2Task], force: bool) -> HashMap<String, FileState> {
        let now = Instant::now();
        if force {
            self.visible = tasks.iter().map(|task| task.gid.clone()).collect();
        }
        let changed = self.dirty.swap(false, Ordering::AcqRel);
        let mut dirty = force || changed || self.checked.elapsed() >= Duration::from_secs(30);
        for task in tasks {
            let complete = task.status == "complete"
                || task.seeder.as_deref() == Some("true")
                || (task.status == "paused" && self.entries.contains_key(&task.gid));
            if !complete {
                self.entries.remove(&task.gid);
                continue;
            }
            let mut paths = content_paths(&task.files, true);
            paths.sort();
            let entry = self.entries.entry(task.gid.clone()).or_insert_with(|| {
                dirty = true;
                Entry {
                    paths: paths.clone(),
                    state: FileState::Unknown,
                    seen: now,
                    recheck: false,
                }
            });
            if entry.paths != paths {
                entry.paths = paths;
                dirty = true;
            }
            entry.seen = now;
        }
        self.entries.retain(|gid, entry| {
            self.visible.contains(gid) || now.duration_since(entry.seen) < Duration::from_secs(300)
        });
        if dirty {
            for entry in self.entries.values_mut() {
                entry.state = availability(&entry.paths);
                entry.recheck |=
                    matches!(entry.state, FileState::Missing | FileState::Inaccessible);
            }
            self.checked = now;
            let roots: HashSet<_> = self
                .entries
                .values()
                .flat_map(|entry| &entry.paths)
                .filter_map(|path| {
                    path.parent()?
                        .ancestors()
                        .find(|parent| parent.is_dir())
                        .map(Path::to_path_buf)
                })
                .collect();
            if let Some(watcher) = &mut self.watcher {
                for root in self.roots.difference(&roots) {
                    let _ = watcher.unwatch(root);
                }
                let mut watching: HashSet<_> = self.roots.intersection(&roots).cloned().collect();
                for root in roots.difference(&self.roots) {
                    match watcher.watch(root, RecursiveMode::NonRecursive) {
                        Ok(()) => {
                            watching.insert(root.clone());
                        }
                        Err(error) => log::debug!(
                            "task_files: watch unavailable; periodic checks remain active: {error}"
                        ),
                    }
                }
                self.roots = watching;
            }
        }
        self.entries
            .iter()
            .map(|(gid, entry)| (gid.clone(), entry.state))
            .collect()
    }

    pub fn needs_recheck(&self, gid: &str) -> bool {
        self.entries.get(gid).is_some_and(|entry| entry.recheck)
    }
    pub fn rechecked(&mut self, gid: &str) {
        if let Some(entry) = self.entries.get_mut(gid) {
            entry.recheck = false;
        }
    }
}

/// Compare resolved file identities, including case-insensitive Windows paths.
pub fn path_identity(path: &Path) -> PathBuf {
    let resolved = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    #[cfg(windows)]
    {
        PathBuf::from(resolved.to_string_lossy().to_lowercase())
    }
    #[cfg(not(windows))]
    {
        resolved
    }
}

pub fn delete_content(
    paths: &[PathBuf],
    protected: &HashSet<PathBuf>,
    root: &Path,
    protected_directories: &HashSet<PathBuf>,
    mode: crate::commands::fs::FileDeletionMode,
) -> Result<(), AppError> {
    let directories = cleanup_directories(paths, root, protected_directories)?;
    let mut failure = None;
    // Keep unrelated and shared content, even when it shares a task directory.
    for path in paths {
        if protected.contains(&path_identity(path)) {
            continue;
        }
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                failure.get_or_insert_with(|| AppError::from(error));
                continue;
            }
        };
        if metadata.is_dir() {
            failure.get_or_insert(AppError::InvalidInput(
                "Task content must refer to individual files".into(),
            ));
            continue;
        }
        if let Err(error) =
            crate::commands::fs::delete_path(path.to_string_lossy().into_owned(), mode)
        {
            failure.get_or_insert(error);
        }
    }
    for directory in directories {
        // Recheck the route after file deletion; never walk a replaced link.
        if cleanup_branch(&directory, root, protected_directories)?.first() != Some(&directory) {
            continue;
        }
        if let Err(error) = std::fs::remove_dir(&directory) {
            if !matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) {
                failure.get_or_insert_with(|| {
                    AppError::Io(format!(
                        "Failed to remove empty directory {}: {error}",
                        directory.display()
                    ))
                });
            }
        }
    }
    failure.map_or(Ok(()), Err)
}

fn cleanup_directories(
    paths: &[PathBuf],
    root: &Path,
    protected: &HashSet<PathBuf>,
) -> Result<Vec<PathBuf>, AppError> {
    let mut directories: Vec<_> = paths
        .iter()
        .filter_map(|path| path.parent())
        .map(|directory| cleanup_branch(directory, root, protected))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    Ok(directories)
}

fn cleanup_branch(
    directory: &Path,
    root: &Path,
    protected: &HashSet<PathBuf>,
) -> Result<Vec<PathBuf>, AppError> {
    use std::path::Component;
    if !root.is_absolute()
        || !directory.is_absolute()
        || root
            .components()
            .chain(directory.components())
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Ok(Vec::new());
    }
    let root = path_identity(root);
    let mut branch = Vec::new();
    for parent in directory.ancestors() {
        let identity = path_identity(parent);
        if identity == root {
            return Ok(branch);
        }
        match std::fs::symlink_metadata(parent) {
            Ok(metadata) => {
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    // Do not traverse junctions or other directory reparse points.
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Ok(Vec::new());
                    }
                }
                if !metadata.is_dir() || metadata.file_type().is_symlink() {
                    return Ok(Vec::new());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        if !protected.contains(&identity) {
            branch.push(parent.to_path_buf());
        }
    }
    Ok(Vec::new())
}

pub async fn inspect(
    engine: &super::TaskService,
    tasks: Vec<Aria2Task>,
    force: bool,
) -> Result<HashMap<String, FileState>, AppError> {
    let monitor = engine.files.clone();
    tokio::task::spawn_blocking(move || {
        monitor
            .lock()
            .map_err(|_| AppError::Io("File monitor lock poisoned".into()))
            .map(|mut monitor| monitor.inspect(&tasks, force))
    })
    .await
    .map_err(|error| AppError::Io(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn deletion_prunes_only_empty_task_directories_and_can_retry_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("torrent/sub/file.bin");
        let shared = dir.path().join("shared/file.bin");
        let extra = dir.path().join("keep/.hidden");
        let owned = dir.path().join("keep/file.bin");
        let category = dir.path().join("category");
        let category_file = category.join("gone.bin");
        for path in [&nested, &shared, &extra, &owned, &category_file] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"content").unwrap();
        }
        let paths = vec![nested, shared.clone(), owned, category_file];
        let protected = HashSet::from([path_identity(&shared)]);
        let roots = HashSet::from([path_identity(&category)]);
        for _ in 0..2 {
            delete_content(
                &paths,
                &protected,
                dir.path(),
                &roots,
                crate::commands::fs::FileDeletionMode::Permanent,
            )
            .unwrap();
        }
        assert!(!dir.path().join("torrent").exists());
        assert!(shared.is_file());
        assert!(extra.is_file());
        assert!(category.is_dir());
        assert!(dir.path().is_dir());

        let outside = tempfile::tempdir().unwrap();
        assert!(
            cleanup_directories(&[outside.path().join("absent")], dir.path(), &roots)
                .unwrap()
                .is_empty()
        );
        let missing = dir.path().join("missing/file.bin");
        std::fs::create_dir(missing.parent().unwrap()).unwrap();
        delete_content(
            &[missing],
            &protected,
            dir.path(),
            &roots,
            crate::commands::fs::FileDeletionMode::Permanent,
        )
        .unwrap();
        assert!(!dir.path().join("missing").exists());
    }

    #[test]
    fn missing_selected_files_and_shared_content_keep_distinct_ownership() {
        let dir = tempfile::tempdir().unwrap();
        let shared = dir.path().join("shared.bin");
        let owned = dir.path().join("owned.bin");
        let unrelated = dir.path().join("keep.bin");
        for path in [&shared, &owned, &unrelated] {
            std::fs::write(path, b"data").unwrap();
        }
        let paths = vec![shared.clone(), owned.clone()];
        assert_eq!(availability(&paths), FileState::Available);
        delete_content(
            &paths,
            &HashSet::from([path_identity(&shared)]),
            dir.path(),
            &HashSet::new(),
            crate::commands::fs::FileDeletionMode::Permanent,
        )
        .unwrap();
        assert!(shared.exists());
        assert!(unrelated.exists());
        assert!(!owned.exists());
        assert_eq!(availability(&paths), FileState::Missing);
        std::fs::write(&owned, b"restored").unwrap();
        assert_eq!(availability(&paths), FileState::Available);
        assert!(delete_content(
            &[dir.path().to_path_buf()],
            &HashSet::new(),
            dir.path(),
            &HashSet::new(),
            crate::commands::fs::FileDeletionMode::Permanent
        )
        .is_err());
        let record: HistoryRecord = serde_json::from_value(serde_json::json!({"gid":"g", "name":"files", "status":"complete", "meta":serde_json::json!({"files":[{"path":shared,"selected":"true","uris":["https://example.test/file"]}]}).to_string()})).unwrap();
        assert_eq!(
            content_paths(&history_task(&record).files, true),
            vec![shared]
        );
    }
}
