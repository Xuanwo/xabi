use std::sync::Arc;

use async_trait::async_trait;

use crate::{Error, Result};

pub const STORE_TRAIT_ID: &str = "lance.IndexStore";
pub const PROGRESS_TRAIT_ID: &str = "lance.IndexBuildProgress";
pub const ABI_VERSION: u32 = 1;

#[async_trait]
pub trait IndexStore: Send + Sync {
    async fn put(&self, path: &str, data: &[u8]) -> Result<()>;
}

#[async_trait]
pub trait IndexBuildProgress: Send + Sync {
    async fn update(&self, rows: i64) -> Result<()>;
}

#[xabi::xabi(id = STORE_TRAIT_ID, version = ABI_VERSION)]
pub trait IndexStoreAbi {
    async fn put(&self, path: &str, data: &[u8]) -> std::result::Result<(), Error>;
}

#[xabi::xabi(id = PROGRESS_TRAIT_ID, version = ABI_VERSION)]
pub trait IndexBuildProgressAbi {
    async fn update(&self, rows: i64) -> std::result::Result<(), Error>;
}

pub use XabiV1BorrowedTraitIndexBuildProgressAbi as BorrowedIndexBuildProgress;
pub use XabiV1BorrowedTraitIndexStoreAbi as BorrowedIndexStore;
pub use XabiV1OwnedTraitIndexBuildProgressAbi as OwnedIndexBuildProgress;
pub use XabiV1OwnedTraitIndexStoreAbi as OwnedIndexStore;
pub use XabiV1VtableTraitIndexBuildProgressAbi as IndexBuildProgressVTable;
pub use XabiV1VtableTraitIndexStoreAbi as IndexStoreVTable;

pub struct HostIndexStore {
    inner: Arc<dyn IndexStore>,
}

impl HostIndexStore {
    pub fn new(inner: Arc<dyn IndexStore>) -> Self {
        Self { inner }
    }
}

impl IndexStoreAbi for HostIndexStore {
    async fn put(&self, path: &str, data: &[u8]) -> std::result::Result<(), Error> {
        self.inner.put(path, data).await
    }
}

pub struct HostIndexBuildProgress {
    inner: Arc<dyn IndexBuildProgress>,
}

impl HostIndexBuildProgress {
    pub fn new(inner: Arc<dyn IndexBuildProgress>) -> Self {
        Self { inner }
    }
}

impl IndexBuildProgressAbi for HostIndexBuildProgress {
    async fn update(&self, rows: i64) -> std::result::Result<(), Error> {
        self.inner.update(rows).await
    }
}

#[async_trait]
impl IndexStore for BorrowedIndexStore {
    async fn put(&self, path: &str, data: &[u8]) -> Result<()> {
        BorrowedIndexStore::put(self, path, data)
            .await
            .map_err(Error::from)
    }
}

#[async_trait]
impl IndexBuildProgress for BorrowedIndexBuildProgress {
    async fn update(&self, rows: i64) -> Result<()> {
        BorrowedIndexBuildProgress::update(self, rows)
            .await
            .map_err(Error::from)
    }
}

/// # Safety
///
/// `vtable` must either be null or point to a readable `IndexStoreVTable` for the duration of
/// this call.
pub unsafe fn validate_store_vtable(vtable: *const IndexStoreVTable) -> Result<()> {
    let vtable = unsafe {
        vtable
            .as_ref()
            .ok_or_else(|| Error::new("IndexStoreVTable pointer is null"))?
    };
    vtable.validate().map_err(Error::from)
}

/// # Safety
///
/// `vtable` must either be null or point to a readable `IndexBuildProgressVTable` for the duration
/// of this call.
pub unsafe fn validate_progress_vtable(vtable: *const IndexBuildProgressVTable) -> Result<()> {
    let vtable = unsafe {
        vtable
            .as_ref()
            .ok_or_else(|| Error::new("IndexBuildProgressVTable pointer is null"))?
    };
    vtable.validate().map_err(Error::from)
}
