//! Typed application-level request envelope.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppRequest {
    Quit,
    ShowMessage(String),
}
