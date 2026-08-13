use crate::app::App;
use crate::controller::{Command, CommandOutcome, Dispatcher, ViewEffect};
use crate::terminal::TerminalSession;
use crate::view::{EditorViewModel, LayoutSnapshot};
use crossterm::event;
use std::io::{Write, stdout};
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

            if commands.is_empty() && event::poll(std::time::Duration::from_millis(50))? {
                let terminal_event = event::read()?;
                if let event::Event::Resize(width, height) = terminal_event {
                    self.resize(vim_ui::Rect::new(0, 0, width, height));
                    should_redraw = true;
                } else if let Some(command) = self.app.controller.feed_event(terminal_event) {
                    commands.push(command);
                }
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
