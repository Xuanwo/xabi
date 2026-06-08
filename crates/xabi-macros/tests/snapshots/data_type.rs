pub struct BuildInput {
    pub value: u64,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiV1DataBuildInput {
    pub size: usize,
    pub abi_version: u32,
    pub value: <u64 as ::xabi::XabiType>::Wire,
}
impl XabiV1DataBuildInput {
    pub const ABI_VERSION: u32 = ::xabi::ABI_VERSION;
    pub const MIN_SIZE: usize = std::mem::size_of::<Self>();
    pub fn validate(&self) -> ::xabi::Result<()> {
        ::xabi::validate_size(
            self.size,
            Self::MIN_SIZE,
            stringify!(XabiV1DataBuildInput),
        )?;
        ::xabi::validate_abi_version(
            self.abi_version,
            Self::ABI_VERSION,
            stringify!(XabiV1DataBuildInput),
        )?;
        Ok(())
    }
}
impl BuildInput {
    pub fn new(value: u64) -> Self {
        Self { value }
    }
}
impl ::xabi::XabiType for BuildInput {
    type Wire = XabiV1DataBuildInput;
    fn into_wire(self) -> Self::Wire {
        let mut wire = std::mem::MaybeUninit::<XabiV1DataBuildInput>::zeroed();
        unsafe {
            let wire_ptr = wire.as_mut_ptr();
            std::ptr::addr_of_mut!((* wire_ptr).size)
                .write(std::mem::size_of::<XabiV1DataBuildInput>());
            std::ptr::addr_of_mut!((* wire_ptr).abi_version)
                .write(XabiV1DataBuildInput::ABI_VERSION);
            std::ptr::addr_of_mut!((* wire_ptr).value)
                .write(::xabi::XabiType::into_wire(self.value));
            wire.assume_init()
        }
    }
    unsafe fn from_wire(wire: *const Self::Wire) -> ::xabi::Result<Self> {
        let wire = unsafe {
            wire.as_ref()
                .ok_or(
                    ::xabi::Error::NullPointer(
                        concat!(stringify!(XabiV1DataBuildInput), " pointer"),
                    ),
                )?
        };
        wire.validate()?;
        Ok(Self {
            value: unsafe {
                <u64 as ::xabi::XabiType>::from_wire(std::ptr::addr_of!(wire.value))
            }?,
        })
    }
}
