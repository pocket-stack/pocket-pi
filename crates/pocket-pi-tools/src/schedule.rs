use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_NAME_BYTES: usize = 64;
const MAX_PROMPT_BYTES: usize = 2_000;
const MAX_DELAY_SECONDS: u64 = 366 * 24 * 60 * 60;

#[derive(Clone, Debug)]
struct Wake {
    name: String,
    prompt: String,
    run_at_unix_seconds: u64,
    every_seconds: Option<u64>,
}

#[derive(Clone, Default)]
struct ScheduleState {
    wakes: Vec<Wake>,
}

#[derive(Clone, Debug, Default)]
pub struct ScheduleProjection {
    pub name: Option<String>,
    pub prompt: String,
    pub next_in_seconds: Option<u64>,
    pub every_minutes: Option<u64>,
}

pub struct ScheduledWake {
    pub name: String,
    pub prompt: String,
}

/// Persistent generic wake scheduler. It knows only absolute and fixed-interval
/// time; the Pi Agent owns all domain decisions and may replace any named wake.
pub struct ScheduleStore {
    path: PathBuf,
    state: Mutex<ScheduleState>,
}

impl ScheduleStore {
    pub fn load(workspace_root: &Path) -> Self {
        let directory = workspace_root.join(".pi-agent");
        let path = directory.join("schedule.json");
        let _ = std::fs::create_dir_all(&directory);
        // The previous recurring format is intentionally removed, not migrated.
        let _ = std::fs::remove_file(directory.join("routines.json"));
        let state = match read_state(&path) {
            Ok(state) => state,
            Err(error) => {
                log::warn!("Ignoring invalid schedule state: {error}");
                ScheduleState::default()
            }
        };
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    pub fn definitions() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({
                "name":"schedule.set",
                "description":"Create or replace a persistent agent wake by name. For a one-time wake provide exactly one of run_at_unix_seconds or delay_seconds. For a recurring wake provide every_minutes and optionally start_at_unix_seconds. The agent may replace the same name at any time to adjust its schedule.",
                "parameters":{"type":"object","properties":{
                    "name":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z0-9._-]+$"},
                    "prompt":{"type":"string","minLength":1,"maxLength":2000},
                    "run_at_unix_seconds":{"type":"integer","minimum":1},
                    "delay_seconds":{"type":"integer","minimum":1,"maximum":31622400},
                    "every_minutes":{"type":"integer","minimum":1,"maximum":10080},
                    "start_at_unix_seconds":{"type":"integer","minimum":1}
                },"required":["name","prompt"],"additionalProperties":false}
            }),
            serde_json::json!({
                "name":"schedule.list",
                "description":"List all pending one-time and recurring agent wakes, ordered by next run time.",
                "parameters":{"type":"object","properties":{},"additionalProperties":false}
            }),
            serde_json::json!({
                "name":"schedule.cancel",
                "description":"Cancel one pending agent wake by name.",
                "parameters":{"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":64}},"required":["name"],"additionalProperties":false}
            }),
            serde_json::json!({
                "name":"schedule.clear",
                "description":"Cancel every pending agent wake.",
                "parameters":{"type":"object","properties":{},"additionalProperties":false}
            }),
        ]
    }

    pub fn execute(
        &self,
        name: &str,
        args: &serde_json::Value,
    ) -> Result<Option<serde_json::Value>, String> {
        match name {
            "schedule.set" => self.set(args).map(Some),
            "schedule.list" => self.list().map(Some),
            "schedule.cancel" => self.cancel(args).map(Some),
            "schedule.clear" => self.clear().map(Some),
            _ => Ok(None),
        }
    }

    pub fn claim_due(&self) -> Option<ScheduledWake> {
        let now = crate::clock::unix_seconds().ok()?;
        if !crate::clock::is_synchronized() {
            return None;
        }
        let mut state = self.state.lock().ok()?;
        let index = state
            .wakes
            .iter()
            .enumerate()
            .filter(|(_, wake)| wake.run_at_unix_seconds <= now)
            .min_by_key(|(_, wake)| wake.run_at_unix_seconds)
            .map(|(index, _)| index)?;
        let wake = state.wakes[index].clone();
        if let Some(interval) = wake.every_seconds {
            let mut next = wake.run_at_unix_seconds;
            while next <= now {
                next = next.saturating_add(interval);
            }
            state.wakes[index].run_at_unix_seconds = next;
        } else {
            state.wakes.remove(index);
        }
        if let Err(error) = self.persist(&state) {
            log::error!("Could not persist claimed wake: {error}");
            if wake.every_seconds.is_some() {
                state.wakes[index] = wake;
            } else {
                state.wakes.insert(index, wake);
            }
            return None;
        }
        Some(ScheduledWake {
            name: wake.name,
            prompt: wake.prompt,
        })
    }

    pub fn projection(&self) -> ScheduleProjection {
        let now = crate::clock::unix_seconds().unwrap_or(0);
        let Ok(state) = self.state.lock() else {
            return ScheduleProjection::default();
        };
        let Some(wake) = state
            .wakes
            .iter()
            .min_by_key(|wake| wake.run_at_unix_seconds)
        else {
            return ScheduleProjection::default();
        };
        ScheduleProjection {
            name: Some(wake.name.clone()),
            prompt: wake.prompt.clone(),
            next_in_seconds: Some(wake.run_at_unix_seconds.saturating_sub(now)),
            every_minutes: wake.every_seconds.map(|seconds| seconds / 60),
        }
    }

    fn set(&self, args: &serde_json::Value) -> Result<serde_json::Value, String> {
        ensure_clock()?;
        let name = required_string(args, "name")?;
        validate_name(name)?;
        let prompt = required_string(args, "prompt")?;
        if prompt.is_empty() || prompt.len() > MAX_PROMPT_BYTES {
            return Err(format!("prompt must contain 1 to {MAX_PROMPT_BYTES} bytes"));
        }
        let absolute = args
            .get("run_at_unix_seconds")
            .and_then(serde_json::Value::as_u64);
        let delay = args
            .get("delay_seconds")
            .and_then(serde_json::Value::as_u64);
        let every_minutes = args
            .get("every_minutes")
            .and_then(serde_json::Value::as_u64);
        let start_at = args
            .get("start_at_unix_seconds")
            .and_then(serde_json::Value::as_u64);
        let now = crate::clock::unix_seconds()?;
        let (run_at, every_seconds) = if let Some(minutes) = every_minutes {
            if absolute.is_some() || delay.is_some() {
                return Err("a recurring wake uses every_minutes and optional start_at_unix_seconds, not run_at_unix_seconds or delay_seconds".to_owned());
            }
            if !(1..=10_080).contains(&minutes) {
                return Err("every_minutes must be between 1 and 10080".to_owned());
            }
            let interval = minutes * 60;
            (
                start_at.unwrap_or_else(|| now.saturating_add(interval)),
                Some(interval),
            )
        } else {
            if start_at.is_some() || absolute.is_some() == delay.is_some() {
                return Err(
                    "a one-time wake needs exactly one of run_at_unix_seconds or delay_seconds"
                        .to_owned(),
                );
            }
            let run_at = match (absolute, delay) {
                (Some(run_at), None) => run_at,
                (None, Some(delay)) if (1..=MAX_DELAY_SECONDS).contains(&delay) => {
                    now.saturating_add(delay)
                }
                (None, Some(_)) => {
                    return Err(format!(
                        "delay_seconds must be between 1 and {MAX_DELAY_SECONDS}"
                    ));
                }
                _ => unreachable!(),
            };
            (run_at, None)
        };
        if run_at <= now || run_at > now.saturating_add(MAX_DELAY_SECONDS) {
            return Err(
                "wake time must be in the future and no more than 366 days away".to_owned(),
            );
        }
        let wake = Wake {
            name: name.to_owned(),
            prompt: prompt.to_owned(),
            run_at_unix_seconds: run_at,
            every_seconds,
        };
        let mut state = self
            .state
            .lock()
            .map_err(|_| "schedule state lock was poisoned".to_owned())?;
        let previous = state.clone();
        if let Some(existing) = state.wakes.iter_mut().find(|wake| wake.name == name) {
            *existing = wake;
        } else {
            state.wakes.push(wake);
        }
        state.wakes.sort_by_key(|wake| wake.run_at_unix_seconds);
        if let Err(error) = self.persist(&state) {
            *state = previous;
            return Err(error);
        }
        Ok(serde_json::json!({
            "status":"ok","name":name,"prompt":prompt,
            "runAtUnixSeconds":run_at,"nextRunInSeconds":run_at - now,
            "kind":if every_seconds.is_some() { "recurring" } else { "once" },
            "everyMinutes":every_seconds.map(|seconds| seconds / 60)
        }))
    }

    fn list(&self) -> Result<serde_json::Value, String> {
        let now = crate::clock::unix_seconds().unwrap_or(0);
        let state = self
            .state
            .lock()
            .map_err(|_| "schedule state lock was poisoned".to_owned())?;
        let wakes = state
            .wakes
            .iter()
            .map(|wake| {
                serde_json::json!({
                    "name":wake.name,"prompt":wake.prompt,
                    "runAtUnixSeconds":wake.run_at_unix_seconds,
                    "nextRunInSeconds":wake.run_at_unix_seconds.saturating_sub(now),
                    "kind":if wake.every_seconds.is_some() { "recurring" } else { "once" },
                    "everyMinutes":wake.every_seconds.map(|seconds| seconds / 60)
                })
            })
            .collect::<Vec<_>>();
        Ok(
            serde_json::json!({"status":"ok","synchronized":crate::clock::is_synchronized(),"wakes":wakes}),
        )
    }

    fn cancel(&self, args: &serde_json::Value) -> Result<serde_json::Value, String> {
        let name = required_string(args, "name")?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| "schedule state lock was poisoned".to_owned())?;
        let previous = state.clone();
        let before = state.wakes.len();
        state.wakes.retain(|wake| wake.name != name);
        if state.wakes.len() == before {
            return Err(format!("wake {name:?} does not exist"));
        }
        if let Err(error) = self.persist(&state) {
            *state = previous;
            return Err(error);
        }
        Ok(serde_json::json!({"status":"ok","cancelled":name}))
    }

    fn clear(&self) -> Result<serde_json::Value, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "schedule state lock was poisoned".to_owned())?;
        let previous = state.clone();
        let cancelled = state.wakes.len();
        state.wakes.clear();
        if let Err(error) = self.persist(&state) {
            *state = previous;
            return Err(error);
        }
        Ok(serde_json::json!({"status":"ok","cancelled":cancelled}))
    }

    fn persist(&self, state: &ScheduleState) -> Result<(), String> {
        let value = serde_json::json!({
            "version":1,
            "wakes":state.wakes.iter().map(|wake| serde_json::json!({
                "name":wake.name,"prompt":wake.prompt,
                "run_at_unix_seconds":wake.run_at_unix_seconds,
                "every_seconds":wake.every_seconds
            })).collect::<Vec<_>>()
        });
        let temporary = self.path.with_extension("json.tmp");
        let mut file = std::fs::File::create(&temporary)
            .map_err(|error| format!("create schedule temp file: {error}"))?;
        use std::io::Write;
        file.write_all(value.to_string().as_bytes())
            .map_err(|error| format!("write schedule: {error}"))?;
        file.sync_all()
            .map_err(|error| format!("sync schedule: {error}"))?;
        drop(file);
        std::fs::rename(&temporary, &self.path)
            .map_err(|error| format!("replace schedule: {error}"))
    }
}

