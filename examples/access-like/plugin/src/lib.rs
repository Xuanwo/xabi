use std::collections::{BTreeSet, HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use access_like_abi::oio::{CopyAbi, DeleteAbi, ListAbi, ReadAbi, WriteAbi};
#[cfg(feature = "python")]
use access_like_abi::ACCESS_TRAIT_ID;
use access_like_abi::{
    AccessAbi, AccessorInfo, BytesRange, CopyResult, DeleteResult, Entry, Error, ListResult,
    Metadata, OpCopier, OpCopy, OpCreateDir, OpDelete, OpList, OpPresign, OpRead, OpRename, OpStat,
    OpWrite, PresignedRequest, ReadResult, Result, RpCopy, RpCreateDir, RpDelete, RpList,
    RpPresign, RpRead, RpRename, RpStat, RpWrite, WriteResult, PRESIGN_DELETE, PRESIGN_READ,
    PRESIGN_STAT, PRESIGN_WRITE,
};
#[cfg(feature = "python")]
use pyo3::prelude::*;

#[derive(Debug, Clone)]
struct DemoAccess {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    dirs: Arc<Mutex<BTreeSet<String>>>,
}

impl Default for DemoAccess {
    fn default() -> Self {
        let mut dirs = BTreeSet::new();
        dirs.insert("/".to_string());
        Self {
            objects: Arc::new(Mutex::new(HashMap::new())),
            dirs: Arc::new(Mutex::new(dirs)),
        }
    }
}

impl DemoAccess {
    fn metadata_for(&self, path: &str) -> Result<Metadata> {
        if let Some(data) = self.objects.lock().unwrap().get(path) {
            return Ok(Metadata::file(data.len() as u64));
        }
        if self.dirs.lock().unwrap().contains(path) {
            return Ok(Metadata::dir());
        }
        Err(Error::not_found(path))
    }
}

impl DemoAccess {
    fn info(&self) -> AccessorInfo {
        AccessorInfo::memory()
    }

    async fn create_dir(
        &self,
        path: &str,
        _args: OpCreateDir,
    ) -> std::result::Result<RpCreateDir, Error> {
        self.dirs.lock().unwrap().insert(path.to_string());
        Ok(RpCreateDir::new())
    }

    async fn stat(&self, path: &str, _args: OpStat) -> std::result::Result<RpStat, Error> {
        Ok(RpStat::new(self.metadata_for(path)?))
    }

    async fn read(&self, path: &str, args: OpRead) -> std::result::Result<ReadResult, Error> {
        let data = self
            .objects
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| Error::not_found(path))?;
        let full_len = data.len() as u64;
        let selected = apply_range(data, args.range)?;
        let reader = DemoReader::new(selected, 4);
        Ok(ReadResult::from_reader(
            RpRead::new(Some(Metadata::file(full_len))),
            reader,
        ))
    }

    async fn write(&self, path: &str, args: OpWrite) -> std::result::Result<WriteResult, Error> {
        if args.if_not_exists && self.objects.lock().unwrap().contains_key(path) {
            return Err(Error::already_exists(path));
        }
        let writer = DemoWriter {
            path: path.to_string(),
            objects: Arc::clone(&self.objects),
            buffer: Vec::new(),
            append: args.append,
            aborted: false,
        };
        Ok(WriteResult::from_writer(RpWrite::new(), writer))
    }

    async fn delete(&self) -> std::result::Result<DeleteResult, Error> {
        Ok(DeleteResult::from_deleter(
            RpDelete::new(),
            DemoDeleter {
                objects: Arc::clone(&self.objects),
                dirs: Arc::clone(&self.dirs),
                pending: Vec::new(),
            },
        ))
    }

    async fn list(&self, path: &str, args: OpList) -> std::result::Result<ListResult, Error> {
        let mut entries = Vec::new();
        for dir in self.dirs.lock().unwrap().iter() {
            if dir == path {
                continue;
            }
            if should_list(path, dir, args.recursive) {
                entries.push(Entry::new(dir.clone(), Metadata::dir()));
            }
        }
        for (object_path, data) in self.objects.lock().unwrap().iter() {
            if should_list(path, object_path, args.recursive) {
                entries.push(Entry::new(
                    object_path.clone(),
                    Metadata::file(data.len() as u64),
                ));
            }
        }
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        if let Some(start_after) = args.start_after {
            entries.retain(|entry| entry.path > start_after);
        }
        if let Some(limit) = args.limit {
            entries.truncate(limit);
        }
        Ok(ListResult::from_lister(
            RpList::new(),
            DemoLister {
                entries: entries.into(),
            },
        ))
    }

    async fn copy(
        &self,
        from: &str,
        to: &str,
        args: OpCopy,
        opts: OpCopier,
    ) -> std::result::Result<CopyResult, Error> {
        if args.if_not_exists && self.objects.lock().unwrap().contains_key(to) {
            return Err(Error::already_exists(to));
        }
        let copier = DemoCopier {
            objects: Arc::clone(&self.objects),
            from: from.to_string(),
            to: to.to_string(),
            copied: false,
            aborted: false,
            chunk_hint: opts.chunk,
        };
        Ok(CopyResult::from_copier(RpCopy::new(), copier))
    }

    async fn rename(
        &self,
        from: &str,
        to: &str,
        _args: OpRename,
    ) -> std::result::Result<RpRename, Error> {
        let mut objects = self.objects.lock().unwrap();
        let data = objects.remove(from).ok_or_else(|| Error::not_found(from))?;
        objects.insert(to.to_string(), data);
        Ok(RpRename::new())
    }

    async fn presign(&self, path: &str, args: OpPresign) -> std::result::Result<RpPresign, Error> {
        let method = match args.operation {
            PRESIGN_STAT => "HEAD",
            PRESIGN_READ => "GET",
            PRESIGN_WRITE => "PUT",
            PRESIGN_DELETE => "DELETE",
            _ => return Err(Error::unsupported("presign operation")),
        };
        Ok(RpPresign::new(PresignedRequest::new(
            method.to_string(),
            format!("memory://demo/{path}?expires={}", args.expire_millis),
            b"x-demo: access-like".to_vec(),
        )))
    }
}

