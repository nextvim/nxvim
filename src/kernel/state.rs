use super::{
    BufferId, CommandContext, CommandKind, CommandLineRequest, TabPageId, WindowId, Windows,
};
use crate::app::legacy_command::Command;
use crate::model::Buffers;
use std::path::Path;

/// Stable location of the editor's current context.
///
/// This contains IDs, not borrowed references, so it remains valid to pass
/// through command, event, and script boundaries after validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorContext {
    pub tab: TabPageId,
    pub window: WindowId,
    pub buffer: BufferId,
}

/// Kernel-owned semantic editor state.
///
/// Buffer storage is owned here. The existing `EditorModel` remains a
/// compatibility façade while command handlers migrate to this owner.
pub struct EditorState {
    buffers: Buffers,
    windows: Windows,
    events: super::EventQueue,
    current: Option<EditorContext>,
    mode: vim_input::Mode,
    insert_session: bool,
    insert_session_mutated: bool,
    recording_register: Option<String>,
    last_replayed_macro: Option<String>,
    pending_command: Option<super::PendingCommandState>,
    last_character_search: Option<vim_input::Action>,
    repeat_actions: Option<Vec<vim_input::Action>>,
    recording_repeat: Option<Vec<vim_input::Action>>,
}

impl EditorState {
    pub fn new(buffers: Buffers) -> Self {
        Self {
            buffers,
            windows: Windows::default(),
            events: super::EventQueue::default(),
            current: None,
            mode: vim_input::Mode::Normal,
            insert_session: false,
            insert_session_mutated: false,
            recording_register: None,
            last_replayed_macro: None,
            pending_command: None,
            last_character_search: None,
            repeat_actions: None,
            recording_repeat: None,
        }
    }

    pub fn buffers(&self) -> &Buffers {
        &self.buffers
    }

    pub fn buffers_mut(&mut self) -> &mut Buffers {
        &mut self.buffers
    }

    pub fn windows(&self) -> &Windows {
        &self.windows
    }

    pub fn events(&self) -> &super::EventQueue {
        &self.events
    }

    pub fn events_mut(&mut self) -> &mut super::EventQueue {
        &mut self.events
    }

    pub fn register_window(&mut self, id: WindowId, buffer: BufferId) {
        self.windows.register(id, buffer);
    }

