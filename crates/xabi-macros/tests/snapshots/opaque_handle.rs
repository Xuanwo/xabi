pub struct StreamHandle {
    raw: *mut ArrowArrayStream,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiV1OpaqueStreamHandle {
    pub size: usize,
    pub abi_version: u32,
    pub raw: *mut ArrowArrayStream,
}
impl XabiV1OpaqueStreamHandle {
    pub const ABI_VERSION: u32 = ::xabi::ABI_VERSION;
    pub const MIN_SIZE: usize = std::mem::offset_of!(XabiV1OpaqueStreamHandle, raw)
        + std::mem::size_of::<*mut ArrowArrayStream>();
    pub const FULL_SIZE: usize = std::mem::size_of::<Self>();
    pub fn validate(&self) -> ::xabi::Result<()> {
        ::xabi::validate_size(
            self.size,
            Self::MIN_SIZE,
            stringify!(XabiV1OpaqueStreamHandle),
        )?;
        ::xabi::validate_abi_version(
            self.abi_version,
            Self::ABI_VERSION,
            stringify!(XabiV1OpaqueStreamHandle),
        )?;
        if self.raw.is_null() {
            return Err(
                ::xabi::Error::NullPointer(
                    concat!(stringify!(XabiV1OpaqueStreamHandle), "::", stringify!(raw),),
                ),
            );
        }
        Ok(())
    }
}
unsafe impl Send for XabiV1OpaqueStreamHandle {}
unsafe impl Sync for XabiV1OpaqueStreamHandle {}
impl StreamHandle {
    pub unsafe fn from_raw(raw: *mut ArrowArrayStream) -> ::xabi::Result<Self> {
        if raw.is_null() {
            return Err(
                ::xabi::Error::NullPointer(
                    concat!(stringify!(StreamHandle), "::", stringify!(raw),),
                ),
            );
        }
        Ok(Self { raw })
    }
    pub fn as_raw(&self) -> *mut ArrowArrayStream {
        self.raw
    }
}
impl ::xabi::XabiType for StreamHandle {
    type Wire = XabiV1OpaqueStreamHandle;
    fn into_wire(self) -> Self::Wire {
        XabiV1OpaqueStreamHandle {
            size: std::mem::size_of::<XabiV1OpaqueStreamHandle>(),
            abi_version: XabiV1OpaqueStreamHandle::ABI_VERSION,
            raw: self.raw,
        }
    }
    unsafe fn from_wire(wire: *const Self::Wire) -> ::xabi::Result<Self> {
        let wire = unsafe {
            wire.as_ref()
                .ok_or(
                    ::xabi::Error::NullPointer(
                        concat!(stringify!(XabiV1OpaqueStreamHandle), " pointer"),
                    ),
                )?
        };
        wire.validate()?;
        Ok(Self { raw: wire.raw })
    }
}
