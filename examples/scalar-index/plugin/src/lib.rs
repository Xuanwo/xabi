use std::ffi::c_void;
use std::ptr;
use std::sync::Arc;

use async_trait::async_trait;
use futures::executor::block_on;
use scalar_index_abi::{
    cap, Error, IndexBuildProgress, IndexBuildProgressVTable, IndexStore, IndexStoreVTable,
    OpTrain, Result, ScalarIndex, ScalarIndexPlugin, ScalarIndexPluginVTable, ScalarIndexVTable,
    TrainOutput, ABI_VERSION, TRAIT_ID,
};
use xabi::{FfiBytes, FfiOwned, FfiStr, PluginEntry, PluginManifest};

struct DemoPlugin;

#[async_trait]
impl ScalarIndexPlugin for DemoPlugin {
    fn name(&self) -> String {
        "demo-scalar-index".to_string()
    }

    fn version(&self) -> u32 {
        1
    }

    async fn train_index(
        &self,
        data: scalar_index_abi::ArrowStreamHandle<'_>,
        store: Arc<dyn IndexStore>,
        progress: Arc<dyn IndexBuildProgress>,
        op: OpTrain,
    ) -> Result<TrainOutput> {
        let rows_seen = scalar_index_abi::drain_arrow_stream(data)?;
        let progress_events = op.requested_partitions.max(1);
        for _ in 0..progress_events {
            progress.update(rows_seen).await?;
        }
        store
            .put("index.details", format!("rows={rows_seen}").as_bytes())
            .await?;

        Ok(TrainOutput {
            rows_seen,
            progress_events,
            details: format!("demo:index:rows={rows_seen}").into_bytes(),
        })
    }

    async fn load_index(
        &self,
        details: Vec<u8>,
        store: Arc<dyn IndexStore>,
    ) -> Result<Box<dyn ScalarIndex>> {
        store.put("index.loaded", &details).await?;
        let details = String::from_utf8(details)
            .map_err(|err| Error::new(format!("invalid details: {err}")))?;
        Ok(Box::new(DemoIndex { details }))
    }

    async fn load_statistics(&self, details: Vec<u8>) -> Result<Option<String>> {
        Ok(Some(format!("statistics:{}", details.len())))
    }
}

struct DemoIndex {
    details: String,
}

#[async_trait]
impl ScalarIndex for DemoIndex {
    async fn search(&self, query: &str) -> Result<String> {
        Ok(format!("{}|query={query}", self.details))
    }
}

#[no_mangle]
pub extern "C" fn xabi_manifest() -> *const PluginManifest {
    &MANIFEST
}

static ENTRY: PluginEntry = PluginEntry {
    trait_id: FfiStr::from_static(TRAIT_ID),
    name: FfiStr::from_static("demo-scalar-index"),
    impl_version: 1,
    make: make_plugin,
};

static ENTRIES: [PluginEntry; 1] = [ENTRY];
static MANIFEST: PluginManifest = PluginManifest::new(&ENTRIES);

unsafe extern "C" fn make_plugin() -> *mut c_void {
    let instance = Box::new(DemoPlugin);
    let vtable = Box::new(ScalarIndexPluginVTable {
        size: std::mem::size_of::<ScalarIndexPluginVTable>(),
        abi_version: ABI_VERSION,
        capabilities: cap::LOAD_STATISTICS,
        instance: Box::into_raw(instance) as *mut c_void,
        name: plugin_name,
        version: plugin_version,
        train_index: plugin_train_index,
        load_index: plugin_load_index,
        load_statistics: plugin_load_statistics,
        destroy: destroy_plugin,
        release: release_plugin_vtable,
    });
    Box::into_raw(vtable) as *mut c_void
}

unsafe extern "C" fn plugin_name(instance: *mut c_void) -> FfiOwned {
    xabi::catch_unwind_owned(|| {
        let Some(plugin) = plugin_ref(instance) else {
            return FfiOwned::from_string("<invalid plugin>".to_string());
        };
        FfiOwned::from_string(plugin.name())
    })
}

