//! Window and Tab page actions for Normal-mode.

use crate::kernel::{
    Editor,
    command::CommandContext,
    outcome::Outcome,
    window::tabpage::{Axis, NavigationDirection, TabPage},
};

pub fn split_horizontal(editor: &mut Editor, ctx: CommandContext) -> Outcome {
    split(editor, ctx, Axis::Horizontal)
}

pub fn split_vertical(editor: &mut Editor, ctx: CommandContext) -> Outcome {
    split(editor, ctx, Axis::Vertical)
}

fn split(editor: &mut Editor, ctx: CommandContext, axis: Axis) -> Outcome {
    let current_win = editor.window(ctx.window).expect("active window must exist");
    let cloned_win = current_win.clone();

    let new_win_id = editor.windows_mut().insert(cloned_win);

    let tab = editor.tabs_mut().active_mut();
    if tab.split_window(ctx.window, new_win_id, axis) {
        editor.set_current_window(new_win_id);
    } else {
        editor.windows_mut().remove(new_win_id);
    }
    Outcome::default()
}

pub fn close_window(editor: &mut Editor, ctx: CommandContext) -> Outcome {
    let tab = editor.tabs_mut().active_mut();
    let win_ids = tab.layout().window_ids();
    if win_ids.len() <= 1 {
        return Outcome::default();
    }

    if tab.remove_window(ctx.window) {
        let new_win = tab.active_window();
        editor.set_current_window(new_win);
        editor.windows_mut().remove(ctx.window);
    }
    Outcome::default()
}

pub fn only_window(editor: &mut Editor, ctx: CommandContext) -> Outcome {
    let tab = editor.tabs_mut().active_mut();
    let win_ids = tab.layout().window_ids();
    if win_ids.len() <= 1 {
        return Outcome::default();
    }

    *tab = TabPage::new(ctx.window);

    for w in win_ids {
        if w != ctx.window {
            editor.windows_mut().remove(w);
        }
    }
    Outcome::default()
}

pub fn focus_window(editor: &mut Editor, ctx: CommandContext, dir: NavigationDirection) -> Outcome {
    let tab = editor.tabs().active();
    if let Some(target_win) = tab.layout().navigate(ctx.window, dir) {
        editor.tabs_mut().active_mut().set_active_window(target_win);
        editor.set_current_window(target_win);
    }
    Outcome::default()
}

pub fn next_tab(editor: &mut Editor, count: u32) -> Outcome {
    let new_tab_id = editor.tabs_mut().next_tab(count as usize);
    editor.set_current_tab(new_tab_id);
    Outcome::default()
}

pub fn previous_tab(editor: &mut Editor, count: u32) -> Outcome {
    let new_tab_id = editor.tabs_mut().previous_tab(count as usize);
    editor.set_current_tab(new_tab_id);
    Outcome::default()
}

pub fn resize_window(
    editor: &mut Editor,
    ctx: CommandContext,
    dir: NavigationDirection,
    count: u32,
) -> Outcome {
    let axis = match dir {
        NavigationDirection::Up | NavigationDirection::Down => Axis::Horizontal,
        NavigationDirection::Left | NavigationDirection::Right => Axis::Vertical,
    };
    // Standard increment/decrement step (e.g. 10 weight units per count)
    let step = 10 * count as i32;
    let delta = match dir {
        NavigationDirection::Up | NavigationDirection::Right => step,
        NavigationDirection::Down | NavigationDirection::Left => -step,
    };

    let active_win = ctx.window;
    let tab = editor.tabs_mut().active_mut();
    tab.layout_mut().adjust_weight(active_win, axis, delta);
    Outcome::default()
}

pub fn resize_equal(editor: &mut Editor, _ctx: CommandContext) -> Outcome {
    let tab = editor.tabs_mut().active_mut();
    tab.layout_mut().equalize_weights();
    Outcome::default()
}

pub fn move_window(editor: &mut Editor, ctx: CommandContext, dir: NavigationDirection) -> Outcome {
    let active_win = ctx.window;
    let tab = editor.tabs_mut().active_mut();
    tab.move_window(active_win, dir);
    Outcome::default()
}
