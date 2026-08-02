use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use vim_buffer::{
    BufferError, BufferManager, BufferOptions, ExternalFileStatus, FileFormat, decode_utf8,
    encode_utf8,
};

fn fixture_dir(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("nxvim-{name}-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn strict_decode_detects_fileformat_and_final_eol() {
    let unix = decode_utf8(b"one\ntwo").unwrap();
    assert_eq!(unix.text, "one\ntwo");
    assert_eq!(unix.fileformat, FileFormat::Unix);
    assert!(!unix.endofline);

    let dos = decode_utf8(b"one\r\ntwo\r\n").unwrap();
    assert_eq!(dos.text, "one\ntwo\n");
    assert_eq!(dos.fileformat, FileFormat::Dos);
    assert!(dos.endofline);

    let mac = decode_utf8(b"one\rtwo\r").unwrap();
    assert_eq!(mac.text, "one\ntwo\n");
    assert_eq!(mac.fileformat, FileFormat::Mac);
    assert!(mac.endofline);

    assert!(matches!(
        decode_utf8(&[0xff]),
        Err(BufferError::DecodeUtf8(_))
    ));
}

#[test]
fn encoding_honors_fileformat_eol_fixeol_and_binary() {
    let mut options = BufferOptions::default();
    options.fileformat = FileFormat::Dos;
    assert_eq!(
        encode_utf8("one\ntwo", &options).unwrap(),
        b"one\r\ntwo\r\n"
    );

    options.endofline = false;
    options.fixeol = false;
    assert_eq!(encode_utf8("one\ntwo\n", &options).unwrap(), b"one\r\ntwo");

    options.fixeol = true;
    options.binary = true;
    assert_eq!(encode_utf8("one\ntwo", &options).unwrap(), b"one\r\ntwo");
}

#[test]
fn load_save_as_and_reload_update_file_state() {
    let directory = fixture_dir("roundtrip");
    let source = directory.join("source.txt");
    let destination = directory.join("saved.txt");
    fs::write(&source, b"one\r\ntwo").unwrap();

    let mut manager = BufferManager::new();
    let (id, _) = manager.load(&source).unwrap();
    let buffer = manager.get(id).unwrap();
    assert_eq!(buffer.snapshot().as_inner().text(), "one\ntwo");
    assert_eq!(buffer.options().fileformat, FileFormat::Dos);
    assert!(!buffer.options().endofline);
    assert!(!buffer.is_modified());

    manager.save_as(id, &destination, false).unwrap();
    assert_eq!(fs::read(&destination).unwrap(), b"one\r\ntwo\r\n");
    assert!(!manager.get(id).unwrap().is_modified());

    fs::write(&destination, b"external\n").unwrap();
    manager.reload(id, false).unwrap();
    assert_eq!(
        manager.get(id).unwrap().snapshot().as_inner().text(),
        "external\n"
    );
    assert!(!manager.get(id).unwrap().is_modified());

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn readonly_requires_force_but_does_not_block_edits() {
    let directory = fixture_dir("readonly");
    let path = directory.join("file.txt");
    fs::write(&path, b"text\n").unwrap();
    let mut manager = BufferManager::new();
    let (id, _) = manager.load(&path).unwrap();
    let mut options = manager.get(id).unwrap().options().clone();
    options.readonly = true;
    manager.get_mut(id).unwrap().set_options(options).unwrap();

    assert!(matches!(manager.save(id, false), Err(BufferError::ReadOnly(found)) if found == id));
    manager.save(id, true).unwrap();
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn reports_external_modification_and_deletion() {
    let directory = fixture_dir("external");
    let path = directory.join("file.txt");
    fs::write(&path, b"text\n").unwrap();
    let mut manager = BufferManager::new();
    let (id, _) = manager.load(&path).unwrap();
    assert_eq!(
        manager.check_external_change(id).unwrap(),
        ExternalFileStatus::Unchanged
    );

    fs::write(&path, b"longer external text\n").unwrap();
    assert_eq!(
        manager.check_external_change(id).unwrap(),
        ExternalFileStatus::Modified
    );
    fs::remove_file(&path).unwrap();
    assert_eq!(
        manager.check_external_change(id).unwrap(),
        ExternalFileStatus::Deleted
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn wiping_the_only_current_buffer_selects_a_fresh_buffer() {
    let mut manager = BufferManager::new();
    let id = manager.create("").id();
    manager.set_current(id).unwrap();
    manager.wipe(id, false).unwrap();

    let replacement = manager.current().unwrap();
    assert_ne!(replacement, id);
    assert!(manager.get(replacement).unwrap().is_loaded());
}
