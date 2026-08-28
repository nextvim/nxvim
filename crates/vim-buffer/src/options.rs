#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileFormat {
    #[default]
    Unix,
    Dos,
    Mac,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BufferOptions {
    pub modifiable: bool,
    pub readonly: bool,
    pub binary: bool,
    pub endofline: bool,
    pub fixeol: bool,
    pub fileformat: FileFormat,
    pub fileencoding: String,
    pub expandtab: bool,
    pub textwidth: u32,
    pub shiftwidth: u32,
    pub tabstop: u32,
}

impl From<text::LineEnding> for FileFormat {
    fn from(line_ending: text::LineEnding) -> Self {
        match line_ending {
            text::LineEnding::Unix => Self::Unix,
            text::LineEnding::Windows => Self::Dos,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedFileFormat;

impl TryFrom<FileFormat> for text::LineEnding {
    type Error = UnsupportedFileFormat;

    fn try_from(format: FileFormat) -> Result<Self, Self::Error> {
        match format {
            FileFormat::Unix => Ok(Self::Unix),
            FileFormat::Dos => Ok(Self::Windows),
            FileFormat::Mac => Err(UnsupportedFileFormat),
        }
    }
}

impl BufferOptions {
    pub(crate) fn file_state_eq(&self, other: &Self) -> bool {
        self.binary == other.binary
            && self.endofline == other.endofline
            && self.fixeol == other.fixeol
            && self.fileformat == other.fileformat
            && self.fileencoding == other.fileencoding
    }
}

impl Default for BufferOptions {
    fn default() -> Self {
        Self {
            modifiable: true,
            readonly: false,
            binary: false,
            endofline: true,
            fixeol: true,
            fileformat: FileFormat::Unix,
            fileencoding: "utf-8".into(),
            expandtab: false,
            textwidth: 0,
            shiftwidth: 8,
            tabstop: 8,
        }
    }
}
