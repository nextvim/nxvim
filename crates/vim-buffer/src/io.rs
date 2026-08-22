use crate::{BufferError, BufferOptions, FileFormat};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::SystemTime,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileMetadata {
    pub path: Option<PathBuf>,
    pub source: LoadSource,
    pub modified: Option<SystemTime>,
    pub size: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoadSource {
    File,
    Stdin,
    #[default]
    Scratch,
    Generated,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalFileStatus {
    Unchanged,
    Modified,
    Deleted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedFile {
    pub text: String,
    pub fileformat: FileFormat,
    pub endofline: bool,
}

pub fn decode_utf8(bytes: &[u8]) -> Result<DecodedFile, BufferError> {
    let source = std::str::from_utf8(bytes).map_err(BufferError::DecodeUtf8)?;
    let fileformat = detect_fileformat(bytes);
    let endofline = match fileformat {
        FileFormat::Unix => bytes.ends_with(b"\n"),
        FileFormat::Dos => bytes.ends_with(b"\r\n"),
        FileFormat::Mac => bytes.ends_with(b"\r"),
    };
    let text = match fileformat {
        FileFormat::Unix => source.replace("\r\n", "\n"),
        FileFormat::Dos => source.replace("\r\n", "\n"),
        FileFormat::Mac => source.replace('\r', "\n"),
    };
    Ok(DecodedFile {
        text,
        fileformat,
        endofline,
    })
}

fn detect_fileformat(bytes: &[u8]) -> FileFormat {
    if bytes.windows(2).any(|pair| pair == b"\r\n") {
        FileFormat::Dos
    } else if bytes.contains(&b'\n') {
        FileFormat::Unix
    } else if bytes.contains(&b'\r') {
        FileFormat::Mac
    } else {
        FileFormat::Unix
    }
}

pub fn encode_utf8(text: &str, options: &BufferOptions) -> Result<Vec<u8>, BufferError> {
    if !options.fileencoding.eq_ignore_ascii_case("utf-8") {
        return Err(BufferError::UnsupportedEncoding(
            options.fileencoding.clone(),
        ));
    }
    let want_eol = options.endofline || (options.fixeol && !options.binary);
    let mut logical = text.to_owned();
    match (want_eol, logical.ends_with('\n')) {
        (true, false) => logical.push('\n'),
        (false, true) => {
            logical.pop();
        }
        _ => {}
    }
    Ok(match options.fileformat {
        FileFormat::Unix => logical.into_bytes(),
        FileFormat::Dos => logical.replace('\n', "\r\n").into_bytes(),
        FileFormat::Mac => logical.replace('\n', "\r").into_bytes(),
    })
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), BufferError> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        BufferError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "file path has no file name",
        ))
    })?;
    let permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());

    for attempt in 0..100u32 {
        let temporary = parent.join(format!(
            ".{}.nxvim-{}-{attempt}.tmp",
            file_name.to_string_lossy(),
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(mut file) => {
                let result = (|| {
                    if let Some(permissions) = permissions.clone() {
                        file.set_permissions(permissions)?;
                    }
                    file.write_all(bytes)?;
                    file.sync_all()?;
                    drop(file);
                    fs::rename(&temporary, path)?;
                    Ok::<_, std::io::Error>(())
                })();
                if result.is_err() {
                    let _ = fs::remove_file(&temporary);
                }
                return result.map_err(BufferError::Io);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(BufferError::Io(error)),
        }
    }
    Err(BufferError::Io(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate an atomic-write temporary file",
    )))
}
