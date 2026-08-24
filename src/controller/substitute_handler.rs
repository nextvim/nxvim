use text::{Point, ToOffset};
use vim_buffer::TextSearch;
use vim_script::host::RangeStateProvider;
use vim_ui::WindowId;

use crate::app::App;
use crate::app::windows::WindowOps;

use super::command::CommandOutcome;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptChoice {
    Yes,
    No,
    All,
    Quit,
    Last,
}

pub struct Prompt {
    pub handler: PromptHandler,
    window_id: WindowId,
    pattern: String,
    replacement: String,
    global: bool,
    row: u32,
    end_row: u32,
    search_offset: usize,
    current_match: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptHandler {
    Substitute,
}

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
        let provider = crate::controller::range::EditorRangeStateProvider {
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
        } else {
            Self::replace_all_remaining(app);
            Self::finish(app);
        }
        CommandOutcome::redraw()
    }

    pub fn respond(app: &mut App, choice: PromptChoice) -> CommandOutcome {
        if app.prompt.is_none() {
            return CommandOutcome::default();
        }

        match choice {
            PromptChoice::Quit => Self::finish(app),
            PromptChoice::All => {
                Self::replace_current(app);
                Self::replace_all_remaining(app);
                Self::finish(app);
            }
            PromptChoice::Last => {
                Self::replace_current(app);
                Self::finish(app);
            }
            PromptChoice::Yes => {
                Self::replace_current(app);
                Self::advance(app);
            }
            PromptChoice::No => {
                Self::skip_current(app);
                Self::advance(app);
            }
        }
        CommandOutcome::redraw()
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

    fn replace_current(app: &mut App) {
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
        let replacement_len = prompt.replacement.len();
        let replacement = prompt.replacement.clone();
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
                let mut transaction = buffer.transaction(vim_buffer::EditOrigin::VimScript);
                transaction.replace(None, range, replacement.as_str());
                let _ = transaction.commit(None);
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

    fn replace_all_remaining(app: &mut App) {
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
            Self::replace_current(app);
        }
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

        SubstituteHandler::respond(&mut app, PromptChoice::Yes);
        SubstituteHandler::respond(&mut app, PromptChoice::No);
        SubstituteHandler::respond(&mut app, PromptChoice::All);

        assert_eq!(text(&app), "bar foo bar");
        assert!(app.prompt.is_none());
    }

    #[test]
    fn confirm_substitute_last_replaces_once_then_stops() {
        let mut app = app_with_text("foo foo");
        SubstituteHandler::start(&mut app, "foo".into(), "bar".into(), "gc".into(), None);
        SubstituteHandler::respond(&mut app, PromptChoice::Last);

        assert_eq!(text(&app), "bar foo");
        assert!(app.prompt.is_none());
    }
}