    pub fn split_window(
        &mut self,
        source: WindowId,
        new_id: WindowId,
    ) -> Result<super::WindowRecord, &'static str> {
        self.windows.split(source, new_id)
    }

    pub fn close_window(&mut self, id: WindowId) -> Option<super::WindowRecord> {
        self.windows.close(id)
    }

    pub fn unregister_window(&mut self, id: WindowId) {
        self.windows.unregister(id);
    }

    pub fn focus_window(&mut self, id: WindowId) -> Result<(), &'static str> {
        self.windows.focus(id)
    }

    pub fn set_window_buffer(
        &mut self,
        id: WindowId,
        buffer: BufferId,
    ) -> Result<(), &'static str> {
        let old_buffer = self
            .windows
            .record(id)
            .ok_or("unknown semantic window")?
            .buffer;
        if old_buffer == buffer {
            return Ok(());
        }
        self.buffers
            .get(buffer)
            .map_err(|_| "unknown semantic buffer")?;
        self.events.push(super::EditorEvent::BufLeave {
            buffer: old_buffer,
            window: id,
        });
        self.windows.set_buffer(id, buffer)?;
        self.events
            .push(super::EditorEvent::BufEnter { buffer, window: id });
        Ok(())
    }

    pub fn create_buffer(&mut self, initial_text: impl Into<String>) -> BufferId {
        let buffer = self.buffers.create(initial_text);
        self.events.push(super::EditorEvent::BufAdd { buffer });
        buffer
    }

    pub fn open_buffer(&mut self, path: impl AsRef<Path>) -> BufferId {
        let (buffer, outcome) = self
            .buffers
            .open_path_with_outcome(path)
            .expect("opening a buffer path should produce a buffer");
        match outcome {
            vim_buffer::ManagerOutcome::Added(_) => {
                self.events.push(super::EditorEvent::BufAdd { buffer });
            }
            vim_buffer::ManagerOutcome::Loaded(_) => {
                self.events.push(super::EditorEvent::BufAdd { buffer });
                self.events.push(super::EditorEvent::BufRead { buffer });
            }
            vim_buffer::ManagerOutcome::Existing(_) => {}
            _ => {}
        }
        buffer
    }

    pub fn wipe_buffer(
        &mut self,
        id: BufferId,
        force: bool,
    ) -> Result<vim_buffer::ManagerOutcome, vim_buffer::BufferError> {
        let outcome = self.buffers.wipe(id, force)?;
        self.events
            .push(super::EditorEvent::BufWipeout { buffer: id });
        Ok(outcome)
    }

    pub fn save_buffer(
        &mut self,
        id: BufferId,
        path: Option<&Path>,
        force: bool,
    ) -> Result<vim_buffer::SaveOutcome, vim_buffer::BufferError> {
        let outcome = self.buffers.save(id, path, force)?;
        self.events
            .push(super::EditorEvent::BufWrite { buffer: id });
        Ok(outcome)
    }

    /// Commits buffer options before publishing the corresponding option event.
    pub fn set_buffer_options(
        &mut self,
        id: BufferId,
        options: vim_buffer::BufferOptions,
        name: impl Into<super::OptionName>,
        value: Option<String>,
    ) -> Result<bool, vim_buffer::BufferError> {
        let changed = self.buffers.get_mut(id)?.set_options(options)?.is_some();
        if changed {
            self.events.push(super::EditorEvent::OptionSet {
                name: name.into(),
                value,
            });
        }
        Ok(changed)
    }

    /// Runs a coordinated buffer/analysis edit through the kernel-owned
    /// buffer store. Window presentation state is supplied by the caller so
    /// this boundary remains independent of the UI crate.
    pub fn edit_buffer_with_state<R>(
        &mut self,
        id: BufferId,
        edit: impl FnOnce(&mut vim_buffer::Buffer, &mut crate::model::BufferState) -> R,
    ) -> Result<R, vim_buffer::BufferError> {
        let (buffer, state) = self.buffers.get_mut_with_state(id)?;
        state.revision = state.revision.wrapping_add(1);
        Ok(edit(buffer, state))
    }

    /// Commits metadata after an asynchronous save without exposing a raw
    /// mutable buffer reference to callers.
    pub fn complete_background_save(
        &mut self,
        id: BufferId,
        path: &Path,
    ) -> Result<(), vim_buffer::BufferError> {
        let buffer = self.buffers.get_mut(id)?;
        if buffer.options().fixeol && !buffer.options().binary && !buffer.options().endofline {
            let mut options = buffer.options().clone();
            options.endofline = true;
            let _ = buffer.set_options(options);
        }
        let metadata = std::fs::metadata(path);
        buffer.set_file_metadata(vim_buffer::FileMetadata {
            path: Some(path.to_path_buf()),
            source: vim_buffer::LoadSource::File,
            modified: metadata.as_ref().ok().and_then(|m| m.modified().ok()),
            size: metadata.as_ref().ok().map(|m| m.len()),
        });
        buffer.mark_saved();
        Ok(())
    }

    pub fn mode(&self) -> vim_input::Mode {
        self.mode
    }

    pub fn insert_session_active(&self) -> bool {
        self.insert_session
    }

    /// Whether a new insert transaction should join the undo block opened by
    /// an earlier mutation in the current Insert/Replace session.
    pub fn join_insert_transaction(&self) -> bool {
        self.insert_session && self.insert_session_mutated
    }

    /// Records that the current Insert/Replace session has committed its first
    /// mutation. Entry commands such as change/open-line may establish this
    /// boundary before the first typed character.
    pub fn note_insert_mutation(&mut self) {
        if self.insert_session {
            self.insert_session_mutated = true;
        }
    }

    pub fn recording_register(&self) -> Option<&str> {
        self.recording_register.as_deref()
    }

    /// Returns the destination register when an action belongs to the active
    /// recording. Begin/end/replay control actions are handled before this
    /// method by the dispatcher and therefore never enter the recording.
    pub fn recording_target(&self) -> Option<String> {
        self.recording_register.clone()
    }

    pub fn begin_macro_recording(
        &mut self,
        register: impl Into<String>,
    ) -> Result<(), &'static str> {
        if self.recording_register.is_some() {
            return Err("macro recording is already active");
        }
        self.recording_register = Some(register.into());
        Ok(())
    }

    pub fn end_macro_recording(&mut self) -> Option<String> {
        self.recording_register.take()
    }

    pub fn repeat_actions(&self) -> Option<&[vim_input::Action]> {
        self.repeat_actions.as_deref()
    }

    pub fn set_repeat_actions(&mut self, actions: Vec<vim_input::Action>) {
        self.repeat_actions = Some(actions);
    }

    pub fn begin_repeat_recording(&mut self, action: vim_input::Action) {
        self.recording_repeat = Some(vec![action]);
    }

    pub fn append_repeat_recording(&mut self, action: vim_input::Action) {
        if let Some(recording) = &mut self.recording_repeat {
            recording.push(action);
        }
    }

    pub fn finish_repeat_recording(&mut self) {
        if let Some(recording) = self.recording_repeat.take() {
            self.repeat_actions = Some(recording);
        }
    }

    pub fn record_character_search(&mut self, action: vim_input::Action) {
        if matches!(
            action,
            vim_input::Action::MoveToNextCharacter { .. }
                | vim_input::Action::MoveToPreviousCharacter { .. }
        ) {
            self.last_character_search = Some(action);
        }
    }

    pub fn last_character_search(&self) -> Option<&vim_input::Action> {
        self.last_character_search.as_ref()
    }

    pub fn pending_command(&self) -> Option<&super::PendingCommandState> {
        self.pending_command.as_ref()
    }

    pub fn set_pending_command(&mut self, pending: super::PendingCommandState) {
        self.pending_command = Some(pending);
    }

    pub fn clear_pending_command(&mut self) {
        self.pending_command = None;
    }

    pub fn request_macro_replay(
        &mut self,
        register: &str,
        count: u32,
    ) -> Result<(String, u32), &'static str> {
        let resolved = if register == "@" || register.is_empty() {
            self.last_replayed_macro
                .clone()
                .ok_or("no previously replayed macro")?
        } else {
            register.to_string()
        };
        self.last_replayed_macro = Some(resolved.clone());
        Ok((resolved, count.max(1)))
    }

    /// Applies an authoritative semantic mode transition and emits lifecycle
    /// effects in command order. Event handlers are connected in Phase 5; the
    /// kernel owns their production now.
    pub fn transition_mode(&mut self, next: vim_input::Mode) -> super::CommandOutcome {
        let previous = self.mode;
        if previous == next {
            return super::CommandOutcome::no_redraw();
        }

        let leaving_insert = previous.is_insert() && !next.is_insert();
        let entering_insert = !previous.is_insert() && next.is_insert();
        let mut outcome = super::CommandOutcome::no_redraw();
        if leaving_insert {
            outcome.effects.push(super::CommandEffect::EventEmitted {
                name: "InsertLeavePre".to_string(),
                payload: None,
            });
            self.insert_session = false;
            self.insert_session_mutated = false;
        }
        self.mode = next;
        outcome.effects.push(super::CommandEffect::ModeChanged {
            from: previous,
            to: next,
        });
        if entering_insert {
            self.insert_session = true;
            self.insert_session_mutated = false;
            if let Some(window) = self.current.map(|context| context.window) {
                self.events.push(super::EditorEvent::InsertEnter { window });
            }
            outcome.effects.push(super::CommandEffect::EventEmitted {
                name: "InsertEnter".to_string(),
                payload: None,
            });
        } else if leaving_insert {
            if let Some(window) = self.current.map(|context| context.window) {
                self.events.push(super::EditorEvent::InsertLeave { window });
            }
            outcome.effects.push(super::CommandEffect::EventEmitted {
                name: "InsertLeave".to_string(),
                payload: None,
            });
        }
        outcome.redraw = super::RedrawRequest::View;
        outcome
    }

    pub fn current(&self) -> Option<EditorContext> {
        self.current
    }

    pub fn command_context(&self, kind: CommandKind) -> Option<CommandContext> {
        self.current.map(|current| CommandContext {
            current,
            kind,
            count: None,
            range: None,
            register: None,
            last_character_search: self.last_character_search.clone(),
        })
    }

    pub fn command_context_for(&self, command: &Command) -> Option<CommandContext> {
        self.current.map(|current| {
            let mut context = command.kernel_context(current);
            context.last_character_search = self.last_character_search.clone();
            context
        })
    }

    pub fn command_line_request(
        &self,
        text: impl Into<String>,
    ) -> Result<CommandLineRequest, String> {
        let current = self
            .current
            .ok_or_else(|| "No current editor context".to_string())?;
        CommandLineRequest::parse(current, text)
    }

    pub fn set_current(&mut self, context: EditorContext) {
        self.current = Some(context);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macro_recording_session_is_kernel_owned() {
        let mut state = EditorState::new(Buffers::new());
        assert_eq!(state.recording_register(), None);
        state.begin_macro_recording("a").unwrap();
        assert_eq!(state.recording_register(), Some("a"));
        assert_eq!(state.recording_target().as_deref(), Some("a"));
        assert!(state.begin_macro_recording("b").is_err());
        assert_eq!(state.end_macro_recording().as_deref(), Some("a"));
        assert_eq!(state.recording_register(), None);
        assert_eq!(state.end_macro_recording(), None);
    }

    #[test]
    fn macro_replay_resolves_last_register_and_normalizes_count() {
        let mut state = EditorState::new(Buffers::new());
        assert!(state.request_macro_replay("@", 1).is_err());
        assert_eq!(
            state.request_macro_replay("a", 0).unwrap(),
            ("a".to_string(), 1)
        );
        assert_eq!(
            state.request_macro_replay("@", 3).unwrap(),
            ("a".to_string(), 3)
        );
    }

    #[test]
    fn mode_transition_emits_ordered_insert_lifecycle_events() {
        let mut state = EditorState::new(Buffers::new());
        let enter = state.transition_mode(vim_input::Mode::Insert);
        let names: Vec<_> = enter
            .effects
            .iter()
            .filter_map(|effect| match effect {
                super::super::CommandEffect::ModeChanged { .. } => Some("ModeChanged"),
                super::super::CommandEffect::EventEmitted { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["ModeChanged", "InsertEnter"]);

        let leave = state.transition_mode(vim_input::Mode::Normal);
        let names: Vec<_> = leave
            .effects
            .iter()
            .filter_map(|effect| match effect {
                super::super::CommandEffect::ModeChanged { .. } => Some("ModeChanged"),
                super::super::CommandEffect::EventEmitted { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["InsertLeavePre", "ModeChanged", "InsertLeave"]);
    }

    #[test]
    fn pending_command_state_can_be_cleared_deterministically() {
        let mut state = EditorState::new(Buffers::new());
        state.set_pending_command(super::super::PendingCommandState {
            count: Some(2),
            operator: None,
            keys: Vec::new(),
            waiting_for_register: false,
            display: "2d".to_string(),
        });
        assert!(state.pending_command().is_some());
        state.clear_pending_command();
        assert!(state.pending_command().is_none());
    }

    #[test]
    fn insert_transaction_joining_is_scoped_to_one_mode_session() {
        let mut state = EditorState::new(Buffers::new());
        assert!(!state.join_insert_transaction());

        state.transition_mode(vim_input::Mode::Insert);
        assert!(!state.join_insert_transaction());
        state.note_insert_mutation();
        assert!(state.join_insert_transaction());

        state.transition_mode(vim_input::Mode::Normal);
        assert!(!state.join_insert_transaction());
        state.transition_mode(vim_input::Mode::Replace);
        assert!(!state.join_insert_transaction());
        state.transition_mode(vim_input::Mode::VirtualReplace);
        assert!(state.insert_session_active());
        assert_eq!(state.mode(), vim_input::Mode::VirtualReplace);
    }
}
