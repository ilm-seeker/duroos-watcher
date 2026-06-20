use std::{
    collections::HashSet,
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use tauri::{AppHandle, Manager};

pub const REQUIRED_MEDIA_TOOLS: &[&str] = &["yt-dlp", "ffmpeg", "ffprobe"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaToolSource {
    Bundled,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaToolStatus {
    pub tool: String,
    pub source: Option<MediaToolSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequiredMediaToolStatus {
    pub available: bool,
    pub source: String,
    pub missing: Vec<String>,
}

pub fn prepare_bundled_media_tool_path(app: &AppHandle) {
    prepend_existing_dirs_to_path(&bundled_media_tool_dirs(app));
}

pub fn required_media_tool_status(app: &AppHandle) -> RequiredMediaToolStatus {
    let bundled_dirs = bundled_media_tool_dirs(app);
    let tools: Vec<MediaToolStatus> = REQUIRED_MEDIA_TOOLS
        .iter()
        .map(|tool| resolve_media_tool(tool, &bundled_dirs))
        .collect();
    let missing: Vec<String> = tools
        .iter()
        .filter(|tool| tool.source.is_none())
        .map(|tool| tool.tool.clone())
        .collect();

    RequiredMediaToolStatus {
        available: missing.is_empty(),
        source: media_tool_source_label(&tools),
        missing,
    }
}

pub fn bundled_media_tool_dirs(app: &AppHandle) -> Vec<PathBuf> {
    app.path()
        .resource_dir()
        .map(|resource_dir| bundled_media_tool_dirs_for_resource(&resource_dir))
        .unwrap_or_default()
}

fn bundled_media_tool_dirs_for_resource(resource_dir: &Path) -> Vec<PathBuf> {
    let roots = [
        resource_dir.join("binaries").join("vendor"),
        resource_dir.join("binaries"),
        resource_dir.to_path_buf(),
    ];
    let mut directories = Vec::new();
    let mut seen = HashSet::new();

    for root in roots {
        push_unique_path(&mut directories, &mut seen, root.clone());
        for child in sorted_child_dirs(&root) {
            push_unique_path(&mut directories, &mut seen, child.clone());
            for grandchild in sorted_child_dirs(&child) {
                push_unique_path(&mut directories, &mut seen, grandchild);
            }
        }
    }

    directories
}

fn resolve_media_tool(tool: &str, bundled_dirs: &[PathBuf]) -> MediaToolStatus {
    if find_bundled_media_tool(tool, bundled_dirs).is_some() {
        return MediaToolStatus {
            tool: tool.to_string(),
            source: Some(MediaToolSource::Bundled),
        };
    }

    if find_system_media_tool(tool).is_some() {
        return MediaToolStatus {
            tool: tool.to_string(),
            source: Some(MediaToolSource::System),
        };
    }

    MediaToolStatus {
        tool: tool.to_string(),
        source: None,
    }
}

fn find_bundled_media_tool(tool: &str, bundled_dirs: &[PathBuf]) -> Option<String> {
    bundled_media_tool_candidates(tool, bundled_dirs)
        .into_iter()
        .find(|candidate| command_available(candidate.as_path(), version_arg_for_tool(tool)))
        .map(|candidate| candidate.to_string_lossy().to_string())
}

fn bundled_media_tool_candidates(tool: &str, bundled_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    for directory in bundled_dirs {
        candidates.push(directory.join(tool));
        candidates.push(directory.join(format!("{tool}.exe")));
    }
    candidates
}

fn find_system_media_tool(tool: &str) -> Option<String> {
    system_media_tool_candidates(tool)
        .into_iter()
        .find(|candidate| command_available(Path::new(candidate), version_arg_for_tool(tool)))
}

fn system_media_tool_candidates(tool: &str) -> Vec<String> {
    let mut candidates = vec![
        tool.to_string(),
        format!("{tool}.exe"),
        format!("/opt/homebrew/bin/{tool}"),
        format!("/usr/local/bin/{tool}"),
        format!("/usr/bin/{tool}"),
    ];

    if cfg!(target_os = "windows") {
        if let Some(program_files) = env::var_os("ProgramFiles") {
            candidates.push(
                PathBuf::from(program_files)
                    .join("ffmpeg")
                    .join("bin")
                    .join(format!("{tool}.exe"))
                    .to_string_lossy()
                    .to_string(),
            );
        }
    }

    candidates
}

fn command_available(program: &Path, version_arg: &str) -> bool {
    if program.components().count() > 1 && !program.is_file() {
        return false;
    }

    Command::new(program)
        .arg(version_arg)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn version_arg_for_tool(tool: &str) -> &'static str {
    if tool == "yt-dlp" {
        "--version"
    } else {
        "-version"
    }
}

fn media_tool_source_label(tools: &[MediaToolStatus]) -> String {
    if tools.iter().any(|tool| tool.source.is_none()) {
        return "missing".to_string();
    }

    let sources: HashSet<MediaToolSource> = tools.iter().filter_map(|tool| tool.source).collect();
    match sources.len() {
        0 => "missing".to_string(),
        1 if sources.contains(&MediaToolSource::Bundled) => "bundled".to_string(),
        1 if sources.contains(&MediaToolSource::System) => "system".to_string(),
        _ => "mixed".to_string(),
    }
}

fn sorted_child_dirs(directory: &Path) -> Vec<PathBuf> {
    let mut children: Vec<PathBuf> = fs::read_dir(directory)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    children.sort();
    children
}

fn prepend_existing_dirs_to_path(dirs: &[PathBuf]) {
    let current_path = env::var_os("PATH").unwrap_or_default();
    let current_dirs: Vec<PathBuf> = env::split_paths(&current_path).collect();
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    for directory in dirs.iter().filter(|directory| directory.is_dir()) {
        push_unique_path(&mut paths, &mut seen, directory.clone());
    }
    for directory in current_dirs {
        push_unique_path(&mut paths, &mut seen, directory);
    }

    if let Ok(joined_path) = env::join_paths(paths) {
        env::set_var("PATH", joined_path);
    }
}

fn push_unique_path(paths: &mut Vec<PathBuf>, seen: &mut HashSet<OsString>, path: PathBuf) {
    if seen.insert(path.as_os_str().to_os_string()) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_dirs_include_vendor_target_roots_before_parent_binaries() {
        let resource_dir = PathBuf::from("/app/resources");
        let dirs = bundled_media_tool_dirs_for_resource(&resource_dir);

        assert_eq!(dirs[0], PathBuf::from("/app/resources/binaries/vendor"));
        assert_eq!(dirs[1], PathBuf::from("/app/resources/binaries"));
        assert!(dirs.contains(&PathBuf::from("/app/resources")));
    }

    #[test]
    fn media_tool_source_label_reports_complete_sources() {
        let bundled = vec![
            MediaToolStatus {
                tool: "yt-dlp".to_string(),
                source: Some(MediaToolSource::Bundled),
            },
            MediaToolStatus {
                tool: "ffmpeg".to_string(),
                source: Some(MediaToolSource::Bundled),
            },
        ];
        let mixed = vec![
            MediaToolStatus {
                tool: "yt-dlp".to_string(),
                source: Some(MediaToolSource::Bundled),
            },
            MediaToolStatus {
                tool: "ffmpeg".to_string(),
                source: Some(MediaToolSource::System),
            },
        ];
        let missing = vec![MediaToolStatus {
            tool: "ffprobe".to_string(),
            source: None,
        }];

        assert_eq!(media_tool_source_label(&bundled), "bundled");
        assert_eq!(media_tool_source_label(&mixed), "mixed");
        assert_eq!(media_tool_source_label(&missing), "missing");
    }
}
