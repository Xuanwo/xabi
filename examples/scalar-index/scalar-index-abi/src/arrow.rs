use std::ffi::c_void;
use std::ptr;

use crate::{Error, Result};

#[repr(C)]
pub struct ArrowArray {
    pub length: i64,
    pub null_count: i64,
    pub offset: i64,
    pub n_buffers: i64,
    pub n_children: i64,
    pub buffers: *mut *const c_void,
    pub children: *mut *mut ArrowArray,
    pub dictionary: *mut ArrowArray,
    pub release: Option<unsafe extern "C" fn(*mut ArrowArray)>,
    pub private_data: *mut c_void,
}

impl ArrowArray {
    pub fn empty() -> Self {
        Self {
            length: 0,
            null_count: 0,
            offset: 0,
            n_buffers: 0,
            n_children: 0,
            buffers: ptr::null_mut(),
            children: ptr::null_mut(),
            dictionary: ptr::null_mut(),
            release: None,
            private_data: ptr::null_mut(),
        }
    }
}

impl Default for ArrowArray {
    fn default() -> Self {
        Self::empty()
    }
}

#[repr(C)]
pub struct ArrowSchema {
    pub format: *const i8,
    pub name: *const i8,
    pub metadata: *const i8,
    pub flags: i64,
    pub n_children: i64,
    pub children: *mut *mut ArrowSchema,
    pub dictionary: *mut ArrowSchema,
    pub release: Option<unsafe extern "C" fn(*mut ArrowSchema)>,
    pub private_data: *mut c_void,
}

impl ArrowSchema {
    pub fn empty() -> Self {
        Self {
            format: ptr::null(),
            name: ptr::null(),
            metadata: ptr::null(),
            flags: 0,
            n_children: 0,
            children: ptr::null_mut(),
            dictionary: ptr::null_mut(),
            release: None,
            private_data: ptr::null_mut(),
        }
    }
}

impl Default for ArrowSchema {
    fn default() -> Self {
        Self::empty()
    }
}

#[repr(C)]
pub struct ArrowArrayStream {
    pub get_schema: Option<unsafe extern "C" fn(*mut ArrowArrayStream, *mut ArrowSchema) -> i32>,
    pub get_next: Option<unsafe extern "C" fn(*mut ArrowArrayStream, *mut ArrowArray) -> i32>,
    pub get_last_error: Option<unsafe extern "C" fn(*mut ArrowArrayStream) -> *const i8>,
    pub release: Option<unsafe extern "C" fn(*mut ArrowArrayStream)>,
    pub private_data: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct XabiArrowStreamHandle {
    pub size: usize,
    pub abi_version: u32,
    pub stream: *mut ArrowArrayStream,
}

impl XabiArrowStreamHandle {
    pub const ABI_VERSION: u32 = xabi::ABI_VERSION;
    pub const MIN_SIZE: usize = std::mem::size_of::<Self>();

    pub fn validate(&self) -> xabi::Result<()> {
        xabi::validate_size(self.size, Self::MIN_SIZE, "XabiArrowStreamHandle")?;
        xabi::validate_abi_version(self.abi_version, Self::ABI_VERSION, "XabiArrowStreamHandle")?;
        if self.stream.is_null() {
            return Err(xabi::Error::NullPointer("XabiArrowStreamHandle::stream"));
        }
        Ok(())
    }
}

unsafe impl Send for XabiArrowStreamHandle {}
unsafe impl Sync for XabiArrowStreamHandle {}

#[derive(Clone, Copy, Debug)]
pub struct ArrowStreamHandle {
    raw: *mut ArrowArrayStream,
}

impl ArrowStreamHandle {
    /// # Safety
    ///
    /// `raw` must point to a live `ArrowArrayStream`, and the caller must ensure no concurrent
    /// mutable access happens while the handle is used.
    pub unsafe fn from_raw(raw: *mut ArrowArrayStream) -> Result<Self> {
        if raw.is_null() {
            return Err(Error::new("ArrowArrayStream pointer is null"));
        }
        Ok(Self { raw })
    }

    pub fn as_raw(&self) -> *mut ArrowArrayStream {
        self.raw
    }
}

unsafe impl Send for ArrowStreamHandle {}

impl xabi::XabiType for ArrowStreamHandle {
    type Wire = XabiArrowStreamHandle;

    fn into_wire(self) -> Self::Wire {
        XabiArrowStreamHandle {
            size: std::mem::size_of::<XabiArrowStreamHandle>(),
            abi_version: XabiArrowStreamHandle::ABI_VERSION,
            stream: self.raw,
        }
    }