unsafe extern "C" fn plugin_version(instance: *mut c_void) -> u32 {
    let Some(plugin) = plugin_ref(instance) else {
        return 0;
    };
    plugin.version()
}

unsafe extern "C" fn plugin_train_index(
    instance: *mut c_void,
    stream: *mut scalar_index_abi::ArrowArrayStream,
    store: *const IndexStoreVTable,
    progress: *const IndexBuildProgressVTable,
    op: *const OpTrain,
    out: *mut scalar_index_abi::RpTrain,
) -> i32 {
    xabi::catch_unwind_code(|| {
        if out.is_null() {
            return xabi::ERR_INVALID_ARGUMENT;
        }
        let Some(plugin) = plugin_ref(instance) else {
            return xabi::ERR_INVALID_ARGUMENT;
        };
        if unsafe { scalar_index_abi::validate_store_vtable(store) }.is_err()
            || unsafe { scalar_index_abi::validate_progress_vtable(progress) }.is_err()
        {
            return xabi::ERR_INVALID_ARGUMENT;
        }
        let Some(op) = op.as_ref().copied() else {
            return xabi::ERR_INVALID_ARGUMENT;
        };
        if xabi::validate_size(op.size, std::mem::size_of::<OpTrain>(), "OpTrain").is_err() {
            return xabi::ERR_INVALID_ARGUMENT;
        }

        let store = Arc::new(ForeignIndexStore { vtable: store }) as Arc<dyn IndexStore>;
        let progress =
            Arc::new(ForeignProgress { vtable: progress }) as Arc<dyn IndexBuildProgress>;
        let data = match scalar_index_abi::ArrowStreamHandle::from_raw(stream) {
            Ok(data) => data,
            Err(_) => return xabi::ERR_INVALID_ARGUMENT,
        };

        match block_on(plugin.train_index(data, store, progress, op)) {
            Ok(result) => {
                *out = scalar_index_abi::RpTrain {
                    size: std::mem::size_of::<scalar_index_abi::RpTrain>(),
                    rows_seen: result.rows_seen,
                    progress_events: result.progress_events,
                    details: FfiOwned::from_vec(result.details),
                };
                xabi::OK
            }
            Err(_) => xabi::ERR_PLUGIN,
        }
    })
}

unsafe extern "C" fn plugin_load_index(
    instance: *mut c_void,
    details: FfiBytes,
    store: *const IndexStoreVTable,
    out: *mut *mut ScalarIndexVTable,
) -> i32 {
    xabi::catch_unwind_code(|| {
        if out.is_null() {
            return xabi::ERR_INVALID_ARGUMENT;
        }
        *out = ptr::null_mut();
        let Some(plugin) = plugin_ref(instance) else {
            return xabi::ERR_INVALID_ARGUMENT;
        };
        if unsafe { scalar_index_abi::validate_store_vtable(store) }.is_err() {
            return xabi::ERR_INVALID_ARGUMENT;
        }
        let details = match details.as_slice() {
            Ok(details) => details.to_vec(),
            Err(_) => return xabi::ERR_INVALID_ARGUMENT,
        };
        let store = Arc::new(ForeignIndexStore { vtable: store }) as Arc<dyn IndexStore>;

        match block_on(plugin.load_index(details, store)) {
            Ok(index) => {
                *out = export_index(index);
                xabi::OK
            }
            Err(_) => xabi::ERR_PLUGIN,
        }
    })
}

unsafe extern "C" fn plugin_load_statistics(
    instance: *mut c_void,
    details: FfiBytes,
    out: *mut FfiOwned,
) -> i32 {
    xabi::catch_unwind_code(|| {
        if out.is_null() {
            return xabi::ERR_INVALID_ARGUMENT;
        }
        let Some(plugin) = plugin_ref(instance) else {
            return xabi::ERR_INVALID_ARGUMENT;
        };
        let details = match details.as_slice() {
            Ok(details) => details.to_vec(),
            Err(_) => return xabi::ERR_INVALID_ARGUMENT,
        };
        match block_on(plugin.load_statistics(details)) {
            Ok(Some(value)) => {
                *out = FfiOwned::from_string(value);
                xabi::OK
            }
            Ok(None) => {
                *out = FfiOwned::empty();
                xabi::OK
            }
            Err(_) => xabi::ERR_PLUGIN,
        }
    })
}

