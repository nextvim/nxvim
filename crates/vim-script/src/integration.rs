use std::collections::HashMap;

use crate::ast::ExCommand;
use crate::bytecode::BytecodeModule;
use crate::runtime::{RuntimeError, RuntimeErrorKind, RuntimeResult, Value};
pub use vim_input::{
    Mapping as CompiledMapping, MappingExpansion, MappingId, MappingStore as KeymapStore,
    SharedMappingStore as SharedKeymapStore,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EventHandlerId(pub u64);

#[derive(Clone, Debug)]
pub struct Event {
    pub name: String,
    pub pattern: Option<String>,
    pub payload: HashMap<String, Value>,
}

#[derive(Clone, Debug)]
pub struct EventHandler {
    pub id: EventHandlerId,
    pub group: Option<String>,
    pub event: String,
    pub patterns: Vec<String>,
    pub action: EventAction,
    pub once: bool,
    pub nested: bool,
}

#[derive(Clone, Debug)]
pub enum EventAction {
    Bytecode(BytecodeModule),
    Command(ExCommand),
}

#[derive(Clone, Debug, Default)]
pub struct EventBus {
    pub handlers: HashMap<String, Vec<EventHandler>>,
}

impl EventBus {
    /// Registers a handler. Registering an existing id replaces its old registration.
    pub fn register(&mut self, handler: EventHandler) {
        for handlers in self.handlers.values_mut() {
            handlers.retain(|existing| existing.id != handler.id);
        }
        self.handlers
            .entry(handler.event.clone())
            .or_default()
            .push(handler);
        self.handlers.retain(|_, handlers| !handlers.is_empty());
    }

    /// Removes handlers in `group` whose event and pattern match the
    /// selective `:autocmd!` form.
    pub fn remove_matching(
        &mut self,
        group: Option<&str>,
        events: Option<&[&str]>,
        patterns: Option<&[&str]>,
    ) -> usize {
        let mut removed = 0;
        for handlers in self.handlers.values_mut() {
            let before = handlers.len();
            handlers.retain(|handler| {
                let group_matches =
                    group.is_none_or(|group| handler.group.as_deref() == Some(group));
                let event_matches = events.is_none_or(|events| {
                    events
                        .iter()
                        .any(|event| *event == "*" || *event == handler.event)
                });
                let pattern_matches = patterns.is_none_or(|patterns| {
                    patterns.iter().any(|pattern| {
                        handler
                            .patterns
                            .iter()
                            .any(|registered| registered == pattern)
                    })
                });
                !(group_matches && event_matches && pattern_matches)
            });
            removed += before - handlers.len();
        }
        self.handlers.retain(|_, handlers| !handlers.is_empty());
        removed
    }

    /// Removes all handlers in `group`, returning the number removed.
    pub fn remove_group(&mut self, group: &str) -> usize {
        let mut removed = 0;
        for handlers in self.handlers.values_mut() {
            let before = handlers.len();
            handlers.retain(|handler| handler.group.as_deref() != Some(group));
            removed += before - handlers.len();
        }
        self.handlers.retain(|_, handlers| !handlers.is_empty());
        removed
    }

    /// Returns matching handlers in registration order and consumes `once` handlers.
    pub fn handlers_for(&mut self, event: &Event) -> Vec<EventHandler> {
        self.handlers_for_with_nesting(event, true)
    }

    /// Returns handlers eligible at the current nesting level. Nested event
    /// delivery admits only handlers explicitly marked `++nested`.
    pub fn handlers_for_with_nesting(
        &mut self,
        event: &Event,
        allow_non_nested: bool,
    ) -> Vec<EventHandler> {
        let Some(handlers) = self.handlers.get_mut(&event.name) else {
            return Vec::new();
        };
        let subject = event.pattern.as_deref().unwrap_or("");
        let mut matching = Vec::new();
        handlers.retain(|handler| {
            let matches = (allow_non_nested || handler.nested)
                && (handler.patterns.is_empty()
                    || handler
                        .patterns
                        .iter()
                        .any(|pattern| pattern_matches_subject(pattern, subject)));
            if matches {
                matching.push(handler.clone());
            }
            !(matches && handler.once)
        });
        if handlers.is_empty() {
            self.handlers.remove(&event.name);
        }
        matching
    }
}

fn pattern_matches_subject(pattern: &str, subject: &str) -> bool {
    if pattern.contains('/') {
        vim_glob_matches(pattern, subject)
    } else {
        let tail = subject.rsplit('/').next().unwrap_or(subject);
        vim_glob_matches(pattern, tail)
    }
}

fn vim_glob_matches(pattern: &str, subject: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let subject: Vec<char> = subject.chars().collect();
    let (mut pattern_index, mut subject_index) = (0, 0);
    let (mut star, mut star_subject) = (None, 0);

    while subject_index < subject.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?' || pattern[pattern_index] == subject[subject_index])
        {
            pattern_index += 1;
            subject_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star = Some(pattern_index);
            pattern_index += 1;
            star_subject = subject_index;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            star_subject += 1;
            subject_index = star_subject;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

#[derive(Clone, Debug)]
pub struct OptionDefinition {
    pub name: String,
    pub short_name: Option<String>,
    pub kind: OptionKind,
    pub scope: OptionValueScope,
    pub default: Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionKind {
    Boolean,
    Number,
    String,
    StringList,
    Flags,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionValueScope {
    Global,
    Buffer,
    Window,
    GlobalBuffer,
    GlobalWindow,
}

#[derive(Clone, Debug, Default)]
pub struct OptionStore {
    pub definitions: HashMap<String, OptionDefinition>,
    pub global: HashMap<String, Value>,
    pub buffers: HashMap<u64, HashMap<String, Value>>,
    pub windows: HashMap<u64, HashMap<String, Value>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptionScope {
    Global,
    Buffer(u64),
    Window(u64),
}

impl OptionStore {
    pub fn define(&mut self, definition: OptionDefinition) -> RuntimeResult<()> {
        validate_option_value(definition.kind, &definition.default)?;
        if definition.name.is_empty() {
            return Err(option_error(
                "E_OPTION_NAME",
                RuntimeErrorKind::NameError,
                "option name cannot be empty",
            ));
        }
        if self.definitions.contains_key(&definition.name)
            || definition.short_name.as_ref().is_some_and(|short| {
                self.definitions.values().any(|existing| {
                    existing.name == *short || existing.short_name.as_ref() == Some(short)
                })
            })
        {
            return Err(option_error(
                "E_OPTION_EXISTS",
                RuntimeErrorKind::NameError,
                format!("option '{}' is already defined", definition.name),
            ));
        }
        self.global
            .insert(definition.name.clone(), definition.default.clone());
        self.definitions.insert(definition.name.clone(), definition);
        Ok(())
    }

    pub fn get(&self, name: &str, scope: OptionScope) -> RuntimeResult<&Value> {
        let definition = self.definition(name)?;
        validate_scope(definition, scope)?;
        let name = definition.name.as_str();
        let local = match scope {
            OptionScope::Global => None,
            OptionScope::Buffer(buffer) => self.buffers.get(&buffer).and_then(|map| map.get(name)),
            OptionScope::Window(window) => self.windows.get(&window).and_then(|map| map.get(name)),
        };
        local.or_else(|| self.global.get(name)).ok_or_else(|| {
            option_error(
                "E_OPTION_STATE",
                RuntimeErrorKind::Internal,
                format!("option '{name}' has no value"),
            )
        })
    }

    pub fn set(&mut self, name: &str, scope: OptionScope, value: Value) -> RuntimeResult<()> {
        let definition = self.definition(name)?;
        validate_scope(definition, scope)?;
        validate_option_value(definition.kind, &value)?;
        let canonical_name = definition.name.clone();
        match scope {
            OptionScope::Global => &mut self.global,
            OptionScope::Buffer(buffer) => self.buffers.entry(buffer).or_default(),
            OptionScope::Window(window) => self.windows.entry(window).or_default(),
        }
        .insert(canonical_name, value);
        Ok(())
    }

    pub fn reset(&mut self, name: &str, scope: OptionScope) -> RuntimeResult<()> {
        let definition = self.definition(name)?;
        validate_scope(definition, scope)?;
        let canonical_name = definition.name.clone();
        let default = definition.default.clone();
        match scope {
            OptionScope::Global => {
                self.global.insert(canonical_name, default);
            }
            OptionScope::Buffer(buffer) => {
                if let Some(values) = self.buffers.get_mut(&buffer) {
                    values.remove(&canonical_name);
                    if values.is_empty() {
                        self.buffers.remove(&buffer);
                    }
                }
            }
            OptionScope::Window(window) => {
                if let Some(values) = self.windows.get_mut(&window) {
                    values.remove(&canonical_name);
                    if values.is_empty() {
                        self.windows.remove(&window);
                    }
                }
            }
        }
        Ok(())
    }

    fn definition(&self, name: &str) -> RuntimeResult<&OptionDefinition> {
        self.definitions
            .get(name)
            .or_else(|| {
                self.definitions
                    .values()
                    .find(|definition| definition.short_name.as_deref() == Some(name))
            })
            .ok_or_else(|| {
                option_error(
                    "E_UNKNOWN_OPTION",
                    RuntimeErrorKind::NameError,
                    format!("unknown option '{name}'"),
                )
            })
    }
}

fn validate_scope(definition: &OptionDefinition, requested: OptionScope) -> RuntimeResult<()> {
    let valid = matches!(
        (definition.scope, requested),
        (OptionValueScope::Global, OptionScope::Global)
            | (OptionValueScope::Buffer, OptionScope::Buffer(_))
            | (OptionValueScope::Window, OptionScope::Window(_))
            | (
                OptionValueScope::GlobalBuffer,
                OptionScope::Global | OptionScope::Buffer(_)
            )
            | (
                OptionValueScope::GlobalWindow,
                OptionScope::Global | OptionScope::Window(_)
            )
    );
    if valid {
        Ok(())
    } else {
        Err(option_error(
            "E_OPTION_SCOPE",
            RuntimeErrorKind::InvalidCommand,
            format!("invalid scope for option '{}'", definition.name),
        ))
    }
}

fn validate_option_value(kind: OptionKind, value: &Value) -> RuntimeResult<()> {
    let valid = match (kind, value) {
        (OptionKind::Boolean, Value::Bool(_))
        | (OptionKind::Number, Value::Integer(_))
        | (OptionKind::String | OptionKind::Flags, Value::String(_)) => true,
        (OptionKind::StringList, Value::List(values)) => {
            values.iter().all(|value| matches!(value, Value::String(_)))
        }
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(option_error(
            "E_OPTION_TYPE",
            RuntimeErrorKind::TypeError,
            format!("value does not match option kind {kind:?}"),
        ))
    }
}

fn option_error(
    code: &'static str,
    kind: RuntimeErrorKind,
    message: impl Into<String>,
) -> RuntimeError {
    RuntimeError::coded(code, kind, message)
}

#[derive(Clone, Debug, Default)]
pub struct ModuleCache {
    pub modules: HashMap<ModuleCacheKey, BytecodeModule>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ModuleCacheKey {
    pub source_name: String,
    pub content_hash: u64,
    pub language_version: LanguageVersion,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LanguageVersion {
    Legacy,
    Vim9,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::resolver::FunctionId;
    use crate::source::SourceId;

    fn module() -> BytecodeModule {
        BytecodeModule {
            source: SourceId(0),
            entrypoint: FunctionId(0),
            functions: Vec::new(),
        }
    }

    fn handler(id: u64, pattern: &str, group: Option<&str>, once: bool) -> EventHandler {
        EventHandler {
            id: EventHandlerId(id),
            group: group.map(str::to_owned),
            event: "BufWrite".into(),
            patterns: vec![pattern.into()],
            action: EventAction::Bytecode(module()),
            once,
            nested: false,
        }
    }

    fn event(pattern: &str) -> Event {
        Event {
            name: "BufWrite".into(),
            pattern: Some(pattern.into()),
            payload: HashMap::new(),
        }
    }

    fn definition(
        name: &str,
        kind: OptionKind,
        scope: OptionValueScope,
        default: Value,
    ) -> OptionDefinition {
        OptionDefinition {
            name: name.into(),
            short_name: None,
            kind,
            scope,
            default,
        }
    }

    #[test]
    fn event_patterns_once_and_groups() {
        let mut bus = EventBus::default();
        bus.register(handler(1, "*.rs", Some("rust"), true));
        bus.register(handler(2, "src/?.rs", Some("rust"), false));
        bus.register(handler(3, "*.txt", Some("text"), false));

        let first = bus.handlers_for(&event("src/a.rs"));
        assert_eq!(
            first.iter().map(|handler| handler.id.0).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(bus.handlers_for(&event("src/a.rs")).len(), 1);
        assert_eq!(bus.remove_group("rust"), 1);
        assert!(bus.handlers_for(&event("src/a.rs")).is_empty());
        assert_eq!(bus.remove_group("text"), 1);
    }

    #[test]
    fn registering_an_event_id_replaces_its_previous_registration() {
        let mut bus = EventBus::default();
        bus.register(handler(1, "*.rs", None, false));
        bus.register(handler(1, "*.txt", None, false));
        assert!(bus.handlers_for(&event("main.rs")).is_empty());
        assert_eq!(bus.handlers_for(&event("notes.txt")).len(), 1);
    }

    #[test]
    fn file_patterns_use_tail_or_full_path_subjects() {
        let mut bus = EventBus::default();
        bus.register(handler(1, "*.rs", None, false));
        bus.register(handler(2, "src/*.rs", None, false));

        let matching = bus.handlers_for(&event("src/main.rs"));
        assert_eq!(
            matching
                .iter()
                .map(|handler| handler.id.0)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert_eq!(bus.handlers_for(&event("/work/src/main.rs")).len(), 1);
        assert!(bus.handlers_for(&event("/work/src/main.txt")).is_empty());
    }

    #[test]
    fn selective_event_removal_preserves_unmatched_handlers() {
        let mut bus = EventBus::default();
        bus.register(handler(1, "*.rs", Some("rust"), false));
        bus.register(handler(2, "*.txt", Some("rust"), false));
        bus.register(handler(3, "*.rs", Some("other"), false));

        assert_eq!(
            bus.remove_matching(Some("rust"), Some(&["BufWrite"]), Some(&["*.rs"])),
            1
        );
        assert_eq!(bus.handlers_for(&event("main.rs")).len(), 1);
        assert_eq!(bus.handlers_for(&event("main.txt")).len(), 1);
    }

    #[test]
    fn nested_dispatch_only_admits_nested_handlers() {
        let mut bus = EventBus::default();
        bus.register(handler(1, "*", None, false));
        let mut nested = handler(2, "*", None, false);
        nested.nested = true;
        bus.register(nested);

        let nested_handlers = bus.handlers_for_with_nesting(&event("main.rs"), false);
        assert_eq!(
            nested_handlers
                .iter()
                .map(|handler| handler.id.0)
                .collect::<Vec<_>>(),
            vec![2]
        );
    }

    #[test]
    fn options_validate_types_and_scopes_and_reset_to_fallback() {
        let mut store = OptionStore::default();
        store
            .define(definition(
                "number",
                OptionKind::Number,
                OptionValueScope::Global,
                Value::Integer(1),
            ))
            .unwrap();
        store
            .define(definition(
                "words",
                OptionKind::StringList,
                OptionValueScope::GlobalBuffer,
                Value::List(vec![Value::String(Arc::from("default"))]),
            ))
            .unwrap();

        let error = store
            .set("number", OptionScope::Global, Value::Bool(true))
            .unwrap_err();
        assert!(matches!(error.kind, RuntimeErrorKind::TypeError));
        let error = store.get("number", OptionScope::Buffer(1)).unwrap_err();
        assert!(matches!(error.kind, RuntimeErrorKind::InvalidCommand));

        let local = Value::List(vec![Value::String(Arc::from("local"))]);
        store
            .set("words", OptionScope::Buffer(1), local.clone())
            .unwrap();
        assert_eq!(store.get("words", OptionScope::Buffer(1)).unwrap(), &local);
        store.reset("words", OptionScope::Buffer(1)).unwrap();
        assert_eq!(
            store.get("words", OptionScope::Buffer(1)).unwrap(),
            &Value::List(vec![Value::String(Arc::from("default"))])
        );
    }

    #[test]
    fn options_support_short_names_and_reject_invalid_defaults() {
        let mut store = OptionStore::default();
        let mut option = definition(
            "enabled",
            OptionKind::Boolean,
            OptionValueScope::Global,
            Value::Bool(false),
        );
        option.short_name = Some("en".into());
        store.define(option).unwrap();
        store
            .set("en", OptionScope::Global, Value::Bool(true))
            .unwrap();
        assert_eq!(
            store.get("enabled", OptionScope::Global).unwrap(),
            &Value::Bool(true)
        );

        let error = store
            .define(definition(
                "bad",
                OptionKind::Boolean,
                OptionValueScope::Global,
                Value::Integer(0),
            ))
            .unwrap_err();
        assert!(matches!(error.kind, RuntimeErrorKind::TypeError));
    }
}