#[derive(Debug)]
struct DemoReader {
    chunks: VecDeque<Vec<u8>>,
}

impl DemoReader {
    fn new(data: Vec<u8>, chunk_size: usize) -> Self {
        let mut chunks = VecDeque::new();
        for chunk in data.chunks(chunk_size) {
            chunks.push_back(chunk.to_vec());
        }
        Self { chunks }
    }
}

impl ReadAbi for DemoReader {
    async fn read(&mut self) -> std::result::Result<Vec<u8>, Error> {
        Ok(self.chunks.pop_front().unwrap_or_default())
    }
}

#[derive(Debug)]
struct DemoWriter {
    path: String,
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    buffer: Vec<u8>,
    append: bool,
    aborted: bool,
}

impl WriteAbi for DemoWriter {
    async fn write(&mut self, data: &[u8]) -> std::result::Result<(), Error> {
        if self.aborted {
            return Err(Error::other("writer was aborted"));
        }
        self.buffer.extend_from_slice(data);
        Ok(())
    }

    async fn close(&mut self) -> std::result::Result<Metadata, Error> {
        if self.aborted {
            return Err(Error::other("writer was aborted"));
        }
        let mut objects = self.objects.lock().unwrap();
        if self.append {
            objects
                .entry(self.path.clone())
                .or_default()
                .extend_from_slice(&self.buffer);
        } else {
            objects.insert(self.path.clone(), self.buffer.clone());
        }
        Ok(Metadata::file(self.buffer.len() as u64))
    }

    async fn abort(&mut self) -> std::result::Result<(), Error> {
        self.aborted = true;
        self.buffer.clear();
        Ok(())
    }
}

#[derive(Debug)]
struct DemoLister {
    entries: VecDeque<Entry>,
}

impl ListAbi for DemoLister {
    async fn next(&mut self) -> std::result::Result<Option<Entry>, Error> {
        Ok(self.entries.pop_front())
    }
}

#[derive(Debug)]
struct DemoDeleter {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    dirs: Arc<Mutex<BTreeSet<String>>>,
    pending: Vec<(String, OpDelete)>,
}

impl DeleteAbi for DemoDeleter {
    async fn delete(&mut self, path: &str, args: OpDelete) -> std::result::Result<(), Error> {
        self.pending.push((path.to_string(), args));
        Ok(())
    }

