#![allow(async_fn_in_trait)]

use std::fmt;
use std::sync::Arc;

pub const ACCESS_TRAIT_ID: &str = "opendal.Access";
pub const ABI_VERSION: u32 = 1;

pub const ENTRY_MODE_UNKNOWN: u32 = 0;
pub const ENTRY_MODE_FILE: u32 = 1;
pub const ENTRY_MODE_DIR: u32 = 2;

pub const PRESIGN_STAT: u32 = 1;
pub const PRESIGN_READ: u32 = 2;
pub const PRESIGN_WRITE: u32 = 3;
pub const PRESIGN_DELETE: u32 = 4;

pub type Result<T> = std::result::Result<T, Error>;

#[xabi::data]
#[derive(Debug, Clone)]
pub struct Error {
    pub kind: u32,
    pub message: String,
}

impl Error {
    pub const KIND_OTHER: u32 = 1;
    pub const KIND_UNSUPPORTED: u32 = 2;
    pub const KIND_NOT_FOUND: u32 = 3;
    pub const KIND_ALREADY_EXISTS: u32 = 4;

    pub fn other(message: impl Into<String>) -> Self {
        Self::new(Self::KIND_OTHER, message.into())
    }

    pub fn unsupported(operation: &str) -> Self {
        Self::new(
            Self::KIND_UNSUPPORTED,
            format!("operation {operation} is not supported"),
        )
    }

    pub fn not_found(path: &str) -> Self {
        Self::new(Self::KIND_NOT_FOUND, format!("path {path} was not found"))
    }

    pub fn already_exists(path: &str) -> Self {
        Self::new(
            Self::KIND_ALREADY_EXISTS,
            format!("path {path} already exists"),
        )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<xabi::Error> for Error {
    fn from(value: xabi::Error) -> Self {
        Self::other(value.to_string())
    }
}

impl From<xabi::XabiCallError<Error>> for Error {
    fn from(value: xabi::XabiCallError<Error>) -> Self {
        match value {
            xabi::XabiCallError::Runtime(err) => Self::from(err),
            xabi::XabiCallError::Export(err) => err,
        }
    }
}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Capability {
    pub stat: bool,
    pub read: bool,
    pub write: bool,
    pub create_dir: bool,
    pub delete: bool,
    pub list: bool,
    pub copy: bool,
    pub rename: bool,
    pub presign: bool,
    pub read_with_if_match: bool,
    pub write_with_if_not_exists: bool,
    pub delete_with_recursive: bool,
    pub list_with_recursive: bool,
    pub copy_can_multi: bool,
}

#[xabi::data]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AccessorInfo {
    pub scheme: String,
    pub root: String,
    pub name: String,
    pub native_capability: Capability,
}

impl AccessorInfo {
    pub fn memory() -> Self {
        Self::new(
            "memory".to_string(),
            "/".to_string(),
            "demo".to_string(),
            Capability::new(
                true, true, true, true, true, true, true, true, true, true, true, true, true, true,
            ),
        )
    }
}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BytesRange {
    pub offset: Option<u64>,
    pub size: Option<u64>,
}

#[xabi::data]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Metadata {
    pub mode: u32,
    pub is_current: Option<bool>,
    pub is_deleted: bool,
    pub cache_control: Option<String>,
    pub content_disposition: Option<String>,
    pub content_length: Option<u64>,
    pub content_md5: Option<String>,
    pub content_type: Option<String>,
    pub content_encoding: Option<String>,
    pub etag: Option<String>,
    pub last_modified_millis: Option<u64>,
    pub version: Option<String>,
    pub user_metadata: Vec<u8>,
}

impl Metadata {
    pub fn file(content_length: u64) -> Self {
        Self::new(
            ENTRY_MODE_FILE,
            Some(true),
            false,
            None,
            None,
            Some(content_length),
            None,
            Some("application/octet-stream".to_string()),
            None,
            None,
            None,
            None,
            Vec::new(),
        )
    }

    pub fn dir() -> Self {
        Self::new(
            ENTRY_MODE_DIR,
            Some(true),
            false,
            None,
            None,
            Some(0),
            None,
            None,
            None,
            None,
            None,
            None,
            Vec::new(),
        )
    }
}

