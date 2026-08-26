use crate::app::App;

use crate::app::windows::WindowOps;
use crate::controller::{Command, CommandOutcome, Dispatcher, ViewEffect};
use crate::terminal::TerminalSession;
use crate::view::LayoutSnapshot;
use crossterm::event;
use std::io::{Write, stdout};
use vim_ui::BufferedRenderer;

// Owns terminal lifecycle, source polling, command dispatch, and rendering.
pub struct Runtime {
    terminal: TerminalSession,
    app: App,
    buffered_renderer: BufferedRenderer,
    script: crate::script::ScriptRuntime,
}

enum RuntimeCommand {
    Controller(Command),
    ScriptHost(crate::script::EmittedCommand),
}

impl Runtime {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let terminal = TerminalSession::enter()?;
        let rect = terminal.size().unwrap_or(vim_ui::Rect::new(0, 0, 80, 24));

        let args = crate::app::args::Args::parse();
        let pre_config_cmds = args.pre_config_cmds.clone();
        let post_config_cmds = args.post_config_cmds.clone();
        let scripts = args.scripts.clone();

        let mut app = App::new(rect, args);
        let mut script = crate::script::ScriptRuntime::new();
        app.init(&mut script, pre_config_cmds, post_config_cmds, scripts);
        app.model
            .kernel_mut()
            .events_mut()
            .push(crate::kernel::EditorEvent::VimEnter);

