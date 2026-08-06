use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const MAX_FILE_BYTES: usize = 4 * 1024;
const MAX_TOTAL_BYTES: usize = 16 * 1024;
const RECENT_MEMORY_FILES: usize = 3;

/// Native, runtime-independent projection of the Agent's durable workspace.
/// It does not own writes: Pi's regular coding tools remain the single file API.
pub struct WorkspaceContext {
    root: PathBuf,
}

impl WorkspaceContext {
    pub fn new(workspace_root: &Path) -> Self {
        Self {
            root: workspace_root.to_owned(),
        }
    }

    pub fn definitions() -> Vec<serde_json::Value> {
        vec![serde_json::json!({
            "name":"workspace.context",
            "description":"Load the bounded durable context for this Agent from /workspace: AGENTS.md, strategy.md, memory/INDEX.md, and the newest memory notes. Use read/write/edit/find/grep/ls to organize or update those files.",
            "parameters":{"type":"object","properties":{},"additionalProperties":false}
        })]
    }

    pub fn execute(&self, name: &str) -> Result<Option<serde_json::Value>, String> {
        if name != "workspace.context" {
            return Ok(None);
        }
        self.load().map(Some)
    }

    fn load(&self) -> Result<serde_json::Value, String> {
        let mut candidates = vec![
            self.root.join("AGENTS.md"),
            self.root.join("strategy.md"),
            self.root.join("memory/INDEX.md"),
        ];
        candidates.extend(self.recent_memory_notes()?);

        let mut remaining = MAX_TOTAL_BYTES;
        let mut files = Vec::new();
        let mut missing = Vec::new();
        for path in candidates {
            let relative = display_path(&self.root, &path);
            let metadata = match std::fs::symlink_metadata(&path) {
                Ok(metadata) if metadata.file_type().is_file() => metadata,
                Ok(_) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    missing.push(relative);
                    continue;
                }
                Err(error) => return Err(format!("inspect {relative}: {error}")),
            };
            if remaining == 0 {
                break;
            }
            let bytes =
                std::fs::read(&path).map_err(|error| format!("read {relative}: {error}"))?;
            let take = bytes.len().min(MAX_FILE_BYTES).min(remaining);
            let content = utf8_prefix(&bytes, take);
            let used = content.len();
            remaining = remaining.saturating_sub(used);
            files.push(serde_json::json!({
                "path":relative,
                "content":content,
                "sizeBytes":metadata.len(),
                "truncated":used < bytes.len()
            }));
        }

        Ok(serde_json::json!({
            "status":"ok",
            "workspace":"/workspace",
            "files":files,
            "missing":missing,
            "loadedBytes":MAX_TOTAL_BYTES - remaining,
            "maxBytes":MAX_TOTAL_BYTES,
            "writeTools":["write","edit"],
            "discoveryTools":["read","find","grep","ls"]
        }))
    }

    fn recent_memory_notes(&self) -> Result<Vec<PathBuf>, String> {
        let directory = self.root.join("memory");
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(format!("read /workspace/memory: {error}")),
        };
        let mut notes = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| format!("read memory entry: {error}"))?;
            let path = entry.path();
            if path.file_name().and_then(|name| name.to_str()) == Some("INDEX.md")
                || path.extension().and_then(|extension| extension.to_str()) != Some("md")
            {
                continue;
            }
            let file_type = entry
                .file_type()
                .map_err(|error| format!("inspect memory entry: {error}"))?;
            if !file_type.is_file() {
                continue;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                .map(|value| value.as_secs())
                .unwrap_or(0);
            notes.push((modified, path));
        }
        notes.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        Ok(notes
            .into_iter()
            .take(RECENT_MEMORY_FILES)
            .map(|(_, path)| path)
            .collect())
    }
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .map(|relative| format!("/workspace/{}", relative.display()))
        .unwrap_or_else(|| path.display().to_string())
}

fn utf8_prefix(bytes: &[u8], limit: usize) -> String {
    let mut end = limit.min(bytes.len());
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}
