#![allow(async_fn_in_trait)]

use std::fmt;

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

    #[xabi::xabi(id = READ_TRAIT_ID, version = ABI_VERSION)]
    pub trait Read: fmt::Debug + Unpin {
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
    pub trait Write: fmt::Debug + Unpin {
        async fn write(&mut self, data: &[u8]) -> std::result::Result<(), Error>;
        async fn close(&mut self) -> std::result::Result<Metadata, Error>;
        async fn abort(&mut self) -> std::result::Result<(), Error>;
    }

    #[xabi::xabi(id = LIST_TRAIT_ID, version = ABI_VERSION)]
    pub trait List: fmt::Debug + Unpin {
        async fn next(&mut self) -> std::result::Result<Option<Entry>, Error>;
    }

    #[xabi::xabi(id = DELETE_TRAIT_ID, version = ABI_VERSION)]
    pub trait Delete: fmt::Debug + Unpin {
        async fn delete(&mut self, path: &str, args: OpDelete) -> std::result::Result<(), Error>;

        async fn close(&mut self) -> std::result::Result<(), Error>;
    }

    #[xabi::xabi(id = COPY_TRAIT_ID, version = ABI_VERSION)]
    pub trait Copy: fmt::Debug + Unpin {
        async fn next(&mut self) -> std::result::Result<Option<usize>, Error>;
        async fn close(&mut self) -> std::result::Result<Metadata, Error>;
        async fn abort(&mut self) -> std::result::Result<(), Error>;
    }

    pub type ReadHandle = XabiV1HandleTraitRead;
    pub type WriteHandle = XabiV1HandleTraitWrite;
    pub type ListHandle = XabiV1HandleTraitList;
    pub type DeleteHandle = XabiV1HandleTraitDelete;
    pub type CopyHandle = XabiV1HandleTraitCopy;
}

#[xabi::xabi(id = ACCESS_TRAIT_ID, version = ABI_VERSION)]
pub trait Access: fmt::Debug + Unpin + Send + Sync + 'static {
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

    async fn read(
        &self,
        path: &str,
        args: OpRead,
    ) -> std::result::Result<(RpRead, impl oio::Read + 'static), Error>;

    async fn write(
        &self,
        path: &str,
        args: OpWrite,
    ) -> std::result::Result<(RpWrite, impl oio::Write + 'static), Error>;

    async fn delete(&self) -> std::result::Result<(RpDelete, impl oio::Delete + 'static), Error>;

    async fn list(
        &self,
        path: &str,
        args: OpList,
    ) -> std::result::Result<(RpList, impl oio::List + 'static), Error>;

    async fn copy(
        &self,
        from: &str,
        to: &str,
        args: OpCopy,
        opts: OpCopier,
    ) -> std::result::Result<(RpCopy, impl oio::Copy + 'static), Error>;

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

pub type AccessHandle = XabiV1HandleTraitAccess;
pub type AccessVTable = XabiV1VtableTraitAccess;
