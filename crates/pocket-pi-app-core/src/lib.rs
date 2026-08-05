#![no_std]

extern crate alloc;

use alloc::{string::String, vec::Vec};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum View {
    #[default]
    Chat,
    Workspace,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    pub id: u64,
    pub role: Role,
    pub text: String,
    pub streaming: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChatState {
    pub turns: Vec<Turn>,
    pub busy: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileEntry {
    pub name: String,
    pub size: u64,
    pub modified_unix_seconds: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpenFile {
    pub name: String,
    pub content: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub path: String,
    pub entries: Vec<FileEntry>,
    pub open_file: Option<OpenFile>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SystemState {
    pub backend: String,
    pub network: String,
    pub free_ram_kib: u32,
    pub fps: u16,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppSnapshot {
    pub revision: u64,
    pub active_view: View,
    pub chat: ChatState,
    pub workspace: WorkspaceState,
    pub system: SystemState,
}

impl AppSnapshot {
    pub fn changed(&mut self) {
        self.revision = self.revision.saturating_add(1);
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppCommand {
    SwitchView { view: View },
    SendPrompt { text: String },
    OpenPath { name: String },
    CloseFile,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostMessage {
    Snapshot { version: u16, snapshot: AppSnapshot },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GuestMessage {
    Command { version: u16, command: AppCommand },
}

pub fn encode_snapshot(snapshot: &AppSnapshot) -> Result<String, serde_json::Error> {
    serde_json::to_string(&HostMessage::Snapshot {
        version: PROTOCOL_VERSION,
        snapshot: snapshot.clone(),
    })
}

pub fn decode_command(line: &str) -> Result<AppCommand, serde_json::Error> {
    match serde_json::from_str::<GuestMessage>(line)? {
        GuestMessage::Command { command, .. } => Ok(command),
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;

    #[test]
    fn command_round_trips() {
        let message = GuestMessage::Command {
            version: PROTOCOL_VERSION,
            command: AppCommand::OpenPath {
                name: "memory.md".into(),
            },
        };
        let json = serde_json::to_string(&message).unwrap();
        assert_eq!(
            decode_command(&json).unwrap(),
            AppCommand::OpenPath {
                name: "memory.md".into()
            }
        );
    }

    #[test]
    fn snapshot_uses_a_versioned_envelope() {
        let json = encode_snapshot(&AppSnapshot::default()).unwrap();
        assert!(json.contains("\"version\":1"));
        assert!(json.contains("\"type\":\"snapshot\""));
    }
}