#[xabi::data]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub metadata: Metadata,
}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default)]
pub struct OpCreateDir {}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct OpDelete {
    pub version: Option<String>,
    pub recursive: bool,
}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct OpList {
    pub limit: Option<usize>,
    pub start_after: Option<String>,
    pub recursive: bool,
    pub versions: bool,
    pub deleted: bool,
}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct OpRead {
    pub range: BytesRange,
    pub if_match: Option<String>,
    pub if_none_match: Option<String>,
    pub if_modified_since_millis: Option<u64>,
    pub if_unmodified_since_millis: Option<u64>,
    pub override_content_type: Option<String>,
    pub override_cache_control: Option<String>,
    pub override_content_disposition: Option<String>,
    pub version: Option<String>,
}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct OpStat {
    pub if_match: Option<String>,
    pub if_none_match: Option<String>,
    pub if_modified_since_millis: Option<u64>,
    pub if_unmodified_since_millis: Option<u64>,
    pub override_content_type: Option<String>,
    pub override_cache_control: Option<String>,
    pub override_content_disposition: Option<String>,
    pub version: Option<String>,
}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct OpWrite {
    pub append: bool,
    pub concurrent: usize,
    pub content_type: Option<String>,
    pub content_disposition: Option<String>,
    pub content_encoding: Option<String>,
    pub cache_control: Option<String>,
    pub if_match: Option<String>,
    pub if_none_match: Option<String>,
    pub if_not_exists: bool,
    pub user_metadata: Vec<u8>,
}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct OpCopy {
    pub if_not_exists: bool,
    pub if_match: Option<String>,
}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct OpCopier {
    pub concurrent: usize,
    pub chunk: Option<usize>,
    pub source_content_length_hint: Option<u64>,
}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default)]
pub struct OpRename {}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default)]
pub struct OpPresign {
    pub expire_millis: u64,
    pub operation: u32,
}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default)]
pub struct RpCreateDir {}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default)]
pub struct RpDelete {}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default)]
pub struct RpList {}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct RpRead {
    pub metadata: Option<Metadata>,
}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct RpStat {
    pub metadata: Metadata,
}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default)]
pub struct RpWrite {}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default)]
pub struct RpCopy {}

#[xabi::data]
#[derive(Debug, Clone, Copy, Default)]
pub struct RpRename {}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct PresignedRequest {
    pub method: String,
    pub uri: String,
    pub headers: Vec<u8>,
}

#[xabi::data]
#[derive(Debug, Clone, Default)]
pub struct RpPresign {
    pub request: PresignedRequest,
}

pub mod oio {
    use super::*;

    pub const READ_TRAIT_ID: &str = "opendal.oio.Read";
    pub const WRITE_TRAIT_ID: &str = "opendal.oio.Write";
    pub const LIST_TRAIT_ID: &str = "opendal.oio.List";
    pub const DELETE_TRAIT_ID: &str = "opendal.oio.Delete";
    pub const COPY_TRAIT_ID: &str = "opendal.oio.Copy";

    pub trait Read: fmt::Debug + Unpin + Send + Sync {
        async fn read(&mut self) -> Result<Vec<u8>>;

        async fn read_all(&mut self) -> Result<Vec<u8>> {
            let mut out = Vec::new();
            loop {
                let chunk = self.read().await?;
                if chunk.is_empty() {
                    return Ok(out);
                }
                out.extend_from_slice(&chunk);
            }
        }
    }

    pub trait Write: fmt::Debug + Unpin + Send + Sync {
        async fn write(&mut self, data: &[u8]) -> Result<()>;
        async fn close(&mut self) -> Result<Metadata>;
        async fn abort(&mut self) -> Result<()>;
    }

    pub trait List: fmt::Debug + Unpin + Send + Sync {
        async fn next(&mut self) -> Result<Option<Entry>>;
    }

    pub trait Delete: fmt::Debug + Unpin + Send + Sync {
        async fn delete(&mut self, path: &str, args: OpDelete) -> Result<()>;
        async fn close(&mut self) -> Result<()>;
    }

    pub trait Copy: fmt::Debug + Unpin + Send + Sync {
        async fn next(&mut self) -> Result<Option<usize>>;
        async fn close(&mut self) -> Result<Metadata>;
        async fn abort(&mut self) -> Result<()>;
    }

    #[xabi::xabi(id = READ_TRAIT_ID, version = ABI_VERSION)]
    pub trait ReadAbi {
        async fn read(&mut self) -> std::result::Result<Vec<u8>, Error>;

        async fn read_all(&mut self) -> std::result::Result<Vec<u8>, Error> {
            let mut out = Vec::new();
            loop {
                let chunk = self.read().await?;
                if chunk.is_empty() {
                    return Ok(out);
                }
                out.extend_from_slice(&chunk);
            }
        }
    }

