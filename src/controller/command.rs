use crate::controller::ControllerResult;
use crate::controller::ex;
use crate::controller::exmap;
use crate::editor::Editor;
use crate::editor::buffers::VimBuffers;
use crate::ui::colorscheme;
use vim_input as actions;

pub struct Command {
    pub cmd: String,
    pub exmap: exmap::ExMap,
}

impl Command {
    pub fn new() -> Self {
        Self {
            cmd: String::new(),
            exmap: exmap::ExMap::new(),
        }
    }

    pub fn set(&mut self, text: &str) {
        self.cmd = text.to_string();
    }

    pub fn clear(&mut self) {
        self.cmd.clear();
    }

    pub fn get_text(&self) -> String {
        self.cmd.clone()
    }

    pub fn try_resolve_action(
        &self,
        cmd: &ex::ExCommand,
        _editor: &mut Editor,
        _buffer_manager: &mut VimBuffers,
    ) -> actions::Action {
        if let Some(range) = &cmd.range {
            if let (Some(start), Some(end)) = (range.start_line, range.end_line) {
                match cmd.op {
                    ex::Ex::Delete => {
                        return actions::Action::DeleteLines {
                            start_line: start,
                            end_line: end,
                        };
                    }
                    ex::Ex::Yank => {
                        return actions::Action::YankLines {
                            start_line: start,
                            end_line: end,
                        };
                    }
                    _ => {}
                }
            }
        }
        actions::Action::NoOp
    }

    pub fn ex(
        &mut self,
        ui: &mut crate::ui::Ui,
        editor: &mut Editor,
        buffers: &mut VimBuffers,
    ) -> Option<ControllerResult> {
        let cmd_text = self.get_text();
        if let Some(resolved) = self.exmap.try_resolve(&cmd_text) {
            let action = self.try_resolve_action(&resolved, editor, buffers);
            if action != actions::Action::NoOp {
                return Some(ControllerResult::Action(action.clone()));
            }
            match resolved.op {
                ex::Ex::Set => {
                    if let Some(args) = &resolved.arguments {
                        for arg in args {
                            match arg.as_str() {
                                "wrap" => editor.wrap = true,
                                "nowrap" => editor.wrap = false,
                                "nu" => editor.show_line_numbers = true,
                                "nonu" => editor.show_line_numbers = false,
                                "number" => editor.show_line_numbers = true,
                                "nonumber" => editor.show_line_numbers = false,
                                "fold" => editor.fold = true,
                                "nofold" => editor.fold = false,
                                "foldmultiline" => editor.fold_multiline_only = true,
                                "nofoldmultiline" => editor.fold_multiline_only = false,
                                "tree" => editor.set_tree_sitter_enabled(ui, buffers, true),
                                "notree" => editor.set_tree_sitter_enabled(ui, buffers, false),
                                "treesitter" => editor.set_tree_sitter_enabled(ui, buffers, true),
                                "notreesitter" => {
                                    editor.set_tree_sitter_enabled(ui, buffers, false)
                                }
                                "mapsc" | "mapscopetoscheme" => {
                                    editor.map_scope_to_scheme = true;
                                    ui.clear_highlights();
                                }

                                "nomapsc" | "nomapscopetoscheme" => {
                                    editor.map_scope_to_scheme = false;
                                    ui.clear_highlights();
                                }
                                "textmate" | "tm" => {
                                    editor.textmate_highlights = true;
                                    ui.clear_highlights();
                                }
                                "notextmate" | "notm" => {
                                    editor.textmate_highlights = false;
                                    ui.clear_highlights();
                                }
                                "ts" | "tshl" => {
                                    editor.treesitter_highlights = true;
                                    ui.clear_highlights();
                                }
                                "nots" | "notshl" => {
                                    editor.treesitter_highlights = false;
                                    ui.clear_highlights();
                                }
                                _ => {}
                            }
                        }
                    }
                    None
                }
                ex::Ex::Write => {
                    if let Some(win) = ui.get_focused_window_mut()
                        && let Some(doc) = win.doc.as_ref()
                        && let Some(id) = vim_buffer::BufferId::new(doc.id as u64)
                        && let Ok(buf) = buffers.get(id)
                    {
                        let path = resolved
                            .arguments
                            .as_ref()
                            .and_then(|args| args.first())
                            .cloned();
                        if let Some(p) = path {
                            let content: String = buf.snapshot().chunks().collect();
                            let _ = std::fs::write(&p, content);
                        } else {
                            let _ = buffers.save(id, false);
                        }
                    }
                    None
                }
                ex::Ex::Edit => {
                    if let Some(win) = ui.get_focused_window_mut() {
                        let path = resolved
                            .arguments
                            .as_ref()
                            .and_then(|args| args.first())
                            .map(|s| s.clone());
                        if let Some(p) = path {
                            if let Ok(new_buf) = buffers.add_buffer_for_path(&p) {
                                win.set_vim_buffer(new_buf.id, buffers);
                            }
                        }
                    }
                    None
                }
                ex::Ex::Quit => Some(ControllerResult::Exit),
                ex::Ex::Colorschemes => {
                    let name = resolved
                        .arguments
                        .as_ref()
                        .and_then(|args| args.first())
                        .map(|s| s.as_str())
                        .unwrap_or("tokyonight");
                    let loaded = colorscheme::ColorScheme::get_by_name(name)
                        .unwrap_or_else(|| colorscheme::ColorScheme::load_default());
                    ui.set_colorscheme(loaded);
                    None
                }
                ex::Ex::Syntax => {
                    let arg = resolved
                        .arguments
                        .as_ref()
                        .and_then(|args| args.first())
                        .map(|s| s.as_str());
                    match arg {
                        Some("on") => editor.syntax = true,
                        Some("off") => editor.syntax = false,
                        _ => {}
                    }
                    None
                }
                ex::Ex::Bnext => {
                    if let Some(win) = ui.get_focused_window_mut() {
                        win.vim_bnext(buffers);
                    }
                    None
                }
                ex::Ex::Bprev => {
                    if let Some(win) = ui.get_focused_window_mut() {
                        win.vim_bprev(buffers);
                    }
                    None
                }
                ex::Ex::Split => {
                    let file_path = resolved
                        .arguments
                        .as_ref()
                        .and_then(|args| args.first())
                        .cloned();
                    Some(ControllerResult::Action(actions::Action::SplitHorizontal {
                        file_path,
                    }))
                }
                ex::Ex::Vsplit => {
                    let file_path = resolved
                        .arguments
                        .as_ref()
                        .and_then(|args| args.first())
                        .cloned();
                    Some(ControllerResult::Action(actions::Action::SplitVertical {
                        file_path,
                    }))
                }
                ex::Ex::Close => Some(ControllerResult::Action(actions::Action::CloseWindow)),
                ex::Ex::Only => Some(ControllerResult::Action(actions::Action::OnlyWindow)),
                _ => None,
            }
        } else {
            None
        }
    }
}
