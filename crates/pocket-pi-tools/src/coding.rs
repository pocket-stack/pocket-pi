use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use globset::GlobBuilder;
use regex::RegexBuilder;

use crate::NativeToolResult;

const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_OUTPUT_BYTES: usize = 50 * 1024;
const MAX_OUTPUT_LINES: usize = 2_000;
const MAX_WALK_ENTRIES: usize = 4_096;
const MAX_WALK_DEPTH: usize = 32;

pub fn definitions() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "name":"read",
            "description":"Read a UTF-8 text file in the ESP32 workspace. Output is truncated to 2000 lines or 50KB; use offset/limit to continue.",
            "parameters":{"type":"object","properties":{"path":{"type":"string","description":"Path relative to the workspace, or an absolute /workspace path"},"offset":{"type":"number","description":"1-indexed starting line"},"limit":{"type":"number","description":"Maximum lines"}},"required":["path"],"additionalProperties":false}
        }),
        serde_json::json!({
            "name":"write",
            "description":"Create or completely overwrite a UTF-8 file in the ESP32 workspace. Parent directories are created automatically.",
            "parameters":{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"],"additionalProperties":false}
        }),
        serde_json::json!({
            "name":"edit",
            "description":"Edit one UTF-8 file using exact, unique, non-overlapping replacements matched against the original file.",
            "parameters":{"type":"object","properties":{"path":{"type":"string"},"edits":{"type":"array","minItems":1,"items":{"type":"object","properties":{"oldText":{"type":"string"},"newText":{"type":"string"}},"required":["oldText","newText"],"additionalProperties":false}}},"required":["path","edits"],"additionalProperties":false}
        }),
        serde_json::json!({
            "name":"find",
            "description":"Find workspace files by glob pattern. Returns paths relative to the search directory.",
            "parameters":{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"},"limit":{"type":"number"}},"required":["pattern"],"additionalProperties":false}
        }),
        serde_json::json!({
            "name":"grep",
            "description":"Search UTF-8 workspace files and return matching lines with paths and line numbers.",
            "parameters":{"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string"},"glob":{"type":"string"},"ignoreCase":{"type":"boolean"},"literal":{"type":"boolean"},"context":{"type":"number"},"limit":{"type":"number"}},"required":["pattern"],"additionalProperties":false}
        }),
        serde_json::json!({
            "name":"ls",
            "description":"List a workspace directory alphabetically, appending / to directories.",
            "parameters":{"type":"object","properties":{"path":{"type":"string"},"limit":{"type":"number"}},"additionalProperties":false}
        }),
    ]
}

pub struct CodingTools {
    root: PathBuf,
    mutation_lock: Mutex<()>,
}

