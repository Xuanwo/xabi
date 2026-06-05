use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::task;
use xabi::{FfiBytes, FfiOwned, FfiStr, LibraryHandle, PluginEntry};

use crate::arrow::{drain_arrow_stream, ArrowStreamHandle};
use crate::host::{
    HostVTables, IndexBuildProgress, IndexBuildProgressVTable, IndexStore, IndexStoreVTable,
};
use crate::{code_to_result, Error, Result};

pub const TRAIT_ID: &str = "lance.ScalarIndexPlugin";
pub const ABI_VERSION: u32 = 1;

pub mod cap {
    pub const LOAD_STATISTICS: u64 = 1 << 0;
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct OpTrain {
    pub size: usize,
    pub requested_partitions: u32,
}

impl OpTrain {
    pub fn new(requested_partitions: u32) -> Self {
        Self {
            size: std::mem::size_of::<OpTrain>(),
            requested_partitions,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct RpTrain {
    pub size: usize,
    pub rows_seen: i64,
    pub progress_events: u32,
    pub details: FfiOwned,
}

impl RpTrain {
    pub fn empty() -> Self {
        Self {
            size: std::mem::size_of::<RpTrain>(),
            rows_seen: 0,
            progress_events: 0,
            details: FfiOwned::empty(),
        }
    }
}

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
        data: ArrowStreamHandle<'_>,
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

#[repr(C)]
pub struct ScalarIndexPluginVTable {
    pub size: usize,
    pub abi_version: u32,
    pub capabilities: u64,
    pub instance: *mut c_void,
    pub name: unsafe extern "C" fn(*mut c_void) -> FfiOwned,
    pub version: unsafe extern "C" fn(*mut c_void) -> u32,
    pub train_index: unsafe extern "C" fn(
        *mut c_void,
        *mut crate::ArrowArrayStream,
        *const IndexStoreVTable,
        *const IndexBuildProgressVTable,
        *const OpTrain,
        *mut RpTrain,
    ) -> i32,
    pub load_index: unsafe extern "C" fn(
        *mut c_void,
        FfiBytes,
        *const IndexStoreVTable,
        *mut *mut ScalarIndexVTable,
    ) -> i32,
    pub load_statistics: unsafe extern "C" fn(*mut c_void, FfiBytes, *mut FfiOwned) -> i32,
    pub destroy: unsafe extern "C" fn(*mut c_void),
    pub release: unsafe extern "C" fn(*mut ScalarIndexPluginVTable),
}

#[repr(C)]
pub struct ScalarIndexVTable {
    pub size: usize,
    pub abi_version: u32,
    pub capabilities: u64,
    pub instance: *mut c_void,
    pub search: unsafe extern "C" fn(*mut c_void, FfiStr) -> FfiOwned,
    pub destroy: unsafe extern "C" fn(*mut c_void),
    pub release: unsafe extern "C" fn(*mut ScalarIndexVTable),
}

pub struct ForeignScalarIndexPlugin {
    vtable: NonNull<ScalarIndexPluginVTable>,
    _library: Arc<LibraryHandle>,
}

unsafe impl Send for ForeignScalarIndexPlugin {}
unsafe impl Sync for ForeignScalarIndexPlugin {}

impl ForeignScalarIndexPlugin {
    /// # Safety
    ///
    /// `entry.make` must return a valid `ScalarIndexPluginVTable` that follows this crate's ABI
    /// and ownership contract. `library` must keep the code backing all function pointers loaded.
    pub unsafe fn from_entry(entry: &PluginEntry, library: Arc<LibraryHandle>) -> Result<Self> {
        let trait_id = entry.trait_id.as_str()?;
        if trait_id != TRAIT_ID {
            return Err(Error::new(format!(
                "plugin entry has trait_id {trait_id}, expected {TRAIT_ID}"
            )));
        }

        let raw = (entry.make)() as *mut ScalarIndexPluginVTable;
        let vtable = NonNull::new(raw)
            .ok_or_else(|| Error::new("ScalarIndexPluginVTable pointer is null"))?;
        validate_plugin_vtable(vtable.as_ref())?;

        Ok(Self {
            vtable,
            _library: library,
        })
    }

    fn vtable(&self) -> &ScalarIndexPluginVTable {
        unsafe { self.vtable.as_ref() }
    }
}

impl Drop for ForeignScalarIndexPlugin {
    fn drop(&mut self) {
        unsafe {
            (self.vtable().release)(self.vtable.as_ptr());
        }
    }
}

#[async_trait]
impl ScalarIndexPlugin for ForeignScalarIndexPlugin {
    fn name(&self) -> String {
        let owned = unsafe { (self.vtable().name)(self.vtable().instance) };
        unsafe {
            owned
                .to_string_and_free()
                .unwrap_or_else(|err| format!("<invalid plugin name: {err}>"))
        }
    }

    fn version(&self) -> u32 {
        unsafe { (self.vtable().version)(self.vtable().instance) }
    }

    async fn train_index(
        &self,
        data: ArrowStreamHandle<'_>,
        store: Arc<dyn IndexStore>,
        progress: Arc<dyn IndexBuildProgress>,
        op: OpTrain,
    ) -> Result<TrainOutput> {
        let vtable = xabi::SendPtr::new(self.vtable.as_ptr());
        let stream = xabi::SendPtr::new(data.as_raw());

        task::spawn_blocking(move || {
            let vtable = unsafe { vtable.as_ptr().as_ref() }
                .ok_or_else(|| Error::new("ScalarIndexPluginVTable pointer is null"))?;
            let host = HostVTables::new(store, progress);
            let mut rp = RpTrain::empty();
            let code = unsafe {
                (vtable.train_index)(
                    vtable.instance,
                    stream.as_ptr(),
                    host.store(),
                    host.progress(),
                    &op,
                    &mut rp,
                )
            };
            code_to_result(code, "ScalarIndexPlugin.train_index")?;
            let details = unsafe { rp.details.to_vec_and_free()? };
            Ok(TrainOutput {
                rows_seen: rp.rows_seen,
                progress_events: rp.progress_events,
                details,
            })
        })
        .await
        .map_err(|err| Error::new(format!("train_index blocking task failed: {err}")))?
    }

    async fn load_index(
        &self,
        details: Vec<u8>,
        store: Arc<dyn IndexStore>,
    ) -> Result<Box<dyn ScalarIndex>> {
        let vtable = xabi::SendPtr::new(self.vtable.as_ptr());
        let library = Arc::clone(&self._library);

        task::spawn_blocking(move || {
            let vtable = unsafe { vtable.as_ptr().as_ref() }
                .ok_or_else(|| Error::new("ScalarIndexPluginVTable pointer is null"))?;
            let noop_progress: Arc<dyn IndexBuildProgress> = Arc::new(NoopProgress);
            let host = HostVTables::new(store, noop_progress);
            let mut raw_index = std::ptr::null_mut();
            let ffi_details = FfiBytes::from_slice(&details);
            let code = unsafe {
                (vtable.load_index)(vtable.instance, ffi_details, host.store(), &mut raw_index)
            };
            code_to_result(code, "ScalarIndexPlugin.load_index")?;
            let index = unsafe { ForeignScalarIndex::from_vtable(raw_index, library)? };
            Ok(Box::new(index) as Box<dyn ScalarIndex>)
        })
        .await
        .map_err(|err| Error::new(format!("load_index blocking task failed: {err}")))?
    }

    async fn load_statistics(&self, details: Vec<u8>) -> Result<Option<String>> {
        let vtable = self.vtable();
        if vtable.capabilities & cap::LOAD_STATISTICS == 0 {
            return Ok(None);
        }

        let vtable = xabi::SendPtr::new(self.vtable.as_ptr());
        task::spawn_blocking(move || {
            let vtable = unsafe { vtable.as_ptr().as_ref() }
                .ok_or_else(|| Error::new("ScalarIndexPluginVTable pointer is null"))?;
            let mut out = FfiOwned::empty();
            let code = unsafe {
                (vtable.load_statistics)(vtable.instance, FfiBytes::from_slice(&details), &mut out)
            };
            code_to_result(code, "ScalarIndexPlugin.load_statistics")?;
            let value = unsafe { out.to_string_and_free()? };
            if value.is_empty() {
                Ok(None)
            } else {
                Ok(Some(value))
            }
        })
        .await
        .map_err(|err| Error::new(format!("load_statistics blocking task failed: {err}")))?
    }
}

pub struct ForeignScalarIndex {
    vtable: NonNull<ScalarIndexVTable>,
    _library: Arc<LibraryHandle>,
}

unsafe impl Send for ForeignScalarIndex {}
unsafe impl Sync for ForeignScalarIndex {}

impl ForeignScalarIndex {
    /// # Safety
    ///
    /// `vtable` must be a valid owned `ScalarIndexVTable` produced by the plugin, and `library`
    /// must keep the code backing all function pointers loaded.
    pub unsafe fn from_vtable(
        vtable: *mut ScalarIndexVTable,
        library: Arc<LibraryHandle>,
    ) -> Result<Self> {
        let vtable =
            NonNull::new(vtable).ok_or_else(|| Error::new("ScalarIndexVTable pointer is null"))?;
        validate_index_vtable(vtable.as_ref())?;
        Ok(Self {
            vtable,
            _library: library,
        })
    }

    fn vtable(&self) -> &ScalarIndexVTable {
        unsafe { self.vtable.as_ref() }
    }
}

impl Drop for ForeignScalarIndex {
    fn drop(&mut self) {
        unsafe {
            (self.vtable().release)(self.vtable.as_ptr());
        }
    }
}

#[async_trait]
impl ScalarIndex for ForeignScalarIndex {
    async fn search(&self, query: &str) -> Result<String> {
        let vtable = xabi::SendPtr::new(self.vtable.as_ptr());
        let query = query.to_string();
        task::spawn_blocking(move || {
            let vtable = unsafe { vtable.as_ptr().as_ref() }
                .ok_or_else(|| Error::new("ScalarIndexVTable pointer is null"))?;
            let owned = unsafe { (vtable.search)(vtable.instance, FfiStr::from_borrowed(&query)) };
            unsafe { owned.to_string_and_free().map_err(Error::from) }
        })
        .await
        .map_err(|err| Error::new(format!("search blocking task failed: {err}")))?
    }
}

struct NoopProgress;

#[async_trait]
impl IndexBuildProgress for NoopProgress {
    async fn update(&self, _rows: i64) -> Result<()> {
        Ok(())
    }
}

fn validate_plugin_vtable(vtable: &ScalarIndexPluginVTable) -> Result<()> {
    xabi::validate_size(
        vtable.size,
        std::mem::size_of::<ScalarIndexPluginVTable>(),
        "ScalarIndexPluginVTable",
    )?;
    xabi::validate_abi_version(vtable.abi_version, ABI_VERSION, "ScalarIndexPluginVTable")?;
    Ok(())
}

fn validate_index_vtable(vtable: &ScalarIndexVTable) -> Result<()> {
    xabi::validate_size(
        vtable.size,
        std::mem::size_of::<ScalarIndexVTable>(),
        "ScalarIndexVTable",
    )?;
    xabi::validate_abi_version(vtable.abi_version, ABI_VERSION, "ScalarIndexVTable")?;
    Ok(())
}

/// # Safety
///
/// `stream` must point to a live `ArrowArrayStream` and must not be accessed concurrently.
pub unsafe fn drain_stream_for_plugin(stream: *mut crate::ArrowArrayStream) -> Result<i64> {
    let handle = unsafe { ArrowStreamHandle::from_raw(stream)? };
    drain_arrow_stream(handle)
}
