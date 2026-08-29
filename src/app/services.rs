use crate::app::request::AppRequest;
use crate::kernel::outcome::Effect;

/// Translates kernel-side effects into application-level requests.
pub fn describe_effect(effect: &Effect) -> Option<AppRequest> {
    match effect {
        Effect::FileSaved {
            path,
            bytes_written,
        } => Some(AppRequest::ShowMessage(format!(
            "\"{}\" {}B written",
            path.display(),
            bytes_written
        ))),
        Effect::FileSaveFailed { message } => Some(AppRequest::ShowMessage(message.clone())),
        Effect::OptionMessage { message } => Some(AppRequest::ShowMessage(message.clone())),
        Effect::ClipboardWrite { text, primary } => {
            let reg_name = if *primary {
                vim_clipboard::RegisterName::Selection
            } else {
                vim_clipboard::RegisterName::System
            };
            vim_clipboard::write_system_clipboard(reg_name, text);
            None
        }
        Effect::ConfirmSubstitute { replacement, .. } => Some(AppRequest::ShowMessage(format!(
            "replace with {} (y/n/a/q/l)?",
            replacement
        ))),
        _ => None,
    }
}