fn ensure_clock() -> Result<(), String> {
    if crate::clock::is_synchronized() {
        Ok(())
    } else {
        Err("device clock is not synchronized; connect Wi-Fi and retry".to_owned())
    }
}

fn required_string<'a>(args: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    args.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("{key} must be a string"))
}

fn validate_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(
            "name must be 1-64 ASCII letters, digits, dots, underscores, or hyphens".to_owned(),
        );
    }
    Ok(())
}

fn read_state(path: &Path) -> Result<ScheduleState, String> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ScheduleState::default());
        }
        Err(error) => return Err(format!("read schedule: {error}")),
    };
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| format!("parse schedule: {error}"))?;
    if value.get("version").and_then(serde_json::Value::as_u64) != Some(1) {
        return Err("unsupported schedule version".to_owned());
    }
    let mut wakes = Vec::new();
    for item in value
        .get("wakes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(name) = item.get("name").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(prompt) = item.get("prompt").and_then(serde_json::Value::as_str) else {
            continue;
        };
        let Some(run_at) = item
            .get("run_at_unix_seconds")
            .and_then(serde_json::Value::as_u64)
        else {
            continue;
        };
        if validate_name(name).is_err() || prompt.is_empty() || prompt.len() > MAX_PROMPT_BYTES {
            continue;
        }
        let every_seconds = item
            .get("every_seconds")
            .and_then(serde_json::Value::as_u64);
        if every_seconds
            .is_some_and(|seconds| !(60..=10_080 * 60).contains(&seconds) || seconds % 60 != 0)
        {
            continue;
        }
        wakes.push(Wake {
            name: name.to_owned(),
            prompt: prompt.to_owned(),
            run_at_unix_seconds: run_at,
            every_seconds,
        });
    }
    wakes.sort_by_key(|wake| wake.run_at_unix_seconds);
    Ok(ScheduleState { wakes })
}
