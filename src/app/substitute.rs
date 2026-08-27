use text::{Point, ToOffset};
use vim_buffer::TextSearch;
use vim_script::host::RangeStateProvider;
use vim_ui::WindowId;

use crate::app::App;
use crate::app::windows::WindowOps;

use super::outcome::CommandOutcome;
use super::prompt::{Prompt, PromptChoice, PromptHandler};

pub struct SubstituteHandler;

impl SubstituteHandler {
    pub fn start(
        app: &mut App,
        pattern: String,
        replacement: String,
        flags: String,
        range: Option<vim_script::ast::CommandRange>,
    ) -> CommandOutcome {
        app.model.search_pattern = Some(pattern.clone());
        app.model.search_regex =
            vim_regex::Regex::compile(&pattern, vim_regex::CompileOptions::default()).ok();
        app.model.search_range = range.clone();
        app.model.substitute_text = Some(replacement.clone());

        let window_id = app.ui.focused_window_id();
        let provider = crate::app::range_ops::EditorRangeStateProvider {
            ui: &app.ui,
            model: &app.model,
            window_id,
        };
        let (start_line, end_line) = if let Some(range) = &range {
            match vim_script::host::resolve_range(range, &provider) {
                Ok(bounds) => bounds,
                Err(err) => {
                    app.model.status = Some(err.message);
                    Self::finish(app);
                    return CommandOutcome::redraw();
                }
            }
        } else {
            let current = provider.cursor_line();
            (current, current)
        };

        let start_row = start_line.saturating_sub(1) as u32;
        let end_row = end_line.saturating_sub(1) as u32;

        app.prompt = Some(Prompt {
            handler: PromptHandler::Substitute,
            message: format!("replace with {replacement}? (y/n/a/q/l)"),
            window_id,
            pattern,
            replacement,
            global: flags.contains('g'),
            row: start_row,
            end_row,
            search_offset: 0,
            current_match: None,
        });

        if flags.contains('c') {
            Self::advance(app);
            CommandOutcome::redraw()
        } else {
            let mutations = Self::replace_all_remaining(app);
            Self::finish(app);
            Self::mutation_outcome(mutations)
        }
    }

    pub fn respond(app: &mut App, choice: PromptChoice) -> CommandOutcome {
        if app.prompt.is_none() {
            return CommandOutcome::default();
        }

        let mut mutations = Vec::new();
        match choice {
            PromptChoice::Quit => Self::finish(app),
            PromptChoice::All => {
                mutations.extend(Self::replace_current(app));
                mutations.extend(Self::replace_all_remaining(app));
                Self::finish(app);
            }
            PromptChoice::Last => {
                mutations.extend(Self::replace_current(app));
                Self::finish(app);
            }
            PromptChoice::Yes => {
                mutations.extend(Self::replace_current(app));
                Self::advance(app);
            }
            PromptChoice::No => {
                Self::skip_current(app);
                Self::advance(app);
            }
        }
        Self::mutation_outcome(mutations)
    }

    fn advance(app: &mut App) {
        loop {
            let Some(prompt) = app.prompt.as_mut() else {
                return;
            };
            if prompt.row > prompt.end_row {
                Self::finish(app);
                return;
            }

            let mut found = None;
            let _ = WindowOps::edit_window(
                &mut app.ui,
                &mut app.model,
                prompt.window_id,
                |buffer, _context, window_state| {
                    let text_buffer = buffer.as_text_buffer();
                    if prompt.row >= text_buffer.row_count() {
                        return;
                    }
                    let line_start = Point::new(prompt.row, 0).to_offset(text_buffer);
                    let line_end = Point::new(prompt.row, text_buffer.line_len(prompt.row))
                        .to_offset(text_buffer);
                    let text: String = text_buffer
                        .as_rope()
                        .chunks_in_range(line_start..line_end)
                        .collect();
                    if prompt.search_offset > text.len() {
                        return;
                    }
                    let Ok(regex) = vim_regex::Regex::compile(
                        &prompt.pattern,
                        vim_regex::CompileOptions::default(),
                    ) else {
                        return;
                    };
                    if let Some((relative_start, len, _)) =
                        text[prompt.search_offset..].find_next_pattern_match(&regex, 0)
                    {
                        let column = prompt.search_offset + relative_start;
                        found = Some((line_start + column, len));
                        window_state.selections.selections.clear();
                        window_state
                            .selections
                            .add(buffer.as_text_buffer(), line_start + column);
                    }
                },
            );

            if let Some(current_match) = found {
                prompt.current_match = Some(current_match);
                app.model.status =
                    Some(format!("replace with {} (y/n/a/q/l)?", prompt.replacement));
                return;
            }

            prompt.row += 1;
            prompt.search_offset = 0;
        }
    }

