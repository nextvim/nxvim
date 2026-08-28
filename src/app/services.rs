use crate::kernel::outcome::Effect;
use crate::app::request::AppRequest;

/// Translates kernel-side effects into application-level requests.
pub fn describe_effect(effect: &Effect) -> Option<AppRequest> {
    match effect {
        Effect::FileSaved { path, bytes_written } => {
            Some(AppRequest::ShowMessage(format!(
                "\"{}\" {}B written",
                path.display(),
                bytes_written
            )))
        }
        Effect::FileSaveFailed { message } => {
            Some(AppRequest::ShowMessage(message.clone()))
        }
        Effect::OptionMessage { message } => {
            Some(AppRequest::ShowMessage(message.clone()))
        }
        _ => None,
    }
}