        Ok(Self {
            terminal,
            app,
            buffered_renderer: BufferedRenderer::new(rect.width, rect.height),
            script,
        })
    }

    pub fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut out = stdout();
        crate::app::services::schedule_state_updates(&mut self.app, None);

        let mut should_redraw = crate::kernel::RedrawRequest::Full;
        let mut last_command_time = std::time::Instant::now();
        let mut is_idle = false;
        let mut idle_since: Option<std::time::Instant> = None;

        'main_loop: loop {
            let current_rect = self.app.ui.screen_rect();
            if let Ok(new_rect) = self.terminal.size() {
                if new_rect != current_rect {
                    self.resize(new_rect);
                    should_redraw = crate::kernel::RedrawRequest::View;
                }
            }

            let mut commands = Vec::new();

            while let Some(cmd) = self.app.command_queue.pop_front() {
                commands.push(RuntimeCommand::Controller(cmd));
            }

            if commands.is_empty() && self.app.services.poll() {
                commands.extend(
                    self.app
                        .services
                        .drain_results()
                        .into_iter()
                        .map(Command::Task)
                        .map(RuntimeCommand::Controller),
                );
            }

            while let Some(emitted) = self.script.try_next_emitted_command() {
                commands.push(RuntimeCommand::ScriptHost(emitted));
            }

            if commands.is_empty() {
                if event::poll(std::time::Duration::from_millis(50))? {
                    let terminal_event = event::read()?;
                    if let event::Event::Resize(width, height) = terminal_event {
                        self.resize(vim_ui::Rect::new(0, 0, width, height));
                        should_redraw = crate::kernel::RedrawRequest::View;
                    } else if self.app.prompt.is_some() {
                        if let Some(choice) = crate::controller::prompt_choice(&terminal_event) {
                            if let Some(handler) =
                                self.app.prompt.as_ref().map(|prompt| prompt.handler)
                            {
                                commands.push(RuntimeCommand::Controller(Command::PromptChoice {
                                    handler,
                                    choice,
                                }));
                            }
                        }
                    } else if let Some(command) = self.app.controller.feed_event(terminal_event) {
                        commands.push(RuntimeCommand::Controller(command));
                    }
                }
            }

            if commands.is_empty() {
                crate::app::services::schedule_state_updates(
                    &mut self.app,
                    idle_since.map(|since: std::time::Instant| since.elapsed()),
                );
            }

            if !commands.is_empty() {
                last_command_time = std::time::Instant::now();
                if is_idle {
                    is_idle = false;
                    idle_since = None;
                    if self.app.model.status.as_deref() == Some("idle") {
                        self.app.model.status = None;
                        should_redraw = crate::kernel::RedrawRequest::View;
                    }
                }
            } else if is_idle == false
                && last_command_time.elapsed() >= std::time::Duration::from_secs(2)
            {
                is_idle = true;
                idle_since = Some(std::time::Instant::now());
                self.app.model.status = Some("idle".to_string());
                should_redraw = crate::kernel::RedrawRequest::View;
            }

            let processed_any = !commands.is_empty();
            for command in commands {
                let RuntimeCommand::Controller(command) = command else {
                    let RuntimeCommand::ScriptHost(emitted) = command else {
                        unreachable!();
                    };
                    let origin = emitted.editor_context();
                    let current = self.app.current_context();
                    match crate::kernel::ExDispatcher::execute_host_command(
                        &mut self.app,
                        current,
                        origin,
                        emitted.command,
                    ) {
                        Ok(outcome) => {
                            should_redraw = should_redraw.max(outcome.redraw);
                            self.apply_outcome(&outcome);
                            self.app.sync_kernel_layout();
                            if outcome.quit {
                                break 'main_loop;
                            }
                        }
                        Err(error) => {
                            self.app.model.status = Some(error);
                            should_redraw = crate::kernel::RedrawRequest::View;
                        }
                    }
                    continue;
                };
                if let Command::CommandLine(ref request) = command {
                    log::trace!(
                        "dispatching command-line {:?} in tab {} window {} buffer {}: range={:?} count={:?} register={:?} modifiers={:?} bang={}",
                        request.kind,
                        request.current.tab.get(),
                        request.current.window.get(),
                        request.current.buffer.get(),
                        request.range,
                        request.count,
                        request.register,
                        request.modifiers,
                        request.bang,
                    );
                    let current_buffer = crate::app::ui::current_buffer(&self.app);
                    let _ = self.script.update_state(&self.app.model, current_buffer);
                    let current = self.app.current_context();
                    if let Err(err) =
                        crate::kernel::ExDispatcher::dispatch(current, request, |accepted| {
                            self.script
                                .execute_with_context(&accepted.text, Some(accepted.current))
                                .map(|_| ())
                        })
                    {
                        self.app.model.status = Some(err);
                    }
                    self.app.sync_kernel_context();
                    should_redraw = crate::kernel::RedrawRequest::View;
                    continue;
                }
                if let Command::ExecuteScript(ref script_str) = command {
                    let current_buffer = crate::app::ui::current_buffer(&self.app);
                    let _ = self.script.update_state(&self.app.model, current_buffer);
                    if let Err(err) = self
                        .script
                        .execute_with_context(script_str, self.app.current_context())
                    {
                        self.app.model.status = Some(err);
                    }
                    self.app.sync_kernel_context();
                    if let Err(err) = self.app.validate_kernel_context() {
                        log::warn!("invalid kernel context after script: {err}");
                    }
                    should_redraw = crate::kernel::RedrawRequest::View;
                    continue;
                }
                if let Err(err) = self.app.validate_kernel_context() {
                    log::warn!("invalid kernel context before command: {err}");
                    self.app.sync_kernel_context();
                }
                let outcome = Dispatcher::dispatch(&mut self.app, command);
                should_redraw = should_redraw.max(outcome.redraw);
                self.apply_outcome(&outcome);
                self.app.sync_kernel_layout();
                if let Err(err) = self.app.validate_kernel_context() {
                    log::warn!("invalid kernel context after command: {err}");
                }
                if outcome.quit {
                    break 'main_loop;
                }
            }
            if processed_any {
                self.app.sync_kernel_context();
                let current_buffer = crate::app::ui::current_buffer(&self.app);
                let _ = self.script.update_state(&self.app.model, current_buffer);
            }

            // Deferred events are delivered only after the command batch has
            // committed, so callbacks cannot observe an in-flight mutation.
            self.deliver_deferred_events();

            let (request, invalidations, mut view_invalidations) = self.app.take_redraw();
            crate::app::services::schedule_redraw_invalidations(&mut self.app, &invalidations);
            view_invalidations.extend(self.app.take_view_invalidations());
            should_redraw = should_redraw.max(request);

            if should_redraw != crate::kernel::RedrawRequest::None {
                let active_rect = self.app.ui.screen_rect();
                self.redraw(active_rect, &view_invalidations, &mut out)?;
                should_redraw = crate::kernel::RedrawRequest::None;
            }
        }

        while self.app.services.has_pending_saves() {
            if self.app.services.poll() {
                for task_res in self.app.services.drain_results() {
                    let _ = Dispatcher::dispatch(&mut self.app, Command::Task(task_res));
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        self.app
            .model
            .kernel_mut()
            .events_mut()
            .push(crate::kernel::EditorEvent::VimLeave);
        self.terminal.restore()?;

        Ok(())
    }

    fn apply_outcome(&mut self, outcome: &CommandOutcome) {
        self.app
            .queue_redraw(outcome.redraw, &outcome.invalidations);
        if outcome.invalidations.is_empty()
            && matches!(
                outcome.redraw,
                crate::kernel::RedrawRequest::Layout | crate::kernel::RedrawRequest::Full
            )
        {
            self.app.queue_redraw(
                outcome.redraw,
                &[crate::kernel::RedrawInvalidation::global(
                    crate::kernel::RedrawInvalidationKind::CompleteLayout,
                )],
            );
        }
        for effect in &outcome.view_effects {
            self.apply_view_effect(*effect);
        }
        for effect in &outcome.kernel_effects {
            match effect {
                crate::kernel::CommandEffect::Message(message) => {
                    self.app.model.status = Some(message.clone());
                    self.app.message = message.clone();
                    self.app.messages.push(message.clone());
                    self.app.queue_redraw(
                        crate::kernel::RedrawRequest::View,
                        &[crate::kernel::RedrawInvalidation::global(
                            crate::kernel::RedrawInvalidationKind::Statusline,
                        )],
                    );
                }
                crate::kernel::CommandEffect::MutationCommitted(mutation) => {
                    self.app.model.kernel_mut().events_mut().push(
                        crate::kernel::EditorEvent::TextChanged {
                            buffer: mutation.buffer,
                            tick: mutation.changed_tick,
                        },
                    );
                    // Older producers may not carry typed invalidations yet.
                    if outcome.invalidations.is_empty() {
                        let invalidations = mutation.invalidations();
                        self.app
                            .queue_redraw(crate::kernel::RedrawRequest::View, &invalidations);
                    }
                }
                crate::kernel::CommandEffect::EventEmitted { name, payload } => {
                    log::trace!("kernel event {name}: {payload:?}");
                }
                crate::kernel::CommandEffect::BackgroundWorkRequested { kind } => {
                    log::trace!("kernel background work requested: {kind}");
                }
                crate::kernel::CommandEffect::CursorMoved { window } => {
                    self.app
                        .model
                        .kernel_mut()
                        .events_mut()
                        .push(crate::kernel::EditorEvent::CursorMoved { window: *window });
                    self.app.queue_redraw(
                        crate::kernel::RedrawRequest::View,
                        &[crate::kernel::RedrawInvalidation::window(
                            crate::kernel::RedrawInvalidationKind::Cursor,
                            *window,
                        )],
                    );
                }
                crate::kernel::CommandEffect::TabChanged { .. } => {
                    self.app.queue_redraw(
                        crate::kernel::RedrawRequest::View,
                        &[crate::kernel::RedrawInvalidation::global(
                            crate::kernel::RedrawInvalidationKind::Tabline,
                        )],
                    );
                }
                crate::kernel::CommandEffect::ModeChanged { .. } => {
                    self.app.queue_redraw(
                        crate::kernel::RedrawRequest::View,
                        &[crate::kernel::RedrawInvalidation::global(
                            crate::kernel::RedrawInvalidationKind::Statusline,
                        )],
                    );
                }
                crate::kernel::CommandEffect::BufferMutated { .. }
                | crate::kernel::CommandEffect::WindowChanged { .. }
                | crate::kernel::CommandEffect::OptionChanged { .. }
                | crate::kernel::CommandEffect::QuitRequested => {}
            }
        }
    }

    fn deliver_deferred_events(&mut self) {
        for event in self.app.model.kernel_mut().events_mut().drain_deferred() {
            log::trace!("deferred editor event: {event:?}");
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
        view_invalidations: &[crate::app::ViewInvalidation],
        out: &mut impl Write,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let status_height = if self.app.inspect { 2 } else { 1 };
        self.app.ui.set_window_constraint(
            self.app.view_ids.statusline,
            vim_ui::SizeConstraint::Fixed(status_height),
        );

        let layout = self.layout_snapshot(rect);
        crate::app::ui::ViewSynchronizer::synchronize_viewports(
            &mut self.app.ui,
            &self.app.model,
            &layout,
        );

        let targeted_windows: std::collections::HashSet<_> = view_invalidations
            .iter()
            .filter_map(|invalidation| match invalidation.target {
                crate::app::ViewInvalidationTarget::Window(window_id) => Some(window_id),
                _ => None,
            })
            .collect();
        let window_ids: Vec<_> = WindowOps::window_buffers(&self.app.ui)
            .into_iter()
            .map(|(window_id, _)| window_id)
            .filter(|window_id| targeted_windows.is_empty() || targeted_windows.contains(window_id))
            .collect();
        for window_id in window_ids {
            crate::app::services::schedule_window_highlight(&mut self.app, window_id, 0, 0);
        }

        crate::app::ui::refresh_views(&mut self.app, &layout);
        self.app.ui.draw(&mut self.buffered_renderer)?;
        self.buffered_renderer.flush(out)?;
        out.flush()?;
        Ok(())
    }

    fn layout_snapshot(&self, fallback: vim_ui::Rect) -> LayoutSnapshot {
        let mut snapshot = LayoutSnapshot::default();
        for (window_id, _) in WindowOps::window_buffers(&self.app.ui) {
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