    #[xabi::xabi(id = WRITE_TRAIT_ID, version = ABI_VERSION)]
    pub trait WriteAbi {
        async fn write(&mut self, data: &[u8]) -> std::result::Result<(), Error>;
        async fn close(&mut self) -> std::result::Result<Metadata, Error>;
        async fn abort(&mut self) -> std::result::Result<(), Error>;
    }

    #[xabi::xabi(id = LIST_TRAIT_ID, version = ABI_VERSION)]
    pub trait ListAbi {
        async fn next(&mut self) -> std::result::Result<Option<Entry>, Error>;
    }

    #[xabi::xabi(id = DELETE_TRAIT_ID, version = ABI_VERSION)]
    pub trait DeleteAbi {
        async fn delete(&mut self, path: &str, args: OpDelete) -> std::result::Result<(), Error>;

        async fn close(&mut self) -> std::result::Result<(), Error>;
    }

    #[xabi::xabi(id = COPY_TRAIT_ID, version = ABI_VERSION)]
    pub trait CopyAbi {
        async fn next(&mut self) -> std::result::Result<Option<usize>, Error>;
        async fn close(&mut self) -> std::result::Result<Metadata, Error>;
        async fn abort(&mut self) -> std::result::Result<(), Error>;
    }

    pub type ReadHandle = XabiV1HandleTraitReadAbi;
    pub type WriteHandle = XabiV1HandleTraitWriteAbi;
    pub type ListHandle = XabiV1HandleTraitListAbi;
    pub type DeleteHandle = XabiV1HandleTraitDeleteAbi;
    pub type CopyHandle = XabiV1HandleTraitCopyAbi;

    pub type OwnedReadRef = XabiV1OwnedRefTraitReadAbi;
    pub type OwnedWriteRef = XabiV1OwnedRefTraitWriteAbi;
    pub type OwnedListRef = XabiV1OwnedRefTraitListAbi;
    pub type OwnedDeleteRef = XabiV1OwnedRefTraitDeleteAbi;
    pub type OwnedCopyRef = XabiV1OwnedRefTraitCopyAbi;

    impl Read for ReadHandle {
        async fn read(&mut self) -> Result<Vec<u8>> {
            ReadHandle::read(self).await.map_err(Error::from)
        }

        async fn read_all(&mut self) -> Result<Vec<u8>> {
            ReadHandle::read_all(self).await.map_err(Error::from)
        }
    }

    impl Write for WriteHandle {
        async fn write(&mut self, data: &[u8]) -> Result<()> {
            WriteHandle::write(self, data).await.map_err(Error::from)
        }

        async fn close(&mut self) -> Result<Metadata> {
            WriteHandle::close(self).await.map_err(Error::from)
        }

        async fn abort(&mut self) -> Result<()> {
            WriteHandle::abort(self).await.map_err(Error::from)
        }
    }

    impl List for ListHandle {
        async fn next(&mut self) -> Result<Option<Entry>> {
            ListHandle::next(self).await.map_err(Error::from)
        }
    }

    impl Delete for DeleteHandle {
        async fn delete(&mut self, path: &str, args: OpDelete) -> Result<()> {
            DeleteHandle::delete(self, path, args)
                .await
                .map_err(Error::from)
        }

        async fn close(&mut self) -> Result<()> {
            DeleteHandle::close(self).await.map_err(Error::from)
        }
    }

    impl Copy for CopyHandle {
        async fn next(&mut self) -> Result<Option<usize>> {
            CopyHandle::next(self).await.map_err(Error::from)
        }

        async fn close(&mut self) -> Result<Metadata> {
            CopyHandle::close(self).await.map_err(Error::from)
        }

        async fn abort(&mut self) -> Result<()> {
            CopyHandle::abort(self).await.map_err(Error::from)
        }
    }
}

#[xabi::data]
#[derive(Debug, Clone)]
pub struct ReadResult {
    pub rp: RpRead,
    pub reader: oio::OwnedReadRef,
}

impl ReadResult {
    pub fn from_reader<R: oio::ReadAbi>(rp: RpRead, reader: R) -> Self {
        Self::new(rp, oio::OwnedReadRef::xabi_from_value(reader))
    }
}

#[xabi::data]
#[derive(Debug, Clone)]
pub struct WriteResult {
    pub rp: RpWrite,
    pub writer: oio::OwnedWriteRef,
}