    fn replace_current(app: &mut App) -> Option<crate::kernel::MutationOutcome> {
        let row_start = app
            .prompt
            .as_ref()
            .map(|prompt| row_start_offset(app, prompt.window_id, prompt.row))
            .unwrap_or(0);
        let Some(prompt) = app.prompt.as_mut() else {
            return None;
        };
        let Some((start, len)) = prompt.current_match.take() else {
            return None;
        };
        let replacement_len = prompt.replacement.len();
        let replacement = prompt.replacement.clone();
        let mut mutation = None;
        let _ = WindowOps::edit_window(
            &mut app.ui,
            &mut app.model,
            prompt.window_id,
            |buffer, _context, window_state| {
                let range = vim_buffer::TextRange::new(
                    vim_buffer::ByteOffset(start),
                    vim_buffer::ByteOffset(start + len),
                )
                .unwrap();
                mutation = crate::kernel::transaction(
                    buffer,
                    vim_buffer::EditOrigin::VimScript,
                    None,
                    |tx| tx.replace(None, range, replacement.as_str()),
                )
                .ok();
                window_state.selections.selections.clear();
                window_state
                    .selections
                    .add(buffer.as_text_buffer(), start + replacement_len);
            },
        );
        prompt.search_offset = start
            .saturating_sub(row_start)
            .saturating_add(replacement_len);
        if !prompt.global {
            prompt.row += 1;
            prompt.search_offset = 0;
        }
        mutation
    }

    fn skip_current(app: &mut App) {
        let row_start = app
            .prompt
            .as_ref()
            .map(|prompt| row_start_offset(app, prompt.window_id, prompt.row))
            .unwrap_or(0);
        let Some(prompt) = app.prompt.as_mut() else {
            return;
        };
        let Some((start, len)) = prompt.current_match.take() else {
            return;
        };
        prompt.search_offset = start.saturating_sub(row_start).saturating_add(len.max(1));
        if !prompt.global {
            prompt.row += 1;
            prompt.search_offset = 0;
        }
    }

    fn replace_all_remaining(app: &mut App) -> Vec<crate::kernel::MutationOutcome> {
        let mut mutations = Vec::new();
        loop {
            Self::advance(app);
            if app
                .prompt
                .as_ref()
                .and_then(|prompt| prompt.current_match)
                .is_none()
            {
                break;
            }
            mutations.extend(Self::replace_current(app));
        }
        mutations
    }

    fn mutation_outcome(mutations: Vec<crate::kernel::MutationOutcome>) -> CommandOutcome {
        if mutations.is_empty() {
            return CommandOutcome::redraw();
        }
        let mut outcome = crate::kernel::CommandOutcome::no_redraw();
        for mutation in mutations {
            outcome.merge(crate::kernel::CommandOutcome::mutation_committed(mutation));
        }
        CommandOutcome::from_kernel(outcome)
    }

    fn finish(app: &mut App) {
        app.prompt = None;
        app.model.status = None;
        app.model.search_pattern = None;
        app.model.search_regex = None;
        app.model.search_range = None;
        app.model.substitute_text = None;
    }
}

fn row_start_offset(app: &App, window_id: WindowId, row: u32) -> usize {
    WindowOps::window_buffer(&app.ui, window_id)
        .and_then(|buffer_id| app.model.get_buffer(buffer_id).ok())
        .map(|buffer| Point::new(row, 0).to_offset(buffer.as_text_buffer()))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vim_ui::Rect;

    fn app_with_text(text: &str) -> App {
        let mut app = App::new(Rect::new(0, 0, 80, 24), crate::app::args::Args::default());
        let buffer_id = WindowOps::window_buffer(&app.ui, app.view_ids.main).unwrap();
        let buffer = app.model.get_buffer_mut(buffer_id).unwrap();
        let mut transaction = buffer.transaction(vim_buffer::EditOrigin::VimScript);
        transaction.replace(
            None,
            vim_buffer::TextRange::new(vim_buffer::ByteOffset(0), vim_buffer::ByteOffset(0))
                .unwrap(),
            text,
        );
        transaction.commit(None).unwrap();
        app
    }

    fn text(app: &App) -> String {
        let buffer_id = WindowOps::window_buffer(&app.ui, app.view_ids.main).unwrap();
        let buffer = app.model.get_buffer(buffer_id).unwrap();
        buffer
            .as_text_buffer()
            .as_rope()
            .chunks_in_range(0..buffer.as_text_buffer().len())
            .collect()
    }

    #[test]
    fn confirm_substitute_accepts_yes_no_and_all() {
        let mut app = app_with_text("foo foo foo");
        SubstituteHandler::start(&mut app, "foo".into(), "bar".into(), "gc".into(), None);
        assert!(app.prompt.is_some());
        assert!(app.model.status.as_deref().unwrap().contains("y/n/a/q/l"));

        let yes = SubstituteHandler::respond(&mut app, PromptChoice::Yes);
        assert!(matches!(
            yes.kernel_effects.as_slice(),
            [crate::kernel::CommandEffect::MutationCommitted(_)]
        ));
        let no = SubstituteHandler::respond(&mut app, PromptChoice::No);
        assert!(no.kernel_effects.is_empty());
        let all = SubstituteHandler::respond(&mut app, PromptChoice::All);
        assert!(matches!(
            all.kernel_effects.as_slice(),
            [crate::kernel::CommandEffect::MutationCommitted(_)]
        ));

        assert_eq!(text(&app), "bar foo bar");
        assert!(app.prompt.is_none());
    }

    #[test]
    fn confirm_substitute_last_replaces_once_then_stops() {
        let mut app = app_with_text("foo foo");
        SubstituteHandler::start(&mut app, "foo".into(), "bar".into(), "gc".into(), None);
        let outcome = SubstituteHandler::respond(&mut app, PromptChoice::Last);
        assert!(matches!(
            outcome.kernel_effects.as_slice(),
            [crate::kernel::CommandEffect::MutationCommitted(_)]
        ));

        assert_eq!(text(&app), "bar foo");
        assert!(app.prompt.is_none());
    }
}
