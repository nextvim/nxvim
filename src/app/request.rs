//! Typed application-level request envelope.

#[derive(Clone, Debug, PartialEq)]
pub enum AppRequest {
    Quit,
    ShowMessage(String),
    ExecuteEx(vim_script::ast::ExCommand),
    ExecuteExString(String),
    Source(std::path::PathBuf),
    FeedKeys { keys: String, mode: String },
    PopupCreate { lines: Vec<String>, options: std::collections::BTreeMap<String, vim_script::runtime::Value> },
    PopupClose { id: u64 },
    PopupSetText { id: u64, lines: Vec<String> },
}
