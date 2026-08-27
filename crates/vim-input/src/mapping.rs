use crate::{KeyParseError, KeySequence};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MappingId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MappingMode {
    Normal,
    Visual,
    Select,
    OperatorPending,
    Insert,
    CommandLine,
    LangArg,
    Terminal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MappingScope {
    Global,
    Buffer(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MappingOrigin {
    BuiltIn,
    User,
    Script,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MappingScriptContext {
    pub script_name: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MappingFlags {
    pub non_recursive: bool,
    pub silent: bool,
    pub nowait: bool,
    pub expr: bool,
    pub unique: bool,
    pub script: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MappingExpansion {
    Keys(String),
    Expression(String),
    Script(String),
    NoOp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mapping {
    pub id: MappingId,
    pub modes: Vec<MappingMode>,
    pub lhs: String,
    pub sequence: KeySequence,
    pub expansion: MappingExpansion,
    pub flags: MappingFlags,
    pub scope: MappingScope,
    pub origin: MappingOrigin,
    pub script_context: MappingScriptContext,
}

impl Mapping {
    pub fn new(
        id: MappingId,
        modes: Vec<MappingMode>,
        lhs: String,
        expansion: MappingExpansion,
        flags: MappingFlags,
        scope: MappingScope,
        origin: MappingOrigin,
        script_context: MappingScriptContext,
    ) -> Result<Self, KeyParseError> {
        let sequence = KeySequence::parse(&lhs)?;
        Ok(Self {
            id,
            modes,
            lhs,
            sequence,
            expansion,
            flags,
            scope,
            origin,
            script_context,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct MappingStore {
    global: HashMap<(MappingMode, KeySequence), Mapping>,
    buffer_local: HashMap<u64, HashMap<(MappingMode, KeySequence), Mapping>>,
}

pub type SharedMappingStore = Arc<RwLock<MappingStore>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MappingMatch {
    Complete(Mapping),
    /// An exact mapping exists, but a longer mapping is also possible.
    /// The resolver must wait for another key unless the exact mapping is `nowait`.
    CompleteWithPrefix(Mapping),
    Prefix,
    None,
}

impl MappingStore {
    pub fn register(&mut self, mapping: Mapping) {
        let target = match mapping.scope {
            MappingScope::Buffer(buffer) => self.buffer_local.entry(buffer).or_default(),
            MappingScope::Global => &mut self.global,
        };
        for mode in &mapping.modes {
            target.insert((*mode, mapping.sequence.clone()), mapping.clone());
        }
    }

    pub fn unmap(
        &mut self,
        mode: MappingMode,
        lhs: &str,
        buffer: Option<u64>,
    ) -> Result<Option<Mapping>, KeyParseError> {
        let key = (mode, KeySequence::parse(lhs)?);
        Ok(match buffer {
            Some(buffer) => {
                let mappings = match self.buffer_local.get_mut(&buffer) {
                    Some(mappings) => mappings,
                    None => return Ok(None),
                };
                let removed = mappings.remove(&key);
                if mappings.is_empty() {
                    self.buffer_local.remove(&buffer);
                }
                removed
            }
            None => self.global.remove(&key),
        })
    }

    pub fn resolve(
        &self,
        mode: MappingMode,
        lhs: &str,
        buffer: Option<u64>,
    ) -> Result<Option<&Mapping>, KeyParseError> {
        let sequence = KeySequence::parse(lhs)?;
        Ok(self.resolve_sequence(mode, &sequence, buffer))
    }

    pub fn match_keys(
        &self,
        mode: MappingMode,
        keys: &[crate::Key],
        buffer: Option<u64>,
    ) -> MappingMatch {
        let exact = KeySequence {
            items: keys.iter().copied().map(crate::KeyPattern::Exact).collect(),
        };
        if let Some(mapping) = self.resolve_sequence(mode, &exact, buffer) {
            let has_longer = self.has_longer_prefix(mode, keys, buffer);
            return if has_longer && !mapping.flags.nowait {
                MappingMatch::CompleteWithPrefix(mapping.clone())
            } else {
                MappingMatch::Complete(mapping.clone())
            };
        }
        let has_prefix = self
            .global
            .keys()
            .chain(
                buffer
                    .and_then(|id| self.buffer_local.get(&id))
                    .into_iter()
                    .flat_map(|store| store.keys()),
            )
            .any(|(candidate_mode, sequence)| {
                *candidate_mode == mode
                    && sequence.items.len() > keys.len()
                    && sequence.items[..keys.len()]
                        .iter()
                        .zip(keys)
                        .all(|(pattern, key)| *pattern == crate::KeyPattern::Exact(*key))
            });
        if has_prefix {
            MappingMatch::Prefix
        } else {
            MappingMatch::None
        }
    }

    fn has_longer_prefix(
        &self,
        mode: MappingMode,
        keys: &[crate::Key],
        buffer: Option<u64>,
    ) -> bool {
        self.global
            .keys()
            .chain(
                buffer
                    .and_then(|id| self.buffer_local.get(&id))
                    .into_iter()
                    .flat_map(|store| store.keys()),
            )
            .any(|(candidate_mode, sequence)| {
                *candidate_mode == mode
                    && sequence.items.len() > keys.len()
                    && sequence.items[..keys.len()]
                        .iter()
                        .zip(keys)
                        .all(|(pattern, key)| *pattern == crate::KeyPattern::Exact(*key))
            })
    }

    pub fn resolve_sequence(
        &self,
        mode: MappingMode,
        sequence: &KeySequence,
        buffer: Option<u64>,
    ) -> Option<&Mapping> {
        let key = (mode, sequence.clone());
        buffer
            .and_then(|buffer| self.buffer_local.get(&buffer))
            .and_then(|mappings| mappings.get(&key))
            .or_else(|| self.global.get(&key))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapping(id: u64, mode: MappingMode, scope: MappingScope) -> Mapping {
        Mapping::new(
            MappingId(id),
            vec![mode],
            "x".into(),
            MappingExpansion::NoOp,
            MappingFlags::default(),
            scope,
            MappingOrigin::Script,
            MappingScriptContext {
                script_name: Some("test.vim".into()),
            },
        )
        .unwrap()
    }

    #[test]
    fn buffer_local_mappings_precede_global_mappings_by_mode() {
        let mut store = MappingStore::default();
        store.register(mapping(1, MappingMode::Normal, MappingScope::Global));
        store.register(mapping(2, MappingMode::Normal, MappingScope::Buffer(7)));
        store.register(mapping(3, MappingMode::Insert, MappingScope::Global));

        assert_eq!(
            store
                .resolve(MappingMode::Normal, "x", Some(7))
                .unwrap()
                .unwrap()
                .id
                .0,
            2
        );
        assert_eq!(
            store
                .resolve(MappingMode::Normal, "x", Some(8))
                .unwrap()
                .unwrap()
                .id
                .0,
            1
        );
        assert_eq!(
            store
                .resolve(MappingMode::Insert, "x", Some(7))
                .unwrap()
                .unwrap()
                .id
                .0,
            3
        );
        assert!(
            store
                .resolve(MappingMode::Visual, "x", Some(7))
                .unwrap()
                .is_none()
        );

        assert_eq!(
            store
                .unmap(MappingMode::Normal, "x", Some(7))
                .unwrap()
                .unwrap()
                .id
                .0,
            2
        );
        assert_eq!(
            store
                .resolve(MappingMode::Normal, "x", Some(7))
                .unwrap()
                .unwrap()
                .id
                .0,
            1
        );
    }

    #[test]
    fn non_nowait_exact_mapping_reports_ambiguous_longer_prefix() {
        let mut store = MappingStore::default();
        store.register(
            Mapping::new(
                MappingId(1),
                vec![MappingMode::Normal],
                "x".into(),
                MappingExpansion::NoOp,
                MappingFlags::default(),
                MappingScope::Global,
                MappingOrigin::Script,
                MappingScriptContext::default(),
            )
            .unwrap(),
        );
        store.register(
            Mapping::new(
                MappingId(2),
                vec![MappingMode::Normal],
                "xy".into(),
                MappingExpansion::NoOp,
                MappingFlags::default(),
                MappingScope::Global,
                MappingOrigin::Script,
                MappingScriptContext::default(),
            )
            .unwrap(),
        );
        assert!(matches!(
            store.match_keys(MappingMode::Normal, &[crate::Key::char('x')], None),
            MappingMatch::CompleteWithPrefix(mapping) if mapping.id == MappingId(1)
        ));
    }

    #[test]
    fn nowait_exact_mapping_wins_over_longer_prefix() {
        let mut store = MappingStore::default();
        let mut flags = MappingFlags::default();
        flags.nowait = true;
        store.register(
            Mapping::new(
                MappingId(1),
                vec![MappingMode::Normal],
                "x".into(),
                MappingExpansion::NoOp,
                flags,
                MappingScope::Global,
                MappingOrigin::Script,
                MappingScriptContext::default(),
            )
            .unwrap(),
        );
        store.register(
            Mapping::new(
                MappingId(2),
                vec![MappingMode::Normal],
                "xy".into(),
                MappingExpansion::NoOp,
                MappingFlags::default(),
                MappingScope::Global,
                MappingOrigin::Script,
                MappingScriptContext::default(),
            )
            .unwrap(),
        );
        assert!(matches!(
            store.match_keys(MappingMode::Normal, &[crate::Key::char('x')], None),
            MappingMatch::Complete(mapping) if mapping.id == MappingId(1)
        ));
    }

    #[test]
    fn mapping_preserves_flags_origin_scope_and_script_context() {
        let mapping = Mapping::new(
            MappingId(9),
            vec![MappingMode::Normal],
            "<leader>w".into(),
            MappingExpansion::Keys(":write<CR>".into()),
            MappingFlags {
                non_recursive: true,
                silent: true,
                ..MappingFlags::default()
            },
            MappingScope::Buffer(4),
            MappingOrigin::Script,
            MappingScriptContext {
                script_name: Some("plugin/example.vim".into()),
            },
        )
        .unwrap();

        assert_eq!(mapping.id, MappingId(9));
        assert_eq!(mapping.scope, MappingScope::Buffer(4));
        assert_eq!(mapping.origin, MappingOrigin::Script);
        assert!(mapping.flags.non_recursive && mapping.flags.silent);
        assert_eq!(
            mapping.script_context.script_name.as_deref(),
            Some("plugin/example.vim")
        );
        assert!(!mapping.sequence.is_empty());
    }
}