    unsafe fn from_wire(wire: *const Self::Wire) -> xabi::Result<Self> {
        let wire = unsafe {
            wire.as_ref()
                .ok_or(xabi::Error::NullPointer("XabiArrowStreamHandle pointer"))?
        };
        wire.validate()?;
        unsafe { Self::from_raw(wire.stream).map_err(|err| xabi::Error::Export(err.to_string())) }
    }
}

pub struct InMemoryArrowStream {
    raw: ArrowArrayStream,
}

impl InMemoryArrowStream {
    pub fn new(batch_lengths: impl IntoIterator<Item = i64>) -> Self {
        let state = Box::new(InMemoryStreamState {
            batch_lengths: batch_lengths.into_iter().collect(),
            position: 0,
        });
        Self {
            raw: ArrowArrayStream {
                get_schema: Some(in_memory_get_schema),
                get_next: Some(in_memory_get_next),
                get_last_error: None,
                release: Some(in_memory_stream_release),
                private_data: Box::into_raw(state) as *mut c_void,
            },
        }
    }

    pub fn handle(&mut self) -> ArrowStreamHandle {
        unsafe { ArrowStreamHandle::from_raw(&mut self.raw).expect("in-memory stream is non-null") }
    }
}

impl Drop for InMemoryArrowStream {
    fn drop(&mut self) {
        if let Some(release) = self.raw.release {
            unsafe {
                release(&mut self.raw);
            }
        }
    }
}

unsafe impl Send for InMemoryArrowStream {}

struct InMemoryStreamState {
    batch_lengths: Vec<i64>,
    position: usize,
}

unsafe extern "C" fn in_memory_get_schema(
    _stream: *mut ArrowArrayStream,
    out: *mut ArrowSchema,
) -> i32 {
    if out.is_null() {
        return xabi::ERR_INVALID_ARGUMENT;
    }

    *out = ArrowSchema::empty();
    (*out).release = Some(release_schema);
    xabi::OK
}

unsafe extern "C" fn in_memory_get_next(
    stream: *mut ArrowArrayStream,
    out: *mut ArrowArray,
) -> i32 {
    if stream.is_null() || out.is_null() {
        return xabi::ERR_INVALID_ARGUMENT;
    }
    let state = &mut *((*stream).private_data as *mut InMemoryStreamState);
    if state.position >= state.batch_lengths.len() {
        *out = ArrowArray::empty();
        return xabi::OK;
    }

    let length = state.batch_lengths[state.position];
    state.position += 1;

    *out = ArrowArray::empty();
    (*out).length = length;
    (*out).release = Some(release_array);
    xabi::OK
}

unsafe extern "C" fn in_memory_stream_release(stream: *mut ArrowArrayStream) {
    if stream.is_null() || (*stream).release.is_none() {
        return;
    }
    let state = (*stream).private_data as *mut InMemoryStreamState;
    if !state.is_null() {
        drop(Box::from_raw(state));
    }
    (*stream).private_data = ptr::null_mut();
    (*stream).release = None;
}

unsafe extern "C" fn release_array(array: *mut ArrowArray) {
    if array.is_null() || (*array).release.is_none() {
        return;
    }
    (*array).release = None;
}

unsafe extern "C" fn release_schema(schema: *mut ArrowSchema) {
    if schema.is_null() || (*schema).release.is_none() {
        return;
    }
    (*schema).release = None;
}

pub fn drain_arrow_stream(stream: ArrowStreamHandle) -> Result<i64> {
    unsafe {
        let stream = stream.as_raw();
        let get_next = (*stream)
            .get_next
            .ok_or_else(|| Error::new("ArrowArrayStream.get_next is null"))?;
        let mut rows = 0;

        loop {
            let mut array = ArrowArray::empty();
            let code = get_next(stream, &mut array);
            xabi::status_to_result(code, "ArrowArrayStream.get_next")?;
            if array.release.is_none() {
                break;
            }
            rows += array.length;
            if let Some(release) = array.release {
                release(&mut array);
            }
        }

        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_arrow_stream_drains_rows() -> Result<()> {
        let mut stream = InMemoryArrowStream::new([3, 5]);

        assert_eq!(drain_arrow_stream(stream.handle())?, 8);
        Ok(())
    }

    #[test]
    fn arrow_stream_handle_rejects_null_pointer() {
        let err = unsafe { ArrowStreamHandle::from_raw(ptr::null_mut()) }
            .expect_err("null stream must fail");

        assert!(err.to_string().contains("ArrowArrayStream pointer is null"));
    }
}
