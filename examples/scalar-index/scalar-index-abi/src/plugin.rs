use std::sync::Arc;

use async_trait::async_trait;
use xabi::{XabiOwnedBytes, XabiType};

use crate::host::{
    BorrowedIndexBuildProgress, BorrowedIndexStore, HostIndexBuildProgress, HostIndexStore,
    IndexBuildProgress, IndexStore, OwnedIndexBuildProgress, OwnedIndexStore,
};
use crate::{drain_arrow_stream, ArrowStreamHandle};
use crate::{Error, Result};

pub const TRAIT_ID: &str = "lance.ScalarIndexPlugin";
pub const INDEX_TRAIT_ID: &str = "lance.ScalarIndex";
pub const ABI_VERSION: u32 = 1;

#[xabi::data]
#[derive(Clone, Copy)]
pub struct OpTrain {
    pub requested_partitions: u32,
}

#[derive(Clone, Copy)]
pub struct TrainInput {
    pub data: ArrowStreamHandle,
    pub store: BorrowedIndexStore,
    pub progress: BorrowedIndexBuildProgress,
    pub op: OpTrain,
}

impl TrainInput {
    pub fn new(
        data: ArrowStreamHandle,
        store: BorrowedIndexStore,
        progress: BorrowedIndexBuildProgress,
        op: OpTrain,
    ) -> Self {
        Self {
            data,
            store,
            progress,
            op,
        }
    }
}

unsafe impl Send for TrainInput {}
unsafe impl Sync for TrainInput {}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiV1DataTrainInput {
    pub size: usize,
    pub abi_version: u32,
    pub data: <ArrowStreamHandle as XabiType>::Wire,
    pub store: <BorrowedIndexStore as XabiType>::Wire,
    pub progress: <BorrowedIndexBuildProgress as XabiType>::Wire,
    pub op: <OpTrain as XabiType>::Wire,
}

impl XabiV1DataTrainInput {
    pub const ABI_VERSION: u32 = xabi::ABI_VERSION;
    pub const MIN_SIZE: usize = std::mem::size_of::<Self>();

    pub fn validate(&self) -> xabi::Result<()> {
        xabi::validate_size(self.size, Self::MIN_SIZE, "XabiV1DataTrainInput")?;
        xabi::validate_abi_version(self.abi_version, Self::ABI_VERSION, "XabiV1DataTrainInput")
    }
}

unsafe impl Send for XabiV1DataTrainInput {}
unsafe impl Sync for XabiV1DataTrainInput {}

impl XabiType for TrainInput {
    type Wire = XabiV1DataTrainInput;

    fn into_wire(self) -> Self::Wire {
        XabiV1DataTrainInput {
            size: std::mem::size_of::<XabiV1DataTrainInput>(),
            abi_version: XabiV1DataTrainInput::ABI_VERSION,
            data: self.data.into_wire(),
            store: self.store.into_wire(),
            progress: self.progress.into_wire(),
            op: self.op.into_wire(),
        }
    }

