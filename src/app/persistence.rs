//! Stable, application-owned persistence for Vim's global session state.
//!
//! Kernel identities (buffer IDs and text anchors) are deliberately not written
//! to disk. Marks and jumps use a file path plus a text point so the data can be
//! restored in a later process with a different allocation history.

use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

use crate::kernel::{
    Editor,
    buffer::registers::{Register, RegisterKind, RegisterName},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedRegister {
    pub text: String,
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedPosition {
    pub path: String,
    pub row: u32,
    pub column: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistentState {
    #[serde(default)]
    pub registers: BTreeMap<String, PersistedRegister>,
    #[serde(default)]
    pub buffer_marks: BTreeMap<String, BTreeMap<String, PersistedPosition>>,
    #[serde(default)]
    pub global_marks: BTreeMap<String, PersistedPosition>,
    #[serde(default)]
    pub jump_list: Vec<PersistedPosition>,
    #[serde(default)]
    pub command_history: Vec<String>,
    #[serde(default)]
    pub search_history: Vec<String>,
}

#[derive(Debug)]
pub enum PersistenceError {
    Read {
        path: String,
        source: std::io::Error,
    },
    Write {
        path: String,
        source: std::io::Error,
    },
    Parse(toml::de::Error),
    Encode(toml::ser::Error),
}

impl std::fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "could not read persistence file {path}: {source}")
            }
            Self::Write { path, source } => {
                write!(f, "could not write persistence file {path}: {source}")
            }
            Self::Parse(error) => write!(f, "could not parse persistence file: {error}"),
            Self::Encode(error) => write!(f, "could not encode persistence file: {error}"),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<toml::de::Error> for PersistenceError {
    fn from(error: toml::de::Error) -> Self {
        Self::Parse(error)
    }
}

impl From<toml::ser::Error> for PersistenceError {
    fn from(error: toml::ser::Error) -> Self {
        Self::Encode(error)
    }
}

pub fn capture(
    editor: &Editor,
    command_history: &[String],
    search_history: &[String],
) -> PersistentState {
    let registers = editor
        .persistent_registers()
        .into_iter()
        .filter(|(name, _)| *name != RegisterName::BlackHole)
        .map(|(name, register)| {
            let kind = match register.kind {
                RegisterKind::Character => "character",
                RegisterKind::Line => "line",
                RegisterKind::Block => "block",
            };
            (
                name.to_char().to_string(),
                PersistedRegister {
                    text: register.text,
                    kind: kind.into(),
                },
            )
        })
        .collect();
    PersistentState {
        registers,
        command_history: command_history.to_vec(),
        search_history: search_history.to_vec(),
        ..PersistentState::default()
    }
}

pub fn restore(editor: &mut Editor, state: &PersistentState) {
    editor.restore_persistent_registers(state.registers.iter().filter_map(|(name, register)| {
        let name = RegisterName::from_char(name.chars().next()?)?;
        let kind = match register.kind.as_str() {
            "line" => RegisterKind::Line,
            "block" => RegisterKind::Block,
            _ => RegisterKind::Character,
        };
        Some((
            name,
            Register {
                text: register.text.clone(),
                kind,
            },
        ))
    }));
}

pub fn load(path: &Path) -> Result<PersistentState, PersistenceError> {
    let path_string = path.display().to_string();
    let contents = std::fs::read_to_string(path).map_err(|source| PersistenceError::Read {
        path: path_string,
        source,
    })?;
    Ok(toml::from_str(&contents)?)
}

pub fn save(path: &Path, state: &PersistentState) -> Result<(), PersistenceError> {
    let contents = toml::to_string_pretty(state)?;
    let path_string = path.display().to_string();
    let temporary = path.with_extension("tmp");
    std::fs::write(&temporary, contents).map_err(|source| PersistenceError::Write {
        path: temporary.display().to_string(),
        source,
    })?;
    std::fs::rename(&temporary, path).map_err(|source| PersistenceError::Write {
        path: path_string,
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_and_restore_registers_across_editors() {
        let mut first = Editor::new("");
        first.restore_persistent_registers([(
            RegisterName::Named('a'),
            Register {
                text: "kept".into(),
                kind: RegisterKind::Line,
            },
        )]);
        let state = capture(&first, &["write".into()], &["needle".into()]);
        let mut second = Editor::new("");
        restore(&mut second, &state);

        let restored = second.persistent_registers();
        assert!(restored.iter().any(|(name, register)| {
            *name == RegisterName::Named('a')
                && register.text == "kept"
                && register.kind == RegisterKind::Line
        }));
        assert_eq!(state.command_history, ["write"]);
        assert_eq!(state.search_history, ["needle"]);
    }

    #[test]
    fn round_trip_preserves_global_state() {
        let mut state = PersistentState::default();
        state.registers.insert(
            "unnamed".into(),
            PersistedRegister {
                text: "hello".into(),
                kind: "character".into(),
            },
        );
        state.global_marks.insert(
            "A".into(),
            PersistedPosition {
                path: "/tmp/a".into(),
                row: 4,
                column: 2,
            },
        );
        state.jump_list.push(PersistedPosition {
            path: "/tmp/b".into(),
            row: 1,
            column: 7,
        });
        state.command_history.push("write".into());

        let encoded = toml::to_string(&state).unwrap();
        let decoded: PersistentState = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded, state);
    }
}
