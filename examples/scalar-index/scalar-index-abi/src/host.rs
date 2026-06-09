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

#[cfg(test)]
mod tests {
    #[test]
    fn index_store_abi_is_stable() {
        xabi_assert::assert_abi!(super::XabiV1AbiTraitIndexStoreAbi);
    }

    #[test]
    fn index_build_progress_abi_is_stable() {
        xabi_assert::assert_abi!(super::XabiV1AbiTraitIndexBuildProgressAbi);
    }
}