    async fn close(&mut self) -> std::result::Result<(), Error> {
        let mut objects = self.objects.lock().unwrap();
        let mut dirs = self.dirs.lock().unwrap();
        for (path, args) in self.pending.drain(..) {
            objects.remove(&path);
            if args.recursive {
                objects.retain(|object_path, _| !object_path.starts_with(&path));
                dirs.retain(|dir_path| !dir_path.starts_with(&path) || dir_path == "/");
            } else {
                dirs.remove(&path);
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
struct DemoCopier {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    from: String,
    to: String,
    copied: bool,
    aborted: bool,
    chunk_hint: Option<usize>,
}

impl CopyAbi for DemoCopier {
    async fn next(&mut self) -> std::result::Result<Option<usize>, Error> {
        if self.aborted || self.copied {
            return Ok(None);
        }
        let mut objects = self.objects.lock().unwrap();
        let data = objects
            .get(&self.from)
            .cloned()
            .ok_or_else(|| Error::not_found(&self.from))?;
        let reported = self.chunk_hint.unwrap_or(data.len()).min(data.len());
        objects.insert(self.to.clone(), data);
        self.copied = true;
        Ok(Some(reported))
    }

    async fn close(&mut self) -> std::result::Result<Metadata, Error> {
        if !self.copied {
            let _ = self.next().await?;
        }
        let len = self
            .objects
            .lock()
            .unwrap()
            .get(&self.to)
            .map(|data| data.len() as u64)
            .unwrap_or(0);
        Ok(Metadata::file(len))
    }

    async fn abort(&mut self) -> std::result::Result<(), Error> {
        self.aborted = true;
        Ok(())
    }
}

fn apply_range(data: Vec<u8>, range: BytesRange) -> Result<Vec<u8>> {
    let offset = range.offset.unwrap_or(0) as usize;
    if offset > data.len() {
        return Ok(Vec::new());
    }
    let end = match range.size {
        Some(size) => offset.saturating_add(size as usize).min(data.len()),
        None => data.len(),
    };
    Ok(data[offset..end].to_vec())
}

fn should_list(root: &str, path: &str, recursive: bool) -> bool {
    if !path.starts_with(root) {
        return false;
    }
    if recursive {
        return true;
    }
    let rest = path.trim_start_matches(root);
    !rest.trim_end_matches('/').contains('/')
}

#[xabi::module]
mod exports {
    use super::*;

    #[xabi::xabi(name = "demo-access", version = 1)]
    impl AccessAbi for DemoAccess {
        fn info(&self) -> AccessorInfo {
            DemoAccess::info(self)
        }

        async fn create_dir(
            &self,
            path: &str,
            args: OpCreateDir,
        ) -> std::result::Result<RpCreateDir, Error> {
            DemoAccess::create_dir(self, path, args).await
        }

        async fn stat(&self, path: &str, args: OpStat) -> std::result::Result<RpStat, Error> {
            DemoAccess::stat(self, path, args).await
        }

        async fn read(&self, path: &str, args: OpRead) -> std::result::Result<ReadResult, Error> {
            DemoAccess::read(self, path, args).await
        }

        async fn write(
            &self,
            path: &str,
            args: OpWrite,
        ) -> std::result::Result<WriteResult, Error> {
            DemoAccess::write(self, path, args).await
        }

        async fn delete(&self) -> std::result::Result<DeleteResult, Error> {
            DemoAccess::delete(self).await
        }

        async fn list(&self, path: &str, args: OpList) -> std::result::Result<ListResult, Error> {
            DemoAccess::list(self, path, args).await
        }

        async fn copy(
            &self,
            from: &str,
            to: &str,
            args: OpCopy,
            opts: OpCopier,
        ) -> std::result::Result<CopyResult, Error> {
            DemoAccess::copy(self, from, to, args, opts).await
        }

        async fn rename(
            &self,
            from: &str,
            to: &str,
            args: OpRename,
        ) -> std::result::Result<RpRename, Error> {
            DemoAccess::rename(self, from, to, args).await
        }

        async fn presign(
            &self,
            path: &str,
            args: OpPresign,
        ) -> std::result::Result<RpPresign, Error> {
            DemoAccess::presign(self, path, args).await
        }
    }
}

#[cfg(feature = "python")]
#[pyfunction]
fn abi_id() -> String {
    ACCESS_TRAIT_ID.to_string()
}

#[cfg(feature = "python")]
#[pyfunction]
fn native_plugin_name() -> String {
    "demo-access".to_string()
}

#[cfg(feature = "python")]
#[pyfunction]
fn export_version() -> u32 {
    1
}

#[cfg(feature = "python")]
#[pymodule]
fn _access_like_plugin(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(abi_id, m)?)?;
    m.add_function(wrap_pyfunction!(native_plugin_name, m)?)?;
    m.add_function(wrap_pyfunction!(export_version, m)?)?;
    Ok(())
}
