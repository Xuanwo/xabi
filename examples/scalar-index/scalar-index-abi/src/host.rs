use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;

use async_trait::async_trait;
use futures::executor::block_on;
use xabi::{FfiBytes, FfiStr};

use crate::{Error, Result};

pub const ABI_VERSION: u32 = 1;

#[async_trait]
pub trait IndexStore: Send + Sync {
    async fn put(&self, path: &str, data: &[u8]) -> Result<()>;
}

#[async_trait]
pub trait IndexBuildProgress: Send + Sync {
    async fn update(&self, rows: i64) -> Result<()>;
}

#[repr(C)]
pub struct IndexStoreVTable {
    pub size: usize,
    pub abi_version: u32,
    pub capabilities: u64,
    pub instance: *mut c_void,
    pub put: unsafe extern "C" fn(*mut c_void, FfiStr, FfiBytes) -> i32,
}

#[repr(C)]
pub struct IndexBuildProgressVTable {
    pub size: usize,
    pub abi_version: u32,
    pub capabilities: u64,
    pub instance: *mut c_void,
    pub update: unsafe extern "C" fn(*mut c_void, i64) -> i32,
}

pub struct HostVTables {
    _store_state: Box<HostStoreState>,
    _progress_state: Box<HostProgressState>,
    store: IndexStoreVTable,
    progress: IndexBuildProgressVTable,
}

impl HostVTables {
    pub fn new(store: Arc<dyn IndexStore>, progress: Arc<dyn IndexBuildProgress>) -> Self {
        let mut store_state = Box::new(HostStoreState { inner: store });
        let mut progress_state = Box::new(HostProgressState { inner: progress });

        Self {
            store: IndexStoreVTable {
                size: std::mem::size_of::<IndexStoreVTable>(),
                abi_version: ABI_VERSION,
                capabilities: 0,
                instance: store_state.as_mut() as *mut HostStoreState as *mut c_void,
                put: host_store_put,
            },
            progress: IndexBuildProgressVTable {
                size: std::mem::size_of::<IndexBuildProgressVTable>(),
                abi_version: ABI_VERSION,
                capabilities: 0,
                instance: progress_state.as_mut() as *mut HostProgressState as *mut c_void,
                update: host_progress_update,
            },
            _store_state: store_state,
            _progress_state: progress_state,
        }
    }

    pub fn store(&self) -> *const IndexStoreVTable {
        &self.store
    }

    pub fn progress(&self) -> *const IndexBuildProgressVTable {
        &self.progress
    }
}

struct HostStoreState {
    inner: Arc<dyn IndexStore>,
}

struct HostProgressState {
    inner: Arc<dyn IndexBuildProgress>,
}

unsafe extern "C" fn host_store_put(instance: *mut c_void, path: FfiStr, data: FfiBytes) -> i32 {
    xabi::catch_unwind_code(|| {
        let Some(instance) = NonNull::new(instance as *mut HostStoreState) else {
            return xabi::ERR_INVALID_ARGUMENT;
        };
        let path = match path.as_str() {
            Ok(path) => path,
            Err(_) => return xabi::ERR_INVALID_ARGUMENT,
        };
        let data = match data.as_slice() {
            Ok(data) => data,
            Err(_) => return xabi::ERR_INVALID_ARGUMENT,
        };

        match block_on(instance.as_ref().inner.put(path, data)) {
            Ok(()) => xabi::OK,
            Err(_) => xabi::ERR_HOST,
        }
    })
}

unsafe extern "C" fn host_progress_update(instance: *mut c_void, rows: i64) -> i32 {
    xabi::catch_unwind_code(|| {
        let Some(instance) = NonNull::new(instance as *mut HostProgressState) else {
            return xabi::ERR_INVALID_ARGUMENT;
        };
        match block_on(instance.as_ref().inner.update(rows)) {
            Ok(()) => xabi::OK,
            Err(_) => xabi::ERR_HOST,
        }
    })
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
    xabi::validate_size(
        vtable.size,
        std::mem::size_of::<IndexStoreVTable>(),
        "IndexStoreVTable",
    )?;
    xabi::validate_abi_version(vtable.abi_version, ABI_VERSION, "IndexStoreVTable")?;
    Ok(())
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
    xabi::validate_size(
        vtable.size,
        std::mem::size_of::<IndexBuildProgressVTable>(),
        "IndexBuildProgressVTable",
    )?;
    xabi::validate_abi_version(vtable.abi_version, ABI_VERSION, "IndexBuildProgressVTable")?;
    Ok(())
}
