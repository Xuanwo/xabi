use std::ffi::c_void;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::task;
use xabi::XabiStr;
use xabi_bytes::{XabiBytes, XabiOwnedBytes};

use crate::host::{
    HostVTables, IndexBuildProgress, IndexBuildProgressVTable, IndexStore, IndexStoreVTable,
};
use crate::{code_to_result, Error, Result};
use crate::{drain_arrow_stream, ArrowStreamHandle};

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
    pub details: XabiOwnedBytes,
}

impl RpTrain {
    pub fn empty() -> Self {
        Self {
            size: std::mem::size_of::<RpTrain>(),
            rows_seen: 0,
            progress_events: 0,
            details: XabiOwnedBytes::empty(),
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

xabi::raw::vtable! {
    pub struct ScalarIndexPluginVTable {
        abi_version = ABI_VERSION;
        @min_size(
            std::mem::offset_of!(ScalarIndexPluginVTable, release)
                + std::mem::size_of::<unsafe extern "C" fn(*mut ScalarIndexPluginVTable)>()
        );
        name: unsafe extern "C" fn(*mut c_void) -> XabiOwnedBytes,
        version: unsafe extern "C" fn(*mut c_void) -> u32,
        train_index: unsafe extern "C" fn(
            *mut c_void,
            *mut crate::ArrowArrayStream,
            *const IndexStoreVTable,
            *const IndexBuildProgressVTable,
            *const OpTrain,
            *mut RpTrain,
        ) -> i32,
        load_index: unsafe extern "C" fn(
            *mut c_void,
            XabiBytes,
            *const IndexStoreVTable,
            *mut *mut ScalarIndexVTable,
        ) -> i32,
        destroy: unsafe extern "C" fn(*mut c_void),
        release: unsafe extern "C" fn(*mut ScalarIndexPluginVTable),
        load_statistics: unsafe extern "C" fn(*mut c_void, XabiBytes, *mut XabiOwnedBytes) -> i32,
    }
}

xabi::raw::vtable! {
    pub struct ScalarIndexVTable {
        abi_version = ABI_VERSION;
        search: unsafe extern "C" fn(*mut c_void, XabiStr) -> XabiOwnedBytes,
        destroy: unsafe extern "C" fn(*mut c_void),
        release: unsafe extern "C" fn(*mut ScalarIndexVTable),
    }
}

xabi::raw::export_handle! {
    pub struct XabiScalarIndexPluginHandle for ScalarIndexPluginVTable {
        error = Error;
        abi_id = TRAIT_ID;
    }
}

#[async_trait]
impl ScalarIndexPlugin for XabiScalarIndexPluginHandle {
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
            let ffi_details = XabiBytes::from_slice(&details);
            let code = unsafe {
                (vtable.load_index)(vtable.instance, ffi_details, host.store(), &mut raw_index)
            };
            code_to_result(code, "ScalarIndexPlugin.load_index")?;
            let index = unsafe { XabiScalarIndexHandle::from_vtable(raw_index, library)? };
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
        if !xabi::raw::field_available!(vtable, ScalarIndexPluginVTable, load_statistics) {
            return Err(Error::new(
                "ScalarIndexPlugin.load_statistics capability is set but vtable field is missing",
            ));
        }

        let vtable = xabi::SendPtr::new(self.vtable.as_ptr());
        task::spawn_blocking(move || {
            let vtable = unsafe { vtable.as_ptr().as_ref() }
                .ok_or_else(|| Error::new("ScalarIndexPluginVTable pointer is null"))?;
            let mut out = XabiOwnedBytes::empty();
            let code = unsafe {
                (vtable.load_statistics)(vtable.instance, XabiBytes::from_slice(&details), &mut out)
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

xabi::raw::handle! {
    pub struct XabiScalarIndexHandle for ScalarIndexVTable {
        error = Error;
    }
}

#[async_trait]
impl ScalarIndex for XabiScalarIndexHandle {
    async fn search(&self, query: &str) -> Result<String> {
        let vtable = xabi::SendPtr::new(self.vtable.as_ptr());
        let query = query.to_string();
        task::spawn_blocking(move || {
            let vtable = unsafe { vtable.as_ptr().as_ref() }
                .ok_or_else(|| Error::new("ScalarIndexVTable pointer is null"))?;
            let owned = unsafe { (vtable.search)(vtable.instance, XabiStr::from_borrowed(&query)) };
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

/// # Safety
///
/// `stream` must point to a live `ArrowArrayStream` and must not be accessed concurrently.
pub unsafe fn drain_stream_for_plugin(stream: *mut crate::ArrowArrayStream) -> Result<i64> {
    let handle = unsafe { ArrowStreamHandle::from_raw(stream)? };
    Ok(drain_arrow_stream(handle)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_vtable_without_optional_tail() {
        let vtable = test_plugin_vtable(ScalarIndexPluginVTable::MIN_SIZE, 0, ABI_VERSION);

        vtable.validate().expect("required vtable prefix is valid");
        assert!(
            !xabi::raw::field_available!(&vtable, ScalarIndexPluginVTable, load_statistics),
            "optional tail field must be reported as unavailable"
        );
    }

    #[test]
    fn rejects_vtable_shorter_than_required_prefix() {
        let vtable = test_plugin_vtable(ScalarIndexPluginVTable::MIN_SIZE - 1, 0, ABI_VERSION);

        assert!(vtable.validate().is_err());
    }

    #[test]
    fn rejects_vtable_with_wrong_abi_version() {
        let vtable = test_plugin_vtable(ScalarIndexPluginVTable::MIN_SIZE, 0, ABI_VERSION + 1);

        assert!(vtable.validate().is_err());
    }

    #[test]
    fn detects_optional_capability_without_optional_field() {
        let vtable = test_plugin_vtable(
            ScalarIndexPluginVTable::MIN_SIZE,
            cap::LOAD_STATISTICS,
            ABI_VERSION,
        );

        vtable.validate().expect("required vtable prefix is valid");
        assert_eq!(
            vtable.capabilities & cap::LOAD_STATISTICS,
            cap::LOAD_STATISTICS
        );
        assert!(
            !xabi::raw::field_available!(&vtable, ScalarIndexPluginVTable, load_statistics),
            "capability alone must not imply that the optional function pointer is in bounds"
        );
    }

    #[test]
    fn detects_optional_field_when_full_vtable_is_available() {
        let vtable = test_plugin_vtable(
            std::mem::size_of::<ScalarIndexPluginVTable>(),
            cap::LOAD_STATISTICS,
            ABI_VERSION,
        );

        vtable.validate().expect("full vtable is valid");
        assert!(xabi::raw::field_available!(
            &vtable,
            ScalarIndexPluginVTable,
            load_statistics
        ));
    }

    fn test_plugin_vtable(
        size: usize,
        capabilities: u64,
        abi_version: u32,
    ) -> ScalarIndexPluginVTable {
        ScalarIndexPluginVTable {
            size,
            abi_version,
            capabilities,
            instance: std::ptr::null_mut(),
            name: test_name,
            version: test_version,
            train_index: test_train_index,
            load_index: test_load_index,
            destroy: test_destroy,
            release: test_release_plugin,
            load_statistics: test_load_statistics,
        }
    }

    unsafe extern "C" fn test_name(_instance: *mut c_void) -> XabiOwnedBytes {
        XabiOwnedBytes::empty()
    }

    unsafe extern "C" fn test_version(_instance: *mut c_void) -> u32 {
        0
    }

    unsafe extern "C" fn test_train_index(
        _instance: *mut c_void,
        _stream: *mut crate::ArrowArrayStream,
        _store: *const IndexStoreVTable,
        _progress: *const IndexBuildProgressVTable,
        _op: *const OpTrain,
        _out: *mut RpTrain,
    ) -> i32 {
        xabi::ERR_EXPORT
    }

    unsafe extern "C" fn test_load_index(
        _instance: *mut c_void,
        _details: XabiBytes,
        _store: *const IndexStoreVTable,
        _out: *mut *mut ScalarIndexVTable,
    ) -> i32 {
        xabi::ERR_EXPORT
    }

    unsafe extern "C" fn test_destroy(_instance: *mut c_void) {}

    unsafe extern "C" fn test_release_plugin(_vtable: *mut ScalarIndexPluginVTable) {}

    unsafe extern "C" fn test_load_statistics(
        _instance: *mut c_void,
        _details: XabiBytes,
        _out: *mut XabiOwnedBytes,
    ) -> i32 {
        xabi::OK
    }
}
