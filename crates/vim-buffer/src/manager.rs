use crate::{
    Buffer, BufferError, BufferId, BufferLifecycle, ByteOffset, EditOrigin, ExternalFileStatus,
    FileMetadata, LoadSource, ManagerOutcome, MutationOutcome, SaveOutcome, TextRange,
    io::{atomic_write, decode_utf8, encode_utf8},
};
use clock::ReplicaId;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Default)]
pub struct BufferManager {
    buffers: HashMap<BufferId, Buffer>,
    names: HashMap<PathBuf, BufferId>,
    next_id: u64,
    current: Option<BufferId>,
    alternate: Option<BufferId>,
    mru: Vec<BufferId>,
}

impl BufferManager {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            ..Self::default()
        }
    }

    pub fn create(&mut self, initial_text: impl Into<String>) -> &mut Buffer {
        let id = self.insert_buffer(initial_text.into(), None);
        self.buffers.get_mut(&id).expect("newly inserted buffer")
    }

    /// Creates an in-memory loaded buffer associated with a canonicalized name.
    ///
    /// This does not read the file system; Phase 5 owns decoding and file I/O.
    pub fn create_named(
        &mut self,
        name: impl AsRef<Path>,
        initial_text: impl Into<String>,
    ) -> Result<(BufferId, ManagerOutcome), BufferError> {
        let name = canonical_name(name.as_ref())?;
        if let Some(id) = self.names.get(&name).copied() {
            return Ok((id, ManagerOutcome::Existing(id)));
        }
        let id = self.insert_buffer(initial_text.into(), Some(name));
        Ok((id, ManagerOutcome::Added(id)))
    }

    fn insert_buffer(&mut self, initial_text: String, name: Option<PathBuf>) -> BufferId {
        let id = BufferId::new(self.next_id).expect("buffer ID allocator overflowed");
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("buffer ID allocator overflowed");
        let mut buffer = Buffer::new(id, ReplicaId::LOCAL, initial_text);
        if let Some(name) = name {
            buffer.set_file_metadata(FileMetadata {
                path: Some(name.clone()),
                source: LoadSource::Generated,
                modified: None,
                size: None,
            });
            self.names.insert(name, id);
        }
        self.buffers.insert(id, buffer);
        id
    }

    pub fn load(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<(BufferId, ManagerOutcome), BufferError> {
        let path = canonical_name(path.as_ref())?;
        if let Some(id) = self.names.get(&path).copied() {
            return Ok((id, ManagerOutcome::Existing(id)));
        }
        let bytes = fs::read(&path)?;
        let decoded = decode_utf8(&bytes)?;
        let metadata = fs::metadata(&path)?;
        let id = self.insert_buffer(decoded.text, Some(path.clone()));
        let buffer = self.buffers.get_mut(&id).expect("newly inserted buffer");
        let mut options = buffer.options().clone();
        options.fileformat = decoded.fileformat;
        options.endofline = decoded.endofline;
        buffer.set_options(options)?;
        buffer.set_file_metadata(FileMetadata {
            path: Some(path),
            source: LoadSource::File,
            modified: metadata.modified().ok(),
            size: Some(metadata.len()),
        });
        buffer.mark_saved();
        Ok((id, ManagerOutcome::Loaded(id)))
    }

    pub fn check_external_change(&self, id: BufferId) -> Result<ExternalFileStatus, BufferError> {
        let file = self.get(id)?.file_metadata();
        let path = file.path.as_deref().ok_or(BufferError::NoFileName(id))?;
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ExternalFileStatus::Deleted);
            }
            Err(error) => return Err(BufferError::Io(error)),
        };
        let modified = metadata.modified().ok();
        if file.size != Some(metadata.len()) || file.modified != modified {
            Ok(ExternalFileStatus::Modified)
        } else {
            Ok(ExternalFileStatus::Unchanged)
        }
    }

    pub fn save(&mut self, id: BufferId, force: bool) -> Result<SaveOutcome, BufferError> {
        let path = self
            .get(id)?
            .path()
            .map(Path::to_path_buf)
            .ok_or(BufferError::NoFileName(id))?;
        self.save_to(id, path, force, false)
    }

    pub fn save_as(
        &mut self,
        id: BufferId,
        path: impl AsRef<Path>,
        force: bool,
    ) -> Result<SaveOutcome, BufferError> {
        let path = canonical_name(path.as_ref())?;
        if self
            .names
            .get(&path)
            .is_some_and(|existing| *existing != id)
        {
            return Err(BufferError::DuplicateBufferName(path));
        }
        self.save_to(id, path, force, true)
    }

    fn save_to(
        &mut self,
        id: BufferId,
        path: PathBuf,
        force: bool,
        rename: bool,
    ) -> Result<SaveOutcome, BufferError> {
        let buffer = self.get(id)?;
        if buffer.options().readonly && !force {
            return Err(BufferError::ReadOnly(id));
        }
        let bytes = encode_utf8(
            buffer.snapshot().as_inner().text().as_ref(),
            buffer.options(),
        )?;
        atomic_write(&path, &bytes)?;
        let metadata = fs::metadata(&path)?;
        let old_path = self.get(id)?.path().map(Path::to_path_buf);
        if rename && old_path.as_ref() != Some(&path) {
            if let Some(old_path) = old_path {
                self.names.remove(&old_path);
            }
            self.names.insert(path.clone(), id);
        }
        let buffer = self.get_mut(id)?;
        if buffer.options().fixeol && !buffer.options().binary && !buffer.options().endofline {
            let mut options = buffer.options().clone();
            options.endofline = true;
            buffer.set_options(options)?;
        }
        buffer.set_file_metadata(FileMetadata {
            path: Some(path.clone()),
            source: LoadSource::File,
            modified: metadata.modified().ok(),
            size: Some(metadata.len()),
        });
        buffer.mark_saved();
        Ok(SaveOutcome {
            buffer: id,
            path,
            bytes_written: bytes.len() as u64,
        })
    }

    pub fn reload(&mut self, id: BufferId, force: bool) -> Result<MutationOutcome, BufferError> {
        self.ensure_abandonable(id, force)?;
        let path = self
            .get(id)?
            .path()
            .map(Path::to_path_buf)
            .ok_or(BufferError::NoFileName(id))?;
        let bytes = fs::read(&path)?;
        let decoded = decode_utf8(&bytes)?;
        let metadata = fs::metadata(&path)?;
        let len = self.get(id)?.snapshot().len_bytes();
        let mut transaction = self.get_mut(id)?.transaction(EditOrigin::Reload);
        transaction.replace(
            None,
            TextRange::new(ByteOffset(0), ByteOffset(len)).expect("ordered full range"),
            decoded.text,
        );
        let outcome = transaction.commit(None)?;
        let buffer = self.get_mut(id)?;
        let mut options = buffer.options().clone();
        options.fileformat = decoded.fileformat;
        options.endofline = decoded.endofline;
        buffer.set_options(options)?;
        buffer.set_file_metadata(FileMetadata {
            path: Some(path),
            source: LoadSource::File,
            modified: metadata.modified().ok(),
            size: Some(metadata.len()),
        });
        buffer.mark_saved();
        Ok(outcome)
    }

    pub fn get(&self, id: BufferId) -> Result<&Buffer, BufferError> {
        self.buffers.get(&id).ok_or(BufferError::UnknownBuffer(id))
    }

    pub fn get_mut(&mut self, id: BufferId) -> Result<&mut Buffer, BufferError> {
        self.buffers
            .get_mut(&id)
            .ok_or(BufferError::UnknownBuffer(id))
    }

    pub fn find_by_name(&self, name: impl AsRef<Path>) -> Result<Option<BufferId>, BufferError> {
        let name = canonical_name(name.as_ref())?;
        Ok(self.names.get(&name).copied())
    }

    /// Returns all non-wiped buffers in Vim buffer-number order.
    pub fn list(&self) -> Vec<BufferId> {
        let mut ids = self.buffers.keys().copied().collect::<Vec<_>>();
        ids.sort_unstable();
        ids
    }

    pub fn listed(&self) -> Vec<BufferId> {
        self.list()
            .into_iter()
            .filter(|id| self.buffers[id].is_listed())
            .collect()
    }

    pub fn current(&self) -> Option<BufferId> {
        self.current
    }

    pub fn alternate(&self) -> Option<BufferId> {
        self.alternate
    }

    pub fn set_current(&mut self, id: BufferId) -> Result<ManagerOutcome, BufferError> {
        let target = self.get(id)?;
        if !target.is_loaded() {
            return Err(BufferError::InvalidLifecycleTransition);
        }
        let old = self.current;
        if old == Some(id) {
            self.touch_mru(id);
            return Ok(ManagerOutcome::CurrentChanged { old, new: id });
        }
        if let Some(old_id) = old {
            if let Some(old_buffer) = self.buffers.get_mut(&old_id) {
                if old_buffer.lifecycle() == BufferLifecycle::Loaded {
                    old_buffer.set_lifecycle(BufferLifecycle::Hidden);
                }
            }
            self.alternate = Some(old_id);
        }
        self.buffers
            .get_mut(&id)
            .expect("target was validated")
            .set_lifecycle(BufferLifecycle::Loaded);
        self.current = Some(id);
        self.touch_mru(id);
        Ok(ManagerOutcome::CurrentChanged { old, new: id })
    }

    pub fn unload(&mut self, id: BufferId, force: bool) -> Result<ManagerOutcome, BufferError> {
        self.ensure_abandonable(id, force)?;
        if self.current == Some(id) {
            self.select_replacement_for(id)?;
        }
        let buffer = self.get_mut(id)?;
        if !buffer.is_loaded() {
            return Err(BufferError::InvalidLifecycleTransition);
        }
        buffer.set_lifecycle(BufferLifecycle::Unloaded);
        self.remove_navigation_reference(id);
        Ok(ManagerOutcome::Unloaded(id))
    }

    pub fn delete(&mut self, id: BufferId, force: bool) -> Result<ManagerOutcome, BufferError> {
        self.ensure_abandonable(id, force)?;
        if self.current == Some(id) {
            self.select_replacement_for(id)?;
        }
        let buffer = self.get_mut(id)?;
        if matches!(
            buffer.lifecycle(),
            BufferLifecycle::Deleted | BufferLifecycle::Wiped
        ) {
            return Err(BufferError::InvalidLifecycleTransition);
        }
        buffer.set_listed(false);
        buffer.set_lifecycle(BufferLifecycle::Deleted);
        self.remove_navigation_reference(id);
        Ok(ManagerOutcome::Deleted(id))
    }

    pub fn wipe(&mut self, id: BufferId, force: bool) -> Result<ManagerOutcome, BufferError> {
        self.ensure_abandonable(id, force)?;
        if self.current == Some(id) {
            self.select_replacement_for(id)?;
        }
        let mut buffer = self
            .buffers
            .remove(&id)
            .ok_or(BufferError::UnknownBuffer(id))?;
        buffer.set_lifecycle(BufferLifecycle::Wiped);
        if let Some(path) = buffer.path() {
            self.names.remove(path);
        }
        self.remove_navigation_reference(id);
        Ok(ManagerOutcome::Wiped(id))
    }

    fn select_replacement_for(&mut self, id: BufferId) -> Result<(), BufferError> {
        let replacement = self
            .mru
            .iter()
            .copied()
            .chain(self.list())
            .find(|candidate| {
                *candidate != id
                    && self
                        .buffers
                        .get(candidate)
                        .is_some_and(|buffer| buffer.is_loaded())
            })
            .unwrap_or_else(|| self.insert_buffer(String::new(), None));
        self.set_current(replacement)?;
        Ok(())
    }

    fn ensure_abandonable(&self, id: BufferId, force: bool) -> Result<(), BufferError> {
        let buffer = self.get(id)?;
        if buffer.is_modified() && !force {
            return Err(BufferError::ModifiedBuffer(id));
        }
        Ok(())
    }

    fn touch_mru(&mut self, id: BufferId) {
        self.mru.retain(|candidate| *candidate != id);
        self.mru.insert(0, id);
    }

    fn remove_navigation_reference(&mut self, id: BufferId) {
        self.mru.retain(|candidate| *candidate != id);
        if self.alternate == Some(id) {
            self.alternate = None;
        }
    }

    /// Most-recently-used loaded buffer other than the current buffer.
    pub fn mru_alternate(&self) -> Option<BufferId> {
        self.mru.iter().copied().find(|id| {
            Some(*id) != self.current
                && self
                    .buffers
                    .get(id)
                    .is_some_and(|buffer| buffer.is_loaded())
        })
    }

    pub fn mru(&self) -> &[BufferId] {
        &self.mru
    }
}

fn canonical_name(path: &Path) -> Result<PathBuf, BufferError> {
    if path.is_absolute() {
        Ok(normalize_path(path))
    } else {
        Ok(normalize_path(&std::env::current_dir()?.join(path)))
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}
