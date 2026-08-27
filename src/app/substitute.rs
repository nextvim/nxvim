use vim_script::host::RangeStateProvider;

use crate::app::App;
use crate::app::windows::WindowOps;

use super::outcome::AppCommandOutcome;
use super::prompt::{Prompt, PromptChoice, PromptHandler};

/// App orchestration for kernel-owned substitution semantics and interactive
/// confirmation prompts.
pub struct SubstituteHandler;

impl SubstituteHandler {
    pub fn start(
        app: &mut App,
        pattern: String,
        replacement: String,
        flags: String,
        range: Option<vim_script::ast::CommandRange>,
    ) -> AppCommandOutcome {
        app.model.kernel_mut().search_mut().set_substitution(
            pattern.clone(),
            range.clone(),
            replacement.clone(),
        );

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
                    return AppCommandOutcome::redraw();
                }
            }
        } else {
            let current = provider.cursor_line();
            (current, current)
        };

        app.prompt = Some(Prompt {
            handler: PromptHandler::Substitute,
            message: format!("replace with {replacement}? (y/n/a/q/l)"),
            window_id,
            substitution: Some(crate::kernel::SubstitutionSession::new(
                pattern,
                replacement,
                flags.contains('g'),
                start_line.saturating_sub(1) as u32,
                end_line.saturating_sub(1) as u32,
            )),
        });

        if flags.contains('c') {
            Self::advance(app);
            AppCommandOutcome::redraw()
        } else {
            let mutations = Self::replace_all_remaining(app);
            Self::finish(app);
            Self::mutation_outcome(mutations)
        }
    }

    pub fn respond(app: &mut App, choice: PromptChoice) -> AppCommandOutcome {
        if app.prompt.is_none() {
            return AppCommandOutcome::default();
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

    fn with_session(
        app: &mut App,
        f: impl FnOnce(
            &mut crate::kernel::SubstitutionSession,
            &mut vim_buffer::Buffer,
            &mut vim_buffer::SelectionSet,
        ),
    ) {
        let Some(mut session) = app
            .prompt
            .as_mut()
            .and_then(|prompt| prompt.substitution.take())
        else {
            return;
        };
        let window_id = app.prompt.as_ref().expect("prompt exists").window_id;
        let _ = WindowOps::edit_window(
            &mut app.ui,
            &mut app.model,
            window_id,
            |buffer, _state, window| f(&mut session, buffer, &mut window.selections),
        );
        if let Some(prompt) = app.prompt.as_mut() {
            prompt.substitution = Some(session);
        }
    }

    fn advance(app: &mut App) {
        let mut found = false;
        Self::with_session(app, |session, buffer, selections| {
            found = session.advance(buffer, selections);
        });
        if found {
            let replacement = app
                .prompt
                .as_ref()
                .and_then(|prompt| prompt.substitution.as_ref())
                .map(crate::kernel::SubstitutionSession::replacement)
                .unwrap_or_default();
            app.model.status = Some(format!("replace with {replacement} (y/n/a/q/l)?"));
        } else {
            Self::finish(app);
        }
    }

    fn replace_current(app: &mut App) -> Option<crate::kernel::MutationOutcome> {
        let mut mutation = None;
        Self::with_session(app, |session, buffer, selections| {
            mutation = session.replace_current(buffer, selections);
        });
        mutation
    }

    fn skip_current(app: &mut App) {
        Self::with_session(app, |session, buffer, _selections| {
            session.skip_current(buffer);
        });
    }

    fn replace_all_remaining(app: &mut App) -> Vec<crate::kernel::MutationOutcome> {
        let mut mutations = Vec::new();
        loop {
            Self::advance(app);
            let has_match = app
                .prompt
                .as_ref()
                .and_then(|prompt| prompt.substitution.as_ref())
                .is_some_and(crate::kernel::SubstitutionSession::has_current_match);
            if !has_match {
                break;
            }
            mutations.extend(Self::replace_current(app));
        }
        mutations
    }

    fn mutation_outcome(mutations: Vec<crate::kernel::MutationOutcome>) -> AppCommandOutcome {
        if mutations.is_empty() {
            return AppCommandOutcome::redraw();
        }
        let mut outcome = crate::kernel::CommandOutcome::no_redraw();
        for mutation in mutations {
            outcome.merge(crate::kernel::CommandOutcome::mutation_committed(mutation));
        }
        AppCommandOutcome::from_kernel(outcome)
    }

    fn finish(app: &mut App) {
        app.prompt = None;
        app.model.status = None;
        app.model.kernel_mut().search_mut().clear();
    }
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
