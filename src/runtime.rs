use crate::app::App;
use crate::controller::{Command, CommandOutcome, Dispatcher, ViewEffect};
use crate::terminal::TerminalSession;
use crate::view::{EditorViewModel, LayoutSnapshot};
use crossterm::event;
use std::io::{Write, stdout};
use text::ToPoint;
use vim_ui::BufferedRenderer;

/// Owns terminal lifecycle, source polling, command dispatch, and rendering.
pub struct Runtime {
    terminal: TerminalSession,
    app: App,
    buffered_renderer: BufferedRenderer,
}

impl Runtime {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let terminal = TerminalSession::enter()?;
        let rect = terminal.size().unwrap_or(vim_ui::Rect::new(0, 0, 80, 24));

        Ok(Self {
            terminal,
            app: App::new(rect, std::env::args_os().skip(1).map(Into::into).collect()),
            buffered_renderer: BufferedRenderer::new(rect.width, rect.height),
        })
    }

    pub fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut out = stdout();
        let rect = self.app.ui.screen_rect();
        self.redraw(rect, &mut out)?;

        let mut should_redraw = false;

        'main_loop: loop {
            let current_rect = self.app.ui.screen_rect();
            if let Ok(new_rect) = self.terminal.size() {
                if new_rect != current_rect {
                    self.resize(new_rect);
                    should_redraw = true;
                }
            }

            let mut commands = Vec::new();

            if self.app.services.poll() {
                commands.extend(
                    self.app
                        .services
                        .drain_results()
                        .into_iter()
                        .map(Command::Task),
                );
            }

            commands.extend(std::iter::from_fn(|| self.app.script.try_next_command()));

            let mut is_idle = false;
            if commands.is_empty() {
                if event::poll(std::time::Duration::from_millis(50))? {
                    let terminal_event = event::read()?;
                    if let event::Event::Resize(width, height) = terminal_event {
                        self.resize(vim_ui::Rect::new(0, 0, width, height));
                        should_redraw = true;
                    } else if let Some(command) = self.app.controller.feed_event(terminal_event) {
                        commands.push(command);
                    }
                } else {
                    is_idle = true;
                }
            }

            if is_idle {
                self.schedule_display_map_expansions();
            }

            for command in commands {
                let outcome = Dispatcher::dispatch(&mut self.app, command);
                should_redraw |= outcome.redraw;
                self.apply_outcome(&outcome);
                if outcome.quit {
                    break 'main_loop;
                }
            }

            if should_redraw {
                let active_rect = self.app.ui.screen_rect();
                self.redraw(active_rect, &mut out)?;
                should_redraw = false;
            }
        }

        self.terminal.restore()?;
        Ok(())
    }

    fn apply_outcome(&mut self, outcome: &CommandOutcome) {
        for effect in &outcome.view_effects {
            self.apply_view_effect(*effect);
        }
    }

    fn apply_view_effect(&mut self, effect: ViewEffect) {
        crate::app::ui::ViewSynchronizer::apply(
            &mut self.app.ui,
            &mut self.app.model,
            self.app.view_ids,
            effect,
        );
    }

    fn schedule_display_map_expansions(&mut self) {
        const CHUNK_ROWS: u32 = 4_096;
        let window_ids = self
            .app
            .model
            .window_buffers()
            .map(|(window_id, _)| window_id)
            .collect::<Vec<_>>();

        for window_id in window_ids {
            let Some(buffer_id) = self.app.model.window_buffer(window_id) else {
                continue;
            };
            let Some(revision) = self
                .app
                .model
                .buffer_state(buffer_id)
                .map(|state| state.revision)
            else {
                continue;
            };
            let Ok(buffer) = self.app.model.get_buffer(buffer_id) else {
                continue;
            };
            let snapshot = buffer.snapshot().as_inner().clone();
            let Some(window) = self.app.model.window_state(window_id) else {
                continue;
            };
            if window.pending_display_map.is_some() {
                continue;
            }
            let cursor_row = if window.selections.selections.is_empty() {
                0
            } else {
                window.selections.primary().head().to_point(&snapshot).row
            };
            let Some(requested_rows) = window
                .display_map
                .nearest_missing_range(cursor_row, CHUNK_ROWS)
            else {
                continue;
            };
            let Some(input) = window.display_map.expansion_input(requested_rows.clone()) else {
                continue;
            };
            let generation = input.generation.clone();
            let sequence = window.sequence.clone();
            let owner = crate::app::services::TaskOwner {
                buffer_id: Some(buffer_id),
                window_id: Some(window_id),
                revision,
            };
            let task = self.app.services.spawn_cancellable_task(
                "display_map",
                sequence,
                owner,
                crate::app::services::TaskType::DisplayMap,
                move |token| display_map::build_expansion(input, &token),
            );
            if task.is_some()
                && let Some(window) = self.app.model.window_state_mut(window_id)
            {
                window.pending_display_map = Some((generation, requested_rows));
            }
        }
    }

    fn resize(&mut self, rect: vim_ui::Rect) {
        self.apply_view_effect(ViewEffect::Resize {
            width: rect.width,
            height: rect.height,
        });
        self.buffered_renderer.resize(rect.width, rect.height);
    }

    fn redraw(
        &mut self,
        rect: vim_ui::Rect,
        out: &mut impl Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let layout = self.layout_snapshot(rect);
        crate::app::ui::ViewSynchronizer::synchronize_viewports(&mut self.app.model, &layout);
        let view_model = EditorViewModel::build(&self.app.model, &self.app.controller, &layout);
        self.app.ui.draw(&view_model, &mut self.buffered_renderer)?;
        self.buffered_renderer.flush(out)?;
        out.flush()?;
        Ok(())
    }

    fn layout_snapshot(&self, fallback: vim_ui::Rect) -> LayoutSnapshot {
        let mut snapshot = LayoutSnapshot::default();
        for (window_id, _) in self.app.model.window_buffers() {
            let rect = self
                .app
                .ui
                .computed_layout()
                .get_rect(window_id)
                .unwrap_or(fallback);
            let draws_border = self
                .app
                .ui
                .window(window_id)
                .is_some_and(|window| window.draws_border());
            snapshot.insert(window_id, rect, draws_border);
        }
        snapshot
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    Runtime::new()?.run()
}
