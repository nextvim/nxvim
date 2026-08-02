use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileMetadata {
    pub path: Option<PathBuf>,
    pub source: LoadSource,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoadSource {
    File,
    Stdin,
    #[default]
    Scratch,
    Generated,
}
