//! Shared classification for previews, manual tasks and browser submissions.
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub label: String,
    pub directory: String,
    pub directory_mode: DirectoryMode,
    pub extensions: Vec<String>,
    #[serde(default)]
    pub url_patterns: Vec<String>,
    #[serde(default = "wildcard")]
    pub url_pattern_mode: String,
    #[serde(default)]
    pub built_in: bool,
}

fn wildcard() -> String {
    "wildcard".into()
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DirectoryMode {
    Relative,
    Absolute,
}

#[derive(Deserialize)]
pub struct Candidate {
    pub path: String,
    #[serde(default)]
    pub urls: Vec<String>,
}

pub fn directory(base: &str, category: &Category) -> Result<String, AppError> {
    let path = Path::new(&category.directory);
    let resolved = match category.directory_mode {
        DirectoryMode::Absolute if path.is_absolute() => path.to_owned(),
        DirectoryMode::Relative
            if !path.is_absolute()
                && path
                    .components()
                    .all(|part| matches!(part, Component::Normal(_) | Component::CurDir)) =>
        {
            let base = if let Some(relative) =
                base.strip_prefix("~/").or_else(|| base.strip_prefix("~\\"))
            {
                dirs::home_dir()
                    .ok_or_else(|| AppError::InvalidInput("Home directory is unavailable".into()))?
                    .join(relative)
            } else {
                PathBuf::from(base)
            };
            if !base.is_absolute() {
                return Err(AppError::InvalidInput(
                    "Default download directory must be absolute".into(),
                ));
            }
            base.join(path)
        }
        _ => {
            return Err(AppError::InvalidInput(
                "Category directory does not match its path mode".into(),
            ))
        }
    };
    Ok(crate::engine::path_to_safe_string(&resolved))
}

/// The extensions a file goes by: its own, and for a part of a split archive
/// (`.7z.001`, `.z01`, `.r00`) the archive's, so it lands with the archive
/// (#736).
fn extensions(value: &str) -> Vec<String> {
    let name = file_name(value);
    let own = lower_extension(&name);
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    let archive = if own.len() == 3 && digits(&own) {
        Path::new(&name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(lower_extension)
            .filter(|inner| !inner.is_empty())
    } else if own.len() == 3 && own.starts_with('z') && digits(&own[1..]) {
        Some("zip".to_owned())
    } else if own.len() == 3 && own.starts_with('r') && digits(&own[1..]) {
        Some("rar".to_owned())
    } else {
        None
    };
    std::iter::once(own).chain(archive).collect()
}

fn file_name(value: &str) -> String {
    match url::Url::parse(value) {
        Ok(url) if matches!(url.scheme(), "http" | "https" | "sftp") => {
            urlencoding::decode(url.path())
                .map(std::borrow::Cow::into_owned)
                .unwrap_or_else(|_| url.path().to_owned())
        }
        Ok(url) if matches!(url.scheme(), "magnet" | "data" | "blob") => String::new(),
        _ => value.to_owned(),
    }
}

fn lower_extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn validate(categories: &[Category], base: &str) -> Result<(), AppError> {
    for category in categories {
        directory(base, category)?;
        patterns(category)?;
    }
    Ok(())
}

fn patterns(category: &Category) -> Result<Vec<regex::Regex>, AppError> {
    category
        .url_patterns
        .iter()
        .filter(|pattern| !pattern.trim().is_empty())
        .map(|pattern| {
            if pattern.len() > 512 {
                return Err(AppError::InvalidInput(
                    "Category URL pattern exceeds 512 bytes".into(),
                ));
            }
            let expression = match category.url_pattern_mode.as_str() {
                "regex" => pattern.clone(),
                "wildcard" => format!("^{}$", regex::escape(pattern).replace("\\*", ".*")),
                _ => {
                    return Err(AppError::InvalidInput(
                        "Unsupported category pattern mode".into(),
                    ))
                }
            };
            regex::RegexBuilder::new(&expression)
                .case_insensitive(true)
                .build()
                .map_err(|error| {
                    AppError::InvalidInput(format!("Invalid category URL pattern: {error}"))
                })
        })
        .collect()
}

pub fn resolve(
    candidates: &[Candidate],
    categories: &[Category],
    base: &str,
) -> Result<Option<Category>, AppError> {
    let compiled = categories
        .iter()
        .map(|category| Ok((category, patterns(category)?)))
        .collect::<Result<Vec<_>, AppError>>()?;
    let mut resolved: Option<Category> = None;
    for candidate in candidates
        .iter()
        .filter(|candidate| !candidate.path.trim().is_empty())
    {
        let names = extensions(&candidate.path);
        let category = compiled.iter().find(|(category, patterns)| {
            !(category.extensions.is_empty() && patterns.is_empty())
                && (category.extensions.is_empty()
                    || category.extensions.iter().any(|value| {
                        let value = value.trim().trim_start_matches('.');
                        names.iter().any(|name| value.eq_ignore_ascii_case(name))
                    }))
                && (patterns.is_empty()
                    || candidate
                        .urls
                        .iter()
                        .chain(std::iter::once(&candidate.path))
                        .filter(|url| url.len() <= 4096)
                        .any(|url| patterns.iter().any(|pattern| pattern.is_match(url))))
        });
        let Some((category, _)) = category else {
            return Ok(None);
        };
        let mut category = (*category).clone();
        category.directory = directory(base, &category)?;
        category.directory_mode = DirectoryMode::Absolute;
        if resolved
            .as_ref()
            .is_some_and(|previous| !same_directory(&previous.directory, &category.directory))
        {
            return Ok(None);
        }
        resolved = Some(category);
    }
    Ok(resolved)
}

fn same_directory(left: &str, right: &str) -> bool {
    let left: PathBuf = Path::new(left).components().collect();
    let right: PathBuf = Path::new(right).components().collect();
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn category() -> Category {
        Category {
            label: "Documents".into(),
            directory: "Documents".into(),
            directory_mode: DirectoryMode::Relative,
            extensions: vec!["pdf".into()],
            url_patterns: vec![],
            url_pattern_mode: "wildcard".into(),
            built_in: true,
        }
    }
    fn candidate(path: &str) -> Candidate {
        Candidate {
            path: path.into(),
            urls: vec![],
        }
    }

    #[test]
    fn relative_targets_follow_the_base_and_fixed_targets_do_not() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let mut rule = category();
        for base in [&first, &second] {
            let result = resolve(
                &[candidate("file.PDF")],
                &[rule.clone()],
                base.to_str().unwrap(),
            )
            .unwrap()
            .unwrap();
            assert_eq!(Path::new(&result.directory), base.join("Documents"));
        }
        rule.directory_mode = DirectoryMode::Absolute;
        rule.directory = first.to_string_lossy().into_owned();
        assert_eq!(
            Path::new(&directory(second.to_str().unwrap(), &rule).unwrap()),
            first
        );
        rule.directory_mode = DirectoryMode::Relative;
        rule.directory = "../outside".into();
        assert!(directory(second.to_str().unwrap(), &rule).is_err());
    }

    #[test]
    fn group_classification_requires_every_file_and_respects_url_rules() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().to_str().unwrap();
        let mut rule = category();
        assert!(resolve(
            &[candidate("a.pdf"), candidate("b.bin")],
            &[rule.clone()],
            base
        )
        .unwrap()
        .is_none());
        rule.url_patterns = vec!["https://*.example.test/*".into()];
        assert!(resolve(&[candidate("a.pdf")], &[rule.clone()], base)
            .unwrap()
            .is_none());
        let matching = Candidate {
            path: "a.pdf".into(),
            urls: vec!["https://cdn.example.test/download".into()],
        };
        assert!(resolve(&[matching], &[rule.clone()], base)
            .unwrap()
            .is_some());
        rule.url_pattern_mode = "regex".into();
        rule.url_patterns = vec!["(".into()];
        assert!(validate(&[rule], base).is_err());
    }

    #[test]
    fn rule_order_and_mixed_destinations_have_one_native_policy() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().to_str().unwrap();
        let first = category();
        let mut second = category();
        second.directory = "Elsewhere".into();
        let resolved = resolve(
            &[candidate("a.pdf")],
            &[first.clone(), second.clone()],
            base,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            Path::new(&resolved.directory),
            temp.path().join("Documents")
        );
        second.extensions = vec!["txt".into()];
        let files = [candidate("a.pdf"), candidate("b.txt")];
        assert!(resolve(&files, &[first.clone(), second.clone()], base)
            .unwrap()
            .is_none());
        second.directory = "Documents/".into();
        assert!(resolve(&files, &[first, second], base).unwrap().is_some());
        assert!(resolve(&[], &[], base).unwrap().is_none());
    }

    #[test]
    fn filenames_are_text_and_only_url_paths_are_decoded() {
        assert_eq!(extensions("https://example.test/file%2Epdf?x=1"), ["pdf"]);
        assert_eq!(extensions("file%2Epdf"), [""]);
        assert_eq!(extensions("magnet:?dn=file.pdf"), [""]);
        assert_eq!(extensions(".gitignore"), [""]);
        assert_eq!(extensions("archive.tar.gz"), ["gz"]);
    }

    #[test]
    fn parts_of_split_archives_go_by_the_archive() {
        assert_eq!(extensions("backup.7z.001"), ["001", "7z"]);
        assert_eq!(extensions("https://example.test/a.zip.002"), ["002", "zip"]);
        assert_eq!(extensions("disk.z01"), ["z01", "zip"]);
        assert_eq!(extensions("disk.R00"), ["r00", "rar"]);
        assert_eq!(extensions("track.001"), ["001"]);
        assert_eq!(extensions("movie.mp4"), ["mp4"]);
        assert_eq!(extensions("photo.zip"), ["zip"]);
    }
}