impl CodingTools {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            mutation_lock: Mutex::new(()),
        }
    }

    pub fn execute(
        &self,
        call_id: &str,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<Option<NativeToolResult>, String> {
        let result = match name {
            "read" => self.read(args),
            "write" => self.write(call_id, args),
            "edit" => self.edit(call_id, args),
            "find" => self.find(args),
            "grep" => self.grep(args),
            "ls" => self.ls(args),
            _ => return Ok(None),
        }?;
        Ok(Some(result))
    }

    fn read(&self, args: &serde_json::Value) -> Result<NativeToolResult, String> {
        let started = std::time::Instant::now();
        let raw_path = required_string(args, "path")?;
        let path = self.resolve(raw_path)?;
        log::info!("diag fs.read phase=start path={raw_path}");
        let content = read_text_file(&path)?;
        let lines: Vec<&str> = content.split('\n').collect();
        let offset = optional_usize(args, "offset")?.unwrap_or(1).max(1);
        if offset > lines.len() {
            return Err(format!(
                "Offset {offset} is beyond end of file ({} lines total)",
                lines.len()
            ));
        }
        let requested = optional_usize(args, "limit")?.unwrap_or(usize::MAX);
        let selected = lines
            .iter()
            .skip(offset - 1)
            .take(requested)
            .copied()
            .collect::<Vec<_>>()
            .join("\n");
        let (mut text, truncation) = truncate_output(&selected);
        let selected_lines = text.split('\n').count();
        let next_offset = offset + selected_lines;
        if next_offset <= lines.len() && (requested != usize::MAX || truncation) {
            text.push_str(&format!(
                "\n\n[More content available. Use offset={next_offset} to continue.]"
            ));
        }
        let result = NativeToolResult {
            text,
            details: serde_json::json!({
                "path": display_path(&self.root, &path),
                "totalLines": lines.len(),
                "truncated": truncation
            }),
            terminate: false,
        };
        log::info!(
            "diag fs.read phase=done path={raw_path} elapsed_ms={} file_bytes={} total_lines={} output_bytes={} truncated={truncation}",
            started.elapsed().as_millis(),
            content.len(),
            lines.len(),
            result.text.len()
        );
        Ok(result)
    }

    fn write(&self, call_id: &str, args: &serde_json::Value) -> Result<NativeToolResult, String> {
        let started = std::time::Instant::now();
        let raw_path = required_string(args, "path")?;
        let content = required_string(args, "content")?;
        ensure_size(content.len() as u64)?;
        let path = self.resolve(raw_path)?;
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| "workspace mutation lock poisoned")?;
        log::info!(
            "diag fs.write phase=start path={raw_path} bytes={}",
            content.len()
        );
        atomic_write(&path, raw_path, content.as_bytes(), call_id)?;
        log::info!(
            "diag fs.write phase=done path={raw_path} elapsed_ms={} bytes={}",
            started.elapsed().as_millis(),
            content.len()
        );
        Ok(NativeToolResult {
            text: format!("Successfully wrote {} bytes to {raw_path}", content.len()),
            details: serde_json::json!({"path":display_path(&self.root, &path),"bytes":content.len()}),
            terminate: false,
        })
    }

    fn edit(&self, call_id: &str, args: &serde_json::Value) -> Result<NativeToolResult, String> {
        let started = std::time::Instant::now();
        let raw_path = required_string(args, "path")?;
        let edits = args
            .get("edits")
            .and_then(|value| value.as_array())
            .ok_or_else(|| "edit.edits must be an array".to_owned())?;
        if edits.is_empty() {
            return Err("edit.edits must contain at least one replacement".to_owned());
        }
        let path = self.resolve(raw_path)?;
        let _guard = self
            .mutation_lock
            .lock()
            .map_err(|_| "workspace mutation lock poisoned")?;
        log::info!(
            "diag fs.edit phase=start path={raw_path} replacements={}",
            edits.len()
        );
        let original = read_text_file(&path)?;
        let uses_crlf = original.contains("\r\n");
        let normalized = original.replace("\r\n", "\n").replace('\r', "\n");
        let mut replacements = Vec::with_capacity(edits.len());
        for edit in edits {
            let old = required_string(edit, "oldText")?
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            let new = required_string(edit, "newText")?
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            if old.is_empty() {
                return Err("edit.oldText must not be empty".to_owned());
            }
            let matches = normalized.match_indices(&old).collect::<Vec<_>>();
            if matches.len() != 1 {
                return Err(format!(
                    "Could not edit {raw_path}: oldText must match exactly once, matched {} times",
                    matches.len()
                ));
            }
            let start = matches[0].0;
            replacements.push((start, start + old.len(), new));
        }
        replacements.sort_by_key(|replacement| replacement.0);
        for pair in replacements.windows(2) {
            if pair[0].1 > pair[1].0 {
                return Err("edit replacements overlap; merge them into one edit".to_owned());
            }
        }
        let mut updated = normalized;
        for (start, end, new) in replacements.iter().rev() {
            updated.replace_range(*start..*end, new);
        }
        if uses_crlf {
            updated = updated.replace('\n', "\r\n");
        }
        ensure_size(updated.len() as u64)?;
        atomic_write(&path, raw_path, updated.as_bytes(), call_id)?;
        log::info!(
            "diag fs.edit phase=done path={raw_path} elapsed_ms={} replacements={} old_bytes={} new_bytes={}",
            started.elapsed().as_millis(),
            edits.len(),
            original.len(),
            updated.len()
        );
        Ok(NativeToolResult {
            text: format!(
                "Successfully replaced {} block(s) in {raw_path}.",
                edits.len()
            ),
            details: serde_json::json!({
                "path":display_path(&self.root, &path),
                "replacements":edits.len(),
                "bytes":updated.len()
            }),
            terminate: false,
        })
    }

    fn find(&self, args: &serde_json::Value) -> Result<NativeToolResult, String> {
        let started = std::time::Instant::now();
        let pattern = required_string(args, "pattern")?;
        let raw_path = optional_string(args, "path").unwrap_or(".");
        let search = self.resolve(raw_path)?;
        log::info!("diag fs.find phase=start path={raw_path}");
        require_directory(&search)?;
        let limit = optional_usize(args, "limit")?
            .unwrap_or(1_000)
            .clamp(1, 4_096);
        let matcher = build_glob(pattern)?;
        let basename_only = !pattern.contains('/');
        let mut paths = Vec::new();
        walk_files(&search, &search, 0, &mut 0, &mut |path, relative| {
            let candidate = if basename_only {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("")
            } else {
                relative
            };
            if matcher.is_match(candidate) && paths.len() < limit {
                paths.push(relative.to_owned());
            }
        })?;
        paths.sort_by_key(|path| path.to_lowercase());
        let limit_reached = paths.len() >= limit;
        let output = if paths.is_empty() {
            "No files found matching pattern".to_owned()
        } else {
            paths.join("\n")
        };
        let (mut text, truncated) = truncate_output(&output);
        if limit_reached {
            text.push_str(&format!("\n\n[{limit} results limit reached]"));
        }
        let result = NativeToolResult {
            text,
            details: serde_json::json!({"resultLimitReached":limit_reached.then_some(limit),"truncated":truncated}),
            terminate: false,
        };
        log::info!(
            "diag fs.find phase=done path={raw_path} elapsed_ms={} results={} limit_reached={limit_reached} output_truncated={truncated}",
            started.elapsed().as_millis(),
            paths.len()
        );
        Ok(result)
    }

    fn grep(&self, args: &serde_json::Value) -> Result<NativeToolResult, String> {
        let started = std::time::Instant::now();
        let pattern = required_string(args, "pattern")?;
        let raw_path = optional_string(args, "path").unwrap_or(".");
        let search = self.resolve(raw_path)?;
        log::info!("diag fs.grep phase=start path={raw_path}");
        if !search.exists() {
            return Err(format!("Path not found: {}", search.display()));
        }
        let literal = optional_bool(args, "literal").unwrap_or(false);
        let source = if literal {
            regex::escape(pattern)
        } else {
            pattern.to_owned()
        };
        let regex = RegexBuilder::new(&source)
            .case_insensitive(optional_bool(args, "ignoreCase").unwrap_or(false))
            .build()
            .map_err(|error| format!("invalid grep pattern: {error}"))?;
        let context = optional_usize(args, "context")?.unwrap_or(0).min(20);
        let limit = optional_usize(args, "limit")?
            .unwrap_or(100)
            .clamp(1, 1_000);
        let glob_pattern = optional_string(args, "glob");
        let glob = glob_pattern.map(build_glob).transpose()?;
        let glob_basename_only = glob_pattern
            .map(|pattern| !pattern.contains('/'))
            .unwrap_or(false);
        let mut files = Vec::new();
        if search.is_file() {
            files.push((
                search.clone(),
                search
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("file")
                    .to_owned(),
            ));
        } else {
            walk_files(&search, &search, 0, &mut 0, &mut |path, relative| {
                let glob_candidate = if glob_basename_only {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("")
                } else {
                    relative
                };
                if glob
                    .as_ref()
                    .map(|matcher| matcher.is_match(glob_candidate))
                    .unwrap_or(true)
                {
                    files.push((path.to_owned(), relative.to_owned()));
                }
            })?;
        }
        files.sort_by(|a, b| a.1.cmp(&b.1));
        let mut blocks = Vec::new();
        let mut matches = 0usize;
        let mut lines_truncated = false;
        'files: for (path, relative) in files {
            let Ok(content) = read_text_file(&path) else {
                continue;
            };
            let lines = content.split('\n').collect::<Vec<_>>();
            for (index, line) in lines.iter().enumerate() {
                if !regex.is_match(line) {
                    continue;
                }
                matches += 1;
                let start = index.saturating_sub(context);
                let end = (index + context + 1).min(lines.len());
                for (current, line) in lines.iter().enumerate().take(end).skip(start) {
                    let mut value = (*line).to_owned();
                    if value.chars().count() > 500 {
                        value = value.chars().take(500).collect::<String>();
                        lines_truncated = true;
                    }
                    let separator = if current == index { ':' } else { '-' };
                    blocks.push(format!(
                        "{relative}{separator}{}{separator} {value}",
                        current + 1
                    ));
                }
                if matches >= limit {
                    break 'files;
                }
            }
        }
        let limit_reached = matches >= limit;
        let output = if blocks.is_empty() {
            "No matches found".to_owned()
        } else {
            blocks.join("\n")
        };
        let (mut text, truncated) = truncate_output(&output);
        let mut notices = Vec::new();
        if limit_reached {
            notices.push(format!("{limit} matches limit reached"));
        }
        if lines_truncated {
            notices.push("some lines truncated to 500 characters".to_owned());
        }
        if truncated {
            notices.push("50KB output limit reached".to_owned());
        }
        if !notices.is_empty() {
            text.push_str(&format!("\n\n[{}]", notices.join(". ")));
        }
        let result = NativeToolResult {
            text,
            details: serde_json::json!({"matchLimitReached":limit_reached.then_some(limit),"linesTruncated":lines_truncated,"truncated":truncated}),
            terminate: false,
        };
        log::info!(
            "diag fs.grep phase=done path={raw_path} elapsed_ms={} matches={matches} limit_reached={limit_reached} output_truncated={truncated}",
            started.elapsed().as_millis()
        );
        Ok(result)
    }

    fn ls(&self, args: &serde_json::Value) -> Result<NativeToolResult, String> {
        let started = std::time::Instant::now();
        let raw_path = optional_string(args, "path").unwrap_or(".");
        let path = self.resolve(raw_path)?;
        log::info!("diag fs.ls phase=start path={raw_path}");
        require_directory(&path)?;
        let limit = optional_usize(args, "limit")?
            .unwrap_or(500)
            .clamp(1, 4_096);
        let mut entries = fs::read_dir(&path)
            .map_err(|error| format!("Cannot read directory {}: {error}", path.display()))?
            .filter_map(Result::ok)
            .map(|entry| {
                let mut name = entry.file_name().to_string_lossy().into_owned();
                if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
                    name.push('/');
                }
                name
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.to_lowercase());
        let limit_reached = entries.len() > limit;
        entries.truncate(limit);
        let output = if entries.is_empty() {
            "(empty directory)".to_owned()
        } else {
            entries.join("\n")
        };
        let (mut text, truncated) = truncate_output(&output);
        if limit_reached {
            text.push_str(&format!("\n\n[{limit} entries limit reached]"));
        }
        let result = NativeToolResult {
            text,
            details: serde_json::json!({"entryLimitReached":limit_reached.then_some(limit),"truncated":truncated}),
            terminate: false,
        };
        log::info!(
            "diag fs.ls phase=done path={raw_path} elapsed_ms={} entries={} limit_reached={limit_reached} output_truncated={truncated}",
            started.elapsed().as_millis(),
            entries.len()
        );
        Ok(result)
    }

    fn resolve(&self, input: &str) -> Result<PathBuf, String> {
        let path = Path::new(input);
        let relative = if path.is_absolute() {
            path.strip_prefix(&self.root)
                .map_err(|_| format!("absolute path must stay under {}", self.root.display()))?
        } else {
            path
        };
        let mut clean = PathBuf::new();
        for component in relative.components() {
            match component {
                Component::Normal(part) => clean.push(part),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err("path escapes the ESP32 workspace".to_owned())
                }
            }
        }
        Ok(self.root.join(clean))
    }
}

