//! Application composition and infrastructure adapters.
//!
//! This module wires the kernel-owned semantic model, app-owned command
//! routing, services, UI synchronization, and scripting. Legacy semantic
//! implementations are retained only as explicitly named compatibility code.
pub mod application;
pub mod args;
pub mod buffer_handler;
pub mod command;
pub mod commandline_handler;
pub mod config;
pub mod editor;
pub mod editor_handler;
pub mod input;
pub mod legacy_command;
pub mod legacy_editor;
pub mod lifecycle;
pub mod lifecycle_ops;
pub mod navigation;
pub mod operations;
pub mod outcome;
pub mod prompt;
pub mod range_ops;
pub mod search;
pub mod services;
pub mod substitute;
pub mod task_dispatcher;
pub mod ui;
pub mod window_handler;
pub mod windows;

use windows::WindowOps;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ViewInvalidationTarget {
    Window(vim_ui::WindowId),
    Statusline,
    Tabline,
    Overlay,
    Layout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ViewInvalidation {
    pub target: ViewInvalidationTarget,
    pub kind: crate::kernel::RedrawInvalidationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectKind {
    None,
    TreeSitter,
    Textmate,
    Indexer,
}

pub struct App {
    pub model: crate::model::EditorModel,
    /// Semantic tab pages. The initial migration contains one page and
    /// projects structural changes from `vim-ui` into it.
    pub tabs: crate::kernel::TabPages,
    pub input: input::InputAdapter,
    pub services: services::Services,
    pub ui: ui::Ui,
    pub view_ids: ui::ViewIds,
    pub command_queue: std::collections::VecDeque<command::AppCommand>,
    /// Typed invalidations accumulated until the next render boundary.
    pub pending_invalidations: Vec<crate::kernel::RedrawInvalidation>,
    pub pending_redraw: crate::kernel::RedrawRequest,
    pub pending_view_invalidations: Vec<ViewInvalidation>,
    pub prompt: Option<prompt::Prompt>,
    pub colorscheme: Option<vim_colorscheme::ColorScheme>,
    pub highlighter: Option<textmate::Highlighter<'static>>,

    // App State
    pub config: std::sync::Arc<std::sync::RwLock<config::ConfigStore>>,
    pub syntax_highlight: bool,
    pub treesitter_enabled: bool,
    pub indexer_enabled: bool,
    pub message: String,
    pub messages: Vec<String>,
    pub inspect: bool,
    pub inspect_what: InspectKind,
}

impl App {
    pub fn new(screen_rect: ui::Rect, args: args::Args) -> Self {
        let mut ui = ui::Ui::new(screen_rect);
        let view_ids = ui::setup_initial_layout(&mut ui).unwrap();
        let model = crate::model::EditorModel::new(args.paths);
        let tabs = crate::kernel::TabPages::single(ui.layout().clone(), view_ids.main);

        let initial_buffer = model
            .get_buffer(model.initial_buffer())
            .expect("initial editor buffer must exist");
        WindowOps::register_placeholder(&mut ui, view_ids.main, initial_buffer);
        let commandline_buffer = model
            .get_buffer(model.commandline_buffer())
            .expect("command-line buffer must exist");
        WindowOps::register_placeholder(&mut ui, view_ids.commandline, commandline_buffer);
        if let Some(window_state) = ui
            .window_mut(view_ids.commandline)
            .and_then(vim_ui::Window::window_state_mut)
        {
            window_state.set_show_gutter(false);
            window_state.set_show_matches(false);
        }

        let colorscheme = vim_colorscheme::ColorScheme::load_default();
        let highlighter = textmate::load_colorscheme(&colorscheme);

        let mut app = Self {
            model,
            tabs,
            input: input::InputAdapter::new(vim_input::Mode::Normal),
            services: services::Services::new(),
            ui,
            view_ids,
            command_queue: std::collections::VecDeque::new(),
            pending_invalidations: Vec::new(),
            pending_redraw: crate::kernel::RedrawRequest::None,
            pending_view_invalidations: Vec::new(),
            prompt: None,
            colorscheme: Some(colorscheme),
            highlighter: Some(highlighter),
            config: std::sync::Arc::new(std::sync::RwLock::new(config::ConfigStore::new())),
            syntax_highlight: true,
            treesitter_enabled: false,
            indexer_enabled: false,
            message: "".to_string(),
            messages: Vec::new(),
            inspect: false,
            inspect_what: InspectKind::None,
        };

        app.ui.set_colorscheme(app.colorscheme.clone());
        app.sync_kernel_windows();
        app.sync_kernel_context();
        app
    }

    /// Reconciles concrete UI windows with kernel-owned semantic identities.
    /// This is projection bookkeeping; tab membership and activation remain
    /// authoritative in `TabPages`.
    pub fn sync_kernel_windows(&mut self) {
        let window_buffers = WindowOps::window_buffers(&self.ui);
        let active_members: Vec<_> = self
            .ui
            .layout()
            .window_ids()
            .into_iter()
            .filter(|window| {
                *window != self.view_ids.commandline
                    && WindowOps::window_buffer(&self.ui, *window).is_some()
            })
            .collect();
        let focused = self.ui.focused_window_id();
        let active_window = if self
            .ui
            .window(focused)
            .is_some_and(vim_ui::Window::has_content)
        {
            focused
        } else {
            self.view_ids.main
        };

        self.tabs.set_active_windows(active_members);

        let kernel = self.model.kernel_mut();
        for (window_id, buffer_id) in window_buffers {
            kernel.register_window(window_id, buffer_id);
        }
        let _ = kernel.focus_window(active_window);
    }

    /// Synchronizes the kernel's stable current context from the active UI
    /// window and the active semantic tab page.
    pub fn sync_kernel_context(&mut self) {
        let focused = self.ui.focused_window_id();
        let window = if self
            .ui
            .window(focused)
            .is_some_and(vim_ui::Window::has_content)
        {
            focused
        } else {
            self.view_ids.main
        };
        // The current context must describe the buffer actually hosted by the
        // focused window. Command-line editing is buffer-backed too; replacing
        // its buffer ID with the main buffer makes every InsertText fail the
        // kernel identity check before it can mutate or render.
        let buffer = WindowOps::window_buffer(&self.ui, window)
            .unwrap_or_else(|| self.model.buffers().current());
        self.model
            .kernel_mut()
            .set_current(crate::kernel::EditorContext {
                tab: self.tabs.active_id(),
                window,
                buffer,
            });
    }

    /// Accumulates typed presentation work until the runtime reaches a
    /// render boundary. Exact duplicate invalidations are intentionally
    /// coalesced; range union remains the responsibility of the owning map.
    pub fn queue_redraw(
        &mut self,
        request: crate::kernel::RedrawRequest,
        invalidations: &[crate::kernel::RedrawInvalidation],
    ) {
        self.pending_redraw = self.pending_redraw.max(request);
        for invalidation in invalidations {
            if !self.pending_invalidations.contains(invalidation) {
                self.pending_invalidations.push(invalidation.clone());
            }
        }
    }

    pub fn queue_view_invalidation(&mut self, invalidation: ViewInvalidation) {
        if !self.pending_view_invalidations.contains(&invalidation) {
            self.pending_view_invalidations.push(invalidation);
        }
    }

    /// Takes all pending presentation work at one render boundary.
    pub fn take_redraw(
        &mut self,
    ) -> (
        crate::kernel::RedrawRequest,
        Vec<crate::kernel::RedrawInvalidation>,
        Vec<ViewInvalidation>,
    ) {
        (
            std::mem::take(&mut self.pending_redraw),
            std::mem::take(&mut self.pending_invalidations),
            std::mem::take(&mut self.pending_view_invalidations),
        )
    }

    pub fn take_view_invalidations(&mut self) -> Vec<ViewInvalidation> {
        std::mem::take(&mut self.pending_view_invalidations)
    }

    pub fn current_context(&self) -> Option<crate::kernel::EditorContext> {
        self.model.kernel().current()
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.count()
    }

    pub fn active_tab(&self) -> crate::kernel::TabPageId {
        self.tabs.active_id()
    }

    pub fn create_tab(
        &mut self,
        layout: vim_ui::LayoutNode,
        active_window: vim_ui::WindowId,
    ) -> crate::kernel::TabPageId {
        self.tabs.create(layout, active_window)
    }

    pub fn new_tab(
        &mut self,
        buffer: vim_buffer::BufferId,
    ) -> Result<crate::kernel::TabPageId, String> {
        let source = self.tabs.active().active_window;
        let mut layout = self.tabs.active().layout.clone();
        let window = self.ui.create_window("MAIN WINDOW".to_string());
        {
            let buffer_ref = self
                .model
                .get_buffer(buffer)
                .map_err(|error| error.to_string())?;
            WindowOps::register_placeholder(&mut self.ui, window, buffer_ref);
        }
        if let Some(created) = self.ui.window_mut(window) {
            created.set_draw_border(false);
            created.set_view(Box::new(crate::view::TextView::new()));
        }
        if !layout.replace_leaf(source, window) {
            self.ui.window_store_mut().remove(window);
            return Err("active tab window is missing from its layout".to_string());
        }
        self.model.kernel_mut().register_window(window, buffer);
        let id = self.tabs.create(layout.clone(), window);
        if let Err(error) = self.ui.activate_layout(layout, window) {
            let _ = self.tabs.close(id);
            self.model.kernel_mut().close_window(window);
            self.ui.window_store_mut().remove(window);
            return Err(error.to_string());
        }
        self.model
            .kernel_mut()
            .focus_window(window)
            .map_err(str::to_string)?;
        self.model
            .kernel_mut()
            .set_current(crate::kernel::EditorContext {
                tab: id,
                window,
                buffer,
            });
        Ok(id)
    }

    pub fn switch_tab(&mut self, id: crate::kernel::TabPageId) -> Result<(), String> {
        let page = self
            .tabs
            .page(id)
            .cloned()
            .ok_or_else(|| "unknown tab page".to_string())?;
        self.ui
            .activate_layout(page.layout, page.active_window)
            .map_err(|error| error.to_string())?;
        self.tabs.switch_to(id).map_err(str::to_string)?;
        self.model
            .kernel_mut()
            .focus_window(page.active_window)
            .map_err(str::to_string)?;
        let buffer = WindowOps::window_buffer(&self.ui, page.active_window)
            .ok_or_else(|| "active tab window has no buffer".to_string())?;
        self.model
            .kernel_mut()
            .set_current(crate::kernel::EditorContext {
                tab: id,
                window: page.active_window,
                buffer,
            });
        Ok(())
    }

    pub fn next_tab(&mut self, count: usize) -> Result<crate::kernel::TabPageId, String> {
        let id = self.tabs.next_id(count);
        self.switch_tab(id)?;
        Ok(id)
    }

    pub fn previous_tab(&mut self, count: usize) -> Result<crate::kernel::TabPageId, String> {
        let id = self.tabs.previous_id(count);
        self.switch_tab(id)?;
        Ok(id)
    }

    pub fn close_tab(
        &mut self,
        id: crate::kernel::TabPageId,
    ) -> Result<crate::kernel::TabPageId, &'static str> {
        self.tabs.close(id)
    }

    pub fn close_active_tab(&mut self) -> Result<crate::kernel::TabPageId, String> {
        let closing = self.tabs.active().clone();
        let survivor = self.tabs.close(closing.id).map_err(str::to_string)?;
        self.switch_tab(survivor)?;
        for window in closing.windows {
            if !self.tabs.iter().any(|page| page.windows.contains(&window)) {
                self.model.kernel_mut().close_window(window);
                self.ui.window_store_mut().remove(window);
            }
        }
        Ok(survivor)
    }

    /// Commits the concrete result of a structural UI operation to the active
    /// semantic tab. Tab activation projects this stored layout back to `vim-ui`.
    pub fn sync_kernel_layout(&mut self) {
        self.sync_kernel_windows();
        let layout = self.ui.layout().clone();
        let focused = self.ui.focused_window_id();
        let active_window = if focused == self.view_ids.commandline {
            self.ui
                .focus_manager()
                .previous_id()
                .filter(|window| *window != self.view_ids.commandline)
                .unwrap_or(self.tabs.active().active_window)
        } else {
            focused
        };
        self.tabs.update_active_layout(layout, active_window);
        self.sync_kernel_context();
    }

    /// Validates that the ID-based kernel context still names the focused
    /// semantic window and an existing buffer. This is intentionally checked
    /// at command boundaries while window and tab stores are still migrating.
    pub fn validate_kernel_context(&self) -> Result<(), String> {
        let context = self
            .current_context()
            .ok_or_else(|| "kernel has no current editor context".to_string())?;
        let focused = self.ui.focused_window_id();
        if context.window != focused
            && self
                .ui
                .window(focused)
                .is_some_and(vim_ui::Window::has_content)
        {
            return Err(format!(
                "kernel window {} is not focused window {}",
                context.window.get(),
                focused.get()
            ));
        }
        let window_buffer = WindowOps::window_buffer(&self.ui, context.window)
            .ok_or_else(|| format!("kernel window {} has no buffer", context.window.get()))?;
        if window_buffer != context.buffer {
            return Err(format!(
                "kernel buffer {} disagrees with window {} buffer {}",
                context.buffer.get(),
                context.window.get(),
                window_buffer.get()
            ));
        }
        self.model
            .get_buffer(context.buffer)
            .map(|_| ())
            .map_err(|err| format!("kernel context names invalid buffer: {err}"))
    }

    pub fn init(
        &mut self,
        script: &mut crate::script::ScriptRuntime,
        pre_config_cmds: Vec<String>,
        post_config_cmds: Vec<String>,
        scripts: Vec<std::path::PathBuf>,
    ) {
        if cfg!(test) {
            return;
        }

        for cmd in pre_config_cmds {
            if let Err(err) = script.execute(&cmd) {
                log::error!("Error executing pre-config command {}: {}", cmd, err);
            }
        }

        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        if let Some(home) = home {
            let paths = [
                home.join(".config/nxvim/init.vim"),
                home.join(".nxvimrc"),
                home.join(".nxvim/nxvimrc"),
                home.join(".config/nxvim/nxvimrc"),
            ];

            for path in &paths {
                if path.exists() {
                    if let Ok(content) = std::fs::read_to_string(path) {
                        if let Err(err) = script.execute(&content) {
                            log::error!("Error executing init file {:?}: {}", path, err);
                        }
                    }
                    break;
                }
            }
        }

        for cmd in post_config_cmds {
            if let Err(err) = script.execute(&cmd) {
                log::error!("Error executing post-config command {}: {}", cmd, err);
            }
        }

        for path in scripts {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    if let Err(err) = script.execute(&content) {
                        log::error!("Error executing script file {:?}: {}", path, err);
                    }
                }
                Err(err) => {
                    log::error!("Error reading script file {:?}: {}", path, err);
                }
            }
        }
    }
}