unsafe extern "C" fn destroy_plugin(instance: *mut c_void) {
    if !instance.is_null() {
        drop(Box::from_raw(instance as *mut DemoPlugin));
    }
}

unsafe extern "C" fn release_plugin_vtable(vtable: *mut ScalarIndexPluginVTable) {
    if vtable.is_null() {
        return;
    }
    let vtable = Box::from_raw(vtable);
    (vtable.destroy)(vtable.instance);
}

unsafe fn plugin_ref<'a>(instance: *mut c_void) -> Option<&'a DemoPlugin> {
    (instance as *const DemoPlugin).as_ref()
}

fn export_index(index: Box<dyn ScalarIndex>) -> *mut ScalarIndexVTable {
    let instance = Box::new(index);
    let vtable = Box::new(ScalarIndexVTable {
        size: std::mem::size_of::<ScalarIndexVTable>(),
        abi_version: ABI_VERSION,
        capabilities: 0,
        instance: Box::into_raw(instance) as *mut c_void,
        search: index_search,
        destroy: destroy_index,
        release: release_index_vtable,
    });
    Box::into_raw(vtable)
}

unsafe extern "C" fn index_search(instance: *mut c_void, query: FfiStr) -> FfiOwned {
    xabi::catch_unwind_owned(|| {
        let Some(index) = (instance as *const Box<dyn ScalarIndex>).as_ref() else {
            return FfiOwned::from_string("<invalid index>".to_string());
        };
        let query = match query.as_str() {
            Ok(query) => query,
            Err(_) => return FfiOwned::from_string("<invalid query>".to_string()),
        };
        match block_on(index.search(query)) {
            Ok(result) => FfiOwned::from_string(result),
            Err(err) => FfiOwned::from_string(format!("<search error: {err}>")),
        }
    })
}

unsafe extern "C" fn destroy_index(instance: *mut c_void) {
    if !instance.is_null() {
        drop(Box::from_raw(instance as *mut Box<dyn ScalarIndex>));
    }
}

unsafe extern "C" fn release_index_vtable(vtable: *mut ScalarIndexVTable) {
    if vtable.is_null() {
        return;
    }
    let vtable = Box::from_raw(vtable);
    (vtable.destroy)(vtable.instance);
}

struct ForeignIndexStore {
    vtable: *const IndexStoreVTable,
}

unsafe impl Send for ForeignIndexStore {}
unsafe impl Sync for ForeignIndexStore {}

#[async_trait]
impl IndexStore for ForeignIndexStore {
    async fn put(&self, path: &str, data: &[u8]) -> Result<()> {
        let Some(vtable) = (unsafe { self.vtable.as_ref() }) else {
            return Err(Error::new("IndexStoreVTable pointer is null"));
        };
        let code = unsafe {
            (vtable.put)(
                vtable.instance,
                FfiStr::from_borrowed(path),
                FfiBytes::from_slice(data),
            )
        };
        scalar_index_abi::code_to_result(code, "IndexStore.put")
    }
}

struct ForeignProgress {
    vtable: *const IndexBuildProgressVTable,
}

unsafe impl Send for ForeignProgress {}
unsafe impl Sync for ForeignProgress {}

#[async_trait]
impl IndexBuildProgress for ForeignProgress {
    async fn update(&self, rows: i64) -> Result<()> {
        let Some(vtable) = (unsafe { self.vtable.as_ref() }) else {
            return Err(Error::new("IndexBuildProgressVTable pointer is null"));
        };
        let code = unsafe { (vtable.update)(vtable.instance, rows) };
        scalar_index_abi::code_to_result(code, "IndexBuildProgress.update")
    }
}