impl WriteResult {
    pub fn from_writer<W: oio::WriteAbi>(rp: RpWrite, writer: W) -> Self {
        Self::new(rp, oio::OwnedWriteRef::xabi_from_value(writer))
    }
}

#[xabi::data]
#[derive(Debug, Clone)]
pub struct DeleteResult {
    pub rp: RpDelete,
    pub deleter: oio::OwnedDeleteRef,
}

impl DeleteResult {
    pub fn from_deleter<D: oio::DeleteAbi>(rp: RpDelete, deleter: D) -> Self {
        Self::new(rp, oio::OwnedDeleteRef::xabi_from_value(deleter))
    }
}

#[xabi::data]
#[derive(Debug, Clone)]
pub struct ListResult {
    pub rp: RpList,
    pub lister: oio::OwnedListRef,
}

impl ListResult {
    pub fn from_lister<L: oio::ListAbi>(rp: RpList, lister: L) -> Self {
        Self::new(rp, oio::OwnedListRef::xabi_from_value(lister))
    }
}

#[xabi::data]
#[derive(Debug, Clone)]
pub struct CopyResult {
    pub rp: RpCopy,
    pub copier: oio::OwnedCopyRef,
}

impl CopyResult {
    pub fn from_copier<C: oio::CopyAbi>(rp: RpCopy, copier: C) -> Self {
        Self::new(rp, oio::OwnedCopyRef::xabi_from_value(copier))
    }
}

pub trait Access: fmt::Debug + Unpin + Send + Sync + 'static {
    type Reader: oio::Read;
    type Writer: oio::Write;
    type Lister: oio::List;
    type Deleter: oio::Delete;
    type Copier: oio::Copy;

    fn info(&self) -> Arc<AccessorInfo>;

    async fn create_dir(&self, path: &str, args: OpCreateDir) -> Result<RpCreateDir> {
        let (_, _) = (path, args);
        Err(Error::unsupported("create_dir"))
    }

    async fn stat(&self, path: &str, args: OpStat) -> Result<RpStat> {
        let (_, _) = (path, args);
        Err(Error::unsupported("stat"))
    }

    async fn read(&self, path: &str, args: OpRead) -> Result<(RpRead, Self::Reader)> {
        let (_, _) = (path, args);
        Err(Error::unsupported("read"))
    }

    async fn write(&self, path: &str, args: OpWrite) -> Result<(RpWrite, Self::Writer)> {
        let (_, _) = (path, args);
        Err(Error::unsupported("write"))
    }

    async fn delete(&self) -> Result<(RpDelete, Self::Deleter)> {
        Err(Error::unsupported("delete"))
    }

    async fn list(&self, path: &str, args: OpList) -> Result<(RpList, Self::Lister)> {
        let (_, _) = (path, args);
        Err(Error::unsupported("list"))
    }

    async fn copy(
        &self,
        from: &str,
        to: &str,
        args: OpCopy,
        opts: OpCopier,
    ) -> Result<(RpCopy, Self::Copier)> {
        let (_, _, _, _) = (from, to, args, opts);
        Err(Error::unsupported("copy"))
    }

    async fn rename(&self, from: &str, to: &str, args: OpRename) -> Result<RpRename> {
        let (_, _, _) = (from, to, args);
        Err(Error::unsupported("rename"))
    }

    async fn presign(&self, path: &str, args: OpPresign) -> Result<RpPresign> {
        let (_, _) = (path, args);
        Err(Error::unsupported("presign"))
    }
}

#[xabi::xabi(id = ACCESS_TRAIT_ID, version = ABI_VERSION)]
pub trait AccessAbi {
    fn info(&self) -> AccessorInfo;

    async fn create_dir(
        &self,
        path: &str,
        args: OpCreateDir,
    ) -> std::result::Result<RpCreateDir, Error> {
        let (_, _) = (path, args);
        Err(Error::unsupported("create_dir"))
    }

    async fn stat(&self, path: &str, args: OpStat) -> std::result::Result<RpStat, Error> {
        let (_, _) = (path, args);
        Err(Error::unsupported("stat"))
    }

    async fn read(&self, path: &str, args: OpRead) -> std::result::Result<ReadResult, Error> {
        let (_, _) = (path, args);
        Err(Error::unsupported("read"))
    }

    async fn write(&self, path: &str, args: OpWrite) -> std::result::Result<WriteResult, Error> {
        let (_, _) = (path, args);
        Err(Error::unsupported("write"))
    }

    async fn delete(&self) -> std::result::Result<DeleteResult, Error> {
        Err(Error::unsupported("delete"))
    }

