use std::collections::HashMap;

use crate::ast::ExCommand;
use crate::bytecode::BytecodeModule;
use crate::runtime::Value;
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
    pub buffer: Option<u64>,
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
            let buffer_matches = handler.buffer.is_none_or(|buffer| {
                event
                    .payload
                    .get("abuf")
                    .and_then(|value| match value {
                        Value::Integer(value) => Some(*value),
                        _ => None,
                    })
                    .is_some_and(|value| value == buffer as i64)
            });
            let matches = (allow_non_nested || handler.nested)
                && buffer_matches
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
            buffer: None,
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
    fn buffer_scoped_handlers_match_only_their_stable_buffer_id() {
        let mut bus = EventBus::default();
        let mut scoped = handler(1, "*", None, false);
        scoped.buffer = Some(7);
        bus.register(scoped);

        let mut matching = event("notes.txt");
        matching.payload.insert("abuf".into(), Value::Integer(7));
        assert_eq!(bus.handlers_for(&matching).len(), 1);
        assert!(bus.handlers_for(&event("notes.txt")).is_empty());

        let mut other = event("notes.txt");
        other.payload.insert("abuf".into(), Value::Integer(8));
        assert!(bus.handlers_for(&other).is_empty());
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
}