fn required_string<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("{key} must be a string"))
}

fn optional_string<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|value| value.as_str())
}

fn optional_bool(args: &serde_json::Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|value| value.as_bool())
}

fn optional_usize(args: &serde_json::Value, key: &str) -> Result<Option<usize>, String> {
    let Some(value) = args.get(key) else {
        return Ok(None);
    };
    let number = value
        .as_f64()
        .ok_or_else(|| format!("{key} must be a number"))?;
    if !number.is_finite() || number < 0.0 || number.fract() != 0.0 {
        return Err(format!("{key} must be a non-negative integer"));
    }
    Ok(Some(number as usize))
}

fn ensure_size(bytes: u64) -> Result<(), String> {
    if bytes > MAX_FILE_BYTES {
        Err(format!(
            "file exceeds embedded {}KB limit",
            MAX_FILE_BYTES / 1024
        ))
    } else {
        Ok(())
    }
}

fn read_text_file(path: &Path) -> Result<String, String> {
    let metadata =
        fs::metadata(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    ensure_size(metadata.len())?;
    let bytes =
        fs::read(path).map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    String::from_utf8(bytes).map_err(|_| format!("File is not valid UTF-8: {}", path.display()))
}

fn atomic_write(path: &Path, path_label: &str, bytes: &[u8], call_id: &str) -> Result<(), String> {
    let started = std::time::Instant::now();
    let parent = path
        .parent()
        .ok_or_else(|| "file has no parent directory".to_owned())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("Cannot create {}: {error}", parent.display()))?;
    let mut hasher = DefaultHasher::new();
    call_id.hash(&mut hasher);
    path.hash(&mut hasher);
    let temp = parent.join(format!(".ppi-{:016x}.tmp", hasher.finish()));
    log::info!(
        "diag fs.atomic_write phase=create path={path_label} bytes={}",
        bytes.len()
    );
    let result = (|| {
        let mut file = fs::File::create(&temp)
            .map_err(|error| format!("Cannot create temporary file: {error}"))?;
        file.write_all(bytes)
            .map_err(|error| format!("Cannot write temporary file: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("Cannot sync temporary file: {error}"))?;
        log::info!("diag fs.atomic_write phase=synced path={path_label}");
        drop(file);
        fs::rename(&temp, path)
            .map_err(|error| format!("Cannot replace {}: {error}", path.display()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    log::info!(
        "diag fs.atomic_write phase=done path={path_label} elapsed_ms={} ok={}",
        started.elapsed().as_millis(),
        result.is_ok()
    );
    result
}

fn require_directory(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Path not found: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("Not a directory: {}", path.display()));
    }
    Ok(())
}

fn build_glob(pattern: &str) -> Result<globset::GlobMatcher, String> {
    GlobBuilder::new(pattern)
        .literal_separator(true)
        .backslash_escape(false)
        .build()
        .map(|glob| glob.compile_matcher())
        .map_err(|error| format!("invalid glob pattern: {error}"))
}

fn walk_files(
    directory: &Path,
    root: &Path,
    depth: usize,
    visited: &mut usize,
    visitor: &mut dyn FnMut(&Path, &str),
) -> Result<(), String> {
    if depth > MAX_WALK_DEPTH {
        return Err("workspace directory depth limit reached".to_owned());
    }
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("Cannot read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        *visited += 1;
        if *visited > MAX_WALK_ENTRIES {
            return Err("workspace scan entry limit reached".to_owned());
        }
        let path = entry.path();
        let name = entry.file_name();
        if name == ".git" || name == "node_modules" {
            continue;
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            walk_files(&path, root, depth + 1, visited, visitor)?;
        } else {
            visitor(&path, &relative);
        }
    }
    Ok(())
}

fn truncate_output(input: &str) -> (String, bool) {
    let mut output = String::new();
    let mut truncated = false;
    for (index, line) in input.split('\n').enumerate() {
        if index >= MAX_OUTPUT_LINES {
            truncated = true;
            break;
        }
        let separator = usize::from(!output.is_empty());
        if output.len() + separator + line.len() > MAX_OUTPUT_BYTES {
            truncated = true;
            break;
        }
        if separator == 1 {
            output.push('\n');
        }
        output.push_str(line);
    }
    (output, truncated)
}

fn display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}