    async fn list(&self, path: &str, args: OpList) -> std::result::Result<ListResult, Error> {
        let (_, _) = (path, args);
        Err(Error::unsupported("list"))
    }

    async fn copy(
        &self,
        from: &str,
        to: &str,
        args: OpCopy,
        opts: OpCopier,
    ) -> std::result::Result<CopyResult, Error> {
        let (_, _, _, _) = (from, to, args, opts);
        Err(Error::unsupported("copy"))
    }

    async fn rename(
        &self,
        from: &str,
        to: &str,
        args: OpRename,
    ) -> std::result::Result<RpRename, Error> {
        let (_, _, _) = (from, to, args);
        Err(Error::unsupported("rename"))
    }

    async fn presign(&self, path: &str, args: OpPresign) -> std::result::Result<RpPresign, Error> {
        let (_, _) = (path, args);
        Err(Error::unsupported("presign"))
    }
}

pub type AccessHandle = XabiV1HandleTraitAccessAbi;
pub type AccessVTable = XabiV1VtableTraitAccessAbi;

impl Access for AccessHandle {
    type Reader = oio::ReadHandle;
    type Writer = oio::WriteHandle;
    type Lister = oio::ListHandle;
    type Deleter = oio::DeleteHandle;
    type Copier = oio::CopyHandle;

    fn info(&self) -> Arc<AccessorInfo> {
        Arc::new(AccessHandle::info(self).unwrap_or_else(|err| {
            AccessorInfo::new(
                "invalid".to_string(),
                "/".to_string(),
                err.to_string(),
                Capability::default(),
            )
        }))
    }

    async fn create_dir(&self, path: &str, args: OpCreateDir) -> Result<RpCreateDir> {
        AccessHandle::create_dir(self, path, args)
            .await
            .map_err(Error::from)
    }

    async fn stat(&self, path: &str, args: OpStat) -> Result<RpStat> {
        AccessHandle::stat(self, path, args)
            .await
            .map_err(Error::from)
    }

    async fn read(&self, path: &str, args: OpRead) -> Result<(RpRead, Self::Reader)> {
        let result = AccessHandle::read(self, path, args)
            .await
            .map_err(Error::from)?;
        let reader = unsafe {
            oio::ReadHandle::xabi_from_owned_ref(result.reader, self.xabi_module())
                .map_err(Error::from)?
        };
        Ok((result.rp, reader))
    }

    async fn write(&self, path: &str, args: OpWrite) -> Result<(RpWrite, Self::Writer)> {
        let result = AccessHandle::write(self, path, args)
            .await
            .map_err(Error::from)?;
        let writer = unsafe {
            oio::WriteHandle::xabi_from_owned_ref(result.writer, self.xabi_module())
                .map_err(Error::from)?
        };
        Ok((result.rp, writer))
    }

    async fn delete(&self) -> Result<(RpDelete, Self::Deleter)> {
        let result = AccessHandle::delete(self).await.map_err(Error::from)?;
        let deleter = unsafe {
            oio::DeleteHandle::xabi_from_owned_ref(result.deleter, self.xabi_module())
                .map_err(Error::from)?
        };
        Ok((result.rp, deleter))
    }

    async fn list(&self, path: &str, args: OpList) -> Result<(RpList, Self::Lister)> {
        let result = AccessHandle::list(self, path, args)
            .await
            .map_err(Error::from)?;
        let lister = unsafe {
            oio::ListHandle::xabi_from_owned_ref(result.lister, self.xabi_module())
                .map_err(Error::from)?
        };
        Ok((result.rp, lister))
    }

    async fn copy(
        &self,
        from: &str,
        to: &str,
        args: OpCopy,
        opts: OpCopier,
    ) -> Result<(RpCopy, Self::Copier)> {
        let result = AccessHandle::copy(self, from, to, args, opts)
            .await
            .map_err(Error::from)?;
        let copier = unsafe {
            oio::CopyHandle::xabi_from_owned_ref(result.copier, self.xabi_module())
                .map_err(Error::from)?
        };
        Ok((result.rp, copier))
    }

    async fn rename(&self, from: &str, to: &str, args: OpRename) -> Result<RpRename> {
        AccessHandle::rename(self, from, to, args)
            .await
            .map_err(Error::from)
    }

    async fn presign(&self, path: &str, args: OpPresign) -> Result<RpPresign> {
        AccessHandle::presign(self, path, args)
            .await
            .map_err(Error::from)
    }
}
