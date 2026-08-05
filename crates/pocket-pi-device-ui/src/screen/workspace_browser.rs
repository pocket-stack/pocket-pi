use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const MAX_VIEWER_BYTES: u64 = 256 * 1024;
const VIEWER_COLUMNS: usize = 32;

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub timestamp: Option<FileTimestamp>,
}

#[derive(Clone, Copy, Debug)]
pub struct FileTimestamp {
    pub unix_seconds: u64,
    kind: TimestampKind,
}

#[derive(Clone, Copy, Debug)]
enum TimestampKind {
    Created,
    Updated,
    Contents,
}

#[derive(Clone, Debug)]
pub struct OpenFile {
    pub relative_path: String,
    pub size: u64,
    pub timestamp: Option<FileTimestamp>,
    pub lines: Vec<String>,
    pub line_offset: usize,
}

#[derive(Debug)]
pub struct WorkspaceBrowser {
    root: PathBuf,
    current_dir: PathBuf,
    pub entries: Vec<FileEntry>,
    pub list_offset: usize,
    pub open_file: Option<OpenFile>,
    pub status: Option<String>,
}

impl WorkspaceBrowser {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            current_dir: PathBuf::new(),
            entries: Vec::new(),
            list_offset: 0,
            open_file: None,
            status: None,
        }
    }

    pub fn current_path(&self) -> String {
        if self.current_dir.as_os_str().is_empty() {
            "/workspace".to_owned()
        } else {
            format!("/workspace/{}", path_text(&self.current_dir))
        }
    }

    pub fn can_go_up(&self) -> bool {
        !self.current_dir.as_os_str().is_empty()
    }

    pub fn refresh(&mut self) {
        self.status = None;
        let directory = self.root.join(&self.current_dir);
        let read_dir = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) => {
                self.status = Some(format!("CANNOT READ DIRECTORY: {error}"));
                self.entries.clear();
                return;
            }
        };
        let mut entries = Vec::new();
        for entry in read_dir.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if hidden_internal_name(&name) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let path = entry.path();
            entries.push(FileEntry {
                name,
                is_dir: metadata.is_dir(),
                size: if metadata.is_file() {
                    metadata.len()
                } else {
                    0
                },
                timestamp: entry_timestamp(&path, &metadata),
            });
        }
        entries.sort_by(|left, right| {
            right
                .is_dir
                .cmp(&left.is_dir)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        self.entries = entries;
        self.list_offset = self.list_offset.min(self.entries.len().saturating_sub(1));
    }

    pub fn go_up(&mut self) {
        if self.current_dir.pop() {
            self.list_offset = 0;
            self.refresh();
        }
    }

    pub fn activate_visible_row(&mut self, row: usize) -> bool {
        let Some(entry) = self.entries.get(self.list_offset + row).cloned() else {
            return false;
        };
        if entry.is_dir {
            self.current_dir.push(&entry.name);
            self.list_offset = 0;
            self.refresh();
            return true;
        }
        self.open(&entry);
        self.open_file.is_some()
    }

    pub fn scroll_list(&mut self, delta: isize, visible_rows: usize) {
        let maximum = self.entries.len().saturating_sub(visible_rows);
        self.list_offset = self.list_offset.saturating_add_signed(delta).min(maximum);
    }

    pub fn scroll_file(&mut self, delta: isize, visible_lines: usize) {
        if let Some(file) = self.open_file.as_mut() {
            let maximum = file.lines.len().saturating_sub(visible_lines);
            file.line_offset = file.line_offset.saturating_add_signed(delta).min(maximum);
        }
    }

    pub fn close_file(&mut self) {
        self.open_file = None;
        self.refresh();
    }

    fn open(&mut self, entry: &FileEntry) {
        self.status = None;
        let relative = self.current_dir.join(&entry.name);
        let path = self.root.join(&relative);
        if entry.size > MAX_VIEWER_BYTES {
            self.status = Some(format!("FILE TOO LARGE: {}", format_size(entry.size)));
            return;
        }
        match fs::read_to_string(&path) {
            Ok(content) => {
                self.open_file = Some(OpenFile {
                    relative_path: path_text(&relative),
                    size: entry.size,
                    timestamp: entry.timestamp,
                    lines: wrap_text(&content, VIEWER_COLUMNS),
                    line_offset: 0,
                });
            }
            Err(error) => {
                self.status = Some(format!("CANNOT OPEN TEXT FILE: {error}"));
            }
        }
    }
}