    unsafe fn from_wire(wire: *const Self::Wire) -> xabi::Result<Self> {
        let wire = unsafe {
            wire.as_ref()
                .ok_or(xabi::Error::NullPointer("XabiV1DataTrainInput pointer"))?
        };
        wire.validate()?;
        Ok(Self {
            data: unsafe { ArrowStreamHandle::from_wire(&wire.data) }?,
            store: unsafe { BorrowedIndexStore::from_wire(&wire.store) }?,
            progress: unsafe { BorrowedIndexBuildProgress::from_wire(&wire.progress) }?,
            op: unsafe { OpTrain::from_wire(&wire.op) }?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct TrainOutput {
    pub rows_seen: i64,
    pub progress_events: u32,
    pub details: Vec<u8>,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiV1DataTrainOutput {
    pub size: usize,
    pub abi_version: u32,
    pub rows_seen: i64,
    pub progress_events: u32,
    pub details: XabiOwnedBytes,
}

impl XabiV1DataTrainOutput {
    pub const ABI_VERSION: u32 = xabi::ABI_VERSION;
    pub const MIN_SIZE: usize = std::mem::size_of::<Self>();

    pub fn validate(&self) -> xabi::Result<()> {
        xabi::validate_size(self.size, Self::MIN_SIZE, "XabiV1DataTrainOutput")?;
        xabi::validate_abi_version(self.abi_version, Self::ABI_VERSION, "XabiV1DataTrainOutput")
    }
}

impl XabiType for TrainOutput {
    type Wire = XabiV1DataTrainOutput;

    fn into_wire(self) -> Self::Wire {
        XabiV1DataTrainOutput {
            size: std::mem::size_of::<XabiV1DataTrainOutput>(),
            abi_version: XabiV1DataTrainOutput::ABI_VERSION,
            rows_seen: self.rows_seen,
            progress_events: self.progress_events,
            details: XabiOwnedBytes::from_vec(self.details),
        }
    }

    unsafe fn from_wire(wire: *const Self::Wire) -> xabi::Result<Self> {
        let wire = unsafe {
            wire.as_ref()
                .ok_or(xabi::Error::NullPointer("XabiV1DataTrainOutput pointer"))?
        };
        wire.validate()?;
        let details = unsafe { wire.details.to_vec_and_free() }?;
        Ok(Self {
            rows_seen: wire.rows_seen,
            progress_events: wire.progress_events,
            details,
        })
    }
}

pub struct LoadedScalarIndex {
    raw: *mut XabiV1VtableTraitScalarIndexAbi,
}

unsafe impl Send for LoadedScalarIndex {}
unsafe impl Sync for LoadedScalarIndex {}

impl LoadedScalarIndex {
    pub fn new(index: impl ScalarIndexAbi) -> Self {
        Self {
            raw: XabiV1AbiTraitScalarIndexAbi::xabi_export(index),
        }
    }

    /// # Safety
    ///
    /// `module` must keep the dynamic library that owns this index vtable loaded.
    pub unsafe fn into_handle(
        self,
        module: Arc<xabi::ModuleHandle>,
    ) -> xabi::Result<XabiScalarIndexHandle> {
        unsafe { XabiScalarIndexHandle::xabi_from_vtable(self.raw, module) }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiV1DataLoadedScalarIndex {
    pub size: usize,
    pub abi_version: u32,
    pub raw: *mut XabiV1VtableTraitScalarIndexAbi,
}

impl XabiV1DataLoadedScalarIndex {
    pub const ABI_VERSION: u32 = xabi::ABI_VERSION;
    pub const MIN_SIZE: usize = std::mem::size_of::<Self>();

    pub fn validate(&self) -> xabi::Result<()> {
        xabi::validate_size(self.size, Self::MIN_SIZE, "XabiV1DataLoadedScalarIndex")?;
        xabi::validate_abi_version(
            self.abi_version,
            Self::ABI_VERSION,
            "XabiV1DataLoadedScalarIndex",
        )?;
        if self.raw.is_null() {
            return Err(xabi::Error::NullPointer("XabiV1DataLoadedScalarIndex::raw"));
        }
        Ok(())
    }
}

unsafe impl Send for XabiV1DataLoadedScalarIndex {}
unsafe impl Sync for XabiV1DataLoadedScalarIndex {}

impl XabiType for LoadedScalarIndex {
    type Wire = XabiV1DataLoadedScalarIndex;

    fn into_wire(self) -> Self::Wire {
        XabiV1DataLoadedScalarIndex {
            size: std::mem::size_of::<XabiV1DataLoadedScalarIndex>(),
            abi_version: XabiV1DataLoadedScalarIndex::ABI_VERSION,
            raw: self.raw,
        }
    }

    unsafe fn from_wire(wire: *const Self::Wire) -> xabi::Result<Self> {
        let wire = unsafe {
            wire.as_ref().ok_or(xabi::Error::NullPointer(
                "XabiV1DataLoadedScalarIndex pointer",
            ))?
        };
        wire.validate()?;
        Ok(Self { raw: wire.raw })
    }
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
    ) -> std::result::Result<LoadedScalarIndex, Error>;

    async fn load_statistics(&self, details: &[u8]) -> std::result::Result<Option<String>, Error> {
        let _ = details;
        Ok(None)
    }
}

pub use XabiV1HandleTraitScalarIndexAbi as XabiScalarIndexHandle;
pub use XabiV1HandleTraitScalarIndexPluginAbi as XabiScalarIndexPluginHandle;
pub use XabiV1VtableTraitScalarIndexAbi as ScalarIndexVTable;
pub use XabiV1VtableTraitScalarIndexPluginAbi as ScalarIndexPluginVTable;

#[async_trait]
impl ScalarIndexPlugin for XabiScalarIndexPluginHandle {
    fn name(&self) -> String {
        XabiScalarIndexPluginHandle::name(self)
            .unwrap_or_else(|err| format!("<invalid plugin name: {err}>"))
    }

    fn version(&self) -> u32 {
        XabiScalarIndexPluginHandle::version(self)
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
        let loaded = XabiScalarIndexPluginHandle::load_index(self, &details, store.xabi_borrow())
            .await
            .map_err(Error::from)?;
        let index = unsafe { loaded.into_handle(self.xabi_module())? };
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
    use super::*;

    #[test]
    fn generated_plugin_vtable_accepts_current_layout() {
        assert!(
            std::mem::size_of::<ScalarIndexPluginVTable>() >= ScalarIndexPluginVTable::MIN_SIZE
        );
    }

    #[test]
    fn generated_index_vtable_accepts_current_layout() {
        assert!(std::mem::size_of::<ScalarIndexVTable>() >= ScalarIndexVTable::MIN_SIZE);
    }
}
