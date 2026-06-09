use std::sync::Arc;

use async_trait::async_trait;

use crate::host::{
    BorrowedIndexBuildProgress, BorrowedIndexStore, HostIndexBuildProgress, HostIndexStore,
    IndexBuildProgress, IndexStore, OwnedIndexBuildProgress, OwnedIndexStore,
};
use crate::{ArrowStreamHandle, drain_arrow_stream};
use crate::{Error, Result};

pub const TRAIT_ID: &str = "lance.ScalarIndexPlugin";
pub const INDEX_TRAIT_ID: &str = "lance.ScalarIndex";
pub const ABI_VERSION: u32 = 1;

#[xabi::data]
#[derive(Clone, Copy)]
pub struct OpTrain {
    pub requested_partitions: u32,
}

#[xabi::data]
#[derive(Clone, Copy)]
pub struct TrainInput {
    pub data: ArrowStreamHandle,
    pub store: BorrowedIndexStore,
    pub progress: BorrowedIndexBuildProgress,
    pub op: OpTrain,
}

unsafe impl Send for TrainInput {}
unsafe impl Sync for TrainInput {}

#[xabi::data]
#[derive(Debug, Clone)]
pub struct TrainOutput {
    pub rows_seen: i64,
    pub progress_events: u32,
    pub details: Vec<u8>,
}

#[async_trait]
pub trait ScalarIndexPlugin: Send + Sync {
    fn name(&self) -> String;

    fn version(&self) -> u32;

    async fn train_index(
        &self,
        data: ArrowStreamHandle,
        store: Arc<dyn IndexStore>,
        progress: Arc<dyn IndexBuildProgress>,
        op: OpTrain,
    ) -> Result<TrainOutput>;

    async fn load_index(
        &self,
        details: Vec<u8>,
        store: Arc<dyn IndexStore>,
    ) -> Result<Box<dyn ScalarIndex>>;

    async fn load_statistics(&self, _details: Vec<u8>) -> Result<Option<String>> {
        Ok(None)
    }
}

#[async_trait]
pub trait ScalarIndex: Send + Sync {
    async fn search(&self, query: &str) -> Result<String>;
}

#[xabi::xabi(id = INDEX_TRAIT_ID, version = ABI_VERSION)]
pub trait ScalarIndexAbi {
    async fn search(&self, query: &str) -> std::result::Result<String, Error>;
}

#[xabi::xabi(id = TRAIT_ID, version = ABI_VERSION)]
pub trait ScalarIndexPluginAbi {
    fn name(&self) -> String;

    fn version(&self) -> u32;

    async fn train_index(&self, input: TrainInput) -> std::result::Result<TrainOutput, Error>;

    async fn load_index(
        &self,
        details: &[u8],
        store: BorrowedIndexStore,
    ) -> std::result::Result<impl ScalarIndexAbi + 'static, Error>;

    async fn load_statistics(&self, details: &[u8]) -> std::result::Result<Option<String>, Error> {
        let _ = details;
        Ok(None)
    }
}

pub use XabiV1HandleTraitScalarIndexAbi as XabiScalarIndexHandle;
pub use XabiV1HandleTraitScalarIndexPluginAbi as XabiScalarIndexPluginHandle;

#[async_trait]
impl ScalarIndexPlugin for XabiScalarIndexPluginHandle {
    fn name(&self) -> String {
        XabiScalarIndexPluginHandle::name(self)
            .unwrap_or_else(|err| format!("<invalid plugin name: {err}>"))
    }

    fn version(&self) -> u32 {
        XabiScalarIndexPluginHandle::version(self).unwrap_or(0)
    }

    async fn train_index(
        &self,
        data: ArrowStreamHandle,
        store: Arc<dyn IndexStore>,
        progress: Arc<dyn IndexBuildProgress>,
        op: OpTrain,
    ) -> Result<TrainOutput> {
        let store = OwnedIndexStore::new(HostIndexStore::new(store));
        let progress = OwnedIndexBuildProgress::new(HostIndexBuildProgress::new(progress));
        let input = TrainInput::new(data, store.xabi_borrow(), progress.xabi_borrow(), op);
        XabiScalarIndexPluginHandle::train_index(self, input)
            .await
            .map_err(Error::from)
    }

    async fn load_index(
        &self,
        details: Vec<u8>,
        store: Arc<dyn IndexStore>,
    ) -> Result<Box<dyn ScalarIndex>> {
        let store = OwnedIndexStore::new(HostIndexStore::new(store));
        let index = XabiScalarIndexPluginHandle::load_index(self, &details, store.xabi_borrow())
            .await
            .map_err(Error::from)?;
        Ok(Box::new(index))
    }

    async fn load_statistics(&self, details: Vec<u8>) -> Result<Option<String>> {
        XabiScalarIndexPluginHandle::load_statistics(self, &details)
            .await
            .map_err(Error::from)
    }
}

#[async_trait]
impl ScalarIndex for XabiScalarIndexHandle {
    async fn search(&self, query: &str) -> Result<String> {
        XabiScalarIndexHandle::search(self, query)
            .await
            .map_err(Error::from)
    }
}

/// # Safety
///
/// `stream` must point to a live `ArrowArrayStream` and must not be accessed concurrently.
pub unsafe fn drain_stream_for_plugin(stream: *mut crate::ArrowArrayStream) -> Result<i64> {
    let handle = unsafe { ArrowStreamHandle::from_raw(stream)? };
    drain_arrow_stream(handle)
}

#[cfg(test)]
mod tests {
    #[test]
    fn scalar_index_abi_is_stable() {
        xabi_assert::assert_abi!(super::XabiV1AbiTraitScalarIndexAbi);
    }

    #[test]
    fn scalar_index_plugin_abi_is_stable() {
        xabi_assert::assert_abi!(super::XabiV1AbiTraitScalarIndexPluginAbi);
    }
}
