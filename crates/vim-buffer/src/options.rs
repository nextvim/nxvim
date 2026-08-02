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
        }
    }
}