fn entry_timestamp(path: &Path, metadata: &fs::Metadata) -> Option<FileTimestamp> {
    file_timestamp(metadata).or_else(|| {
        metadata.is_dir().then(|| {
            let mut remaining = 256;
            newest_descendant_timestamp(path, 0, &mut remaining).map(|timestamp| FileTimestamp {
                unix_seconds: timestamp.unix_seconds,
                kind: TimestampKind::Contents,
            })
        })?
    })
}

fn newest_descendant_timestamp(
    directory: &Path,
    depth: usize,
    remaining: &mut usize,
) -> Option<FileTimestamp> {
    if depth >= 8 || *remaining == 0 {
        return None;
    }
    let mut newest = None;
    for entry in fs::read_dir(directory).ok()?.flatten() {
        if *remaining == 0 {
            break;
        }
        *remaining -= 1;
        let name = entry.file_name().to_string_lossy().into_owned();
        if hidden_internal_name(&name) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let candidate = file_timestamp(&metadata).or_else(|| {
            metadata
                .is_dir()
                .then(|| newest_descendant_timestamp(&entry.path(), depth + 1, remaining))?
        });
        if candidate.is_some_and(|candidate| {
            newest
                .is_none_or(|current: FileTimestamp| candidate.unix_seconds > current.unix_seconds)
        }) {
            newest = candidate;
        }
    }
    newest
}

fn hidden_internal_name(name: &str) -> bool {
    name == ".pi-agent" || (name.starts_with(".ppi-") && name.ends_with(".tmp"))
}

fn file_timestamp(metadata: &fs::Metadata) -> Option<FileTimestamp> {
    if let Ok(created) = metadata.created() {
        if let Ok(duration) = created.duration_since(UNIX_EPOCH) {
            let seconds = duration.as_secs();
            if valid_wall_clock(seconds) {
                return Some(FileTimestamp {
                    unix_seconds: seconds,
                    kind: TimestampKind::Created,
                });
            }
        }
    }
    // LittleFS primarily exposes mtime. Some VFS versions return a zero/epoch
    // creation time even when the modification time is valid, so creation must
    // not prevent this fallback.
    let seconds = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    valid_wall_clock(seconds).then_some(FileTimestamp {
        unix_seconds: seconds,
        kind: TimestampKind::Updated,
    })
}

fn valid_wall_clock(seconds: u64) -> bool {
    // An ESP32 without SNTP commonly starts at the Unix epoch. Do not present
    // that as a real file date.
    seconds >= 1_577_836_800
}

fn wrap_text(content: &str, columns: usize) -> Vec<String> {
    let mut output = Vec::new();
    for physical_line in content.replace('\r', "").split('\n') {
        let characters = physical_line.chars().collect::<Vec<_>>();
        if characters.is_empty() {
            output.push(String::new());
            continue;
        }
        for chunk in characters.chunks(columns) {
            output.push(chunk.iter().collect());
        }
    }
    if output.is_empty() {
        output.push(String::new());
    }
    output
}

pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{} KB", bytes.div_ceil(1024))
    } else {
        format!("{} MB", bytes.div_ceil(1024 * 1024))
    }
}

pub fn format_timestamp(timestamp: Option<FileTimestamp>) -> String {
    let Some(timestamp) = timestamp else {
        return "TIME UNKNOWN".to_owned();
    };
    let seconds = timestamp.unix_seconds as i64;
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let label = match timestamp.kind {
        TimestampKind::Created => "CREATED",
        TimestampKind::Updated => "UPDATED",
        TimestampKind::Contents => "CONTENTS",
    };
    format!("{label} {year:04}-{month:02}-{day:02} {hour:02}:{minute:02}Z")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
