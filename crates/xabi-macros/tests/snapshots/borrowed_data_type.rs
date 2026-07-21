pub struct CallbackInput<'a> {
    pub callback: XabiV1BorrowedTraitCallback<'a>,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiV1DataCallbackInput {
    pub size: usize,
    pub abi_version: u32,
    pub callback: <XabiV1BorrowedTraitCallback<'static> as ::xabi::XabiType>::Wire,
}
impl XabiV1DataCallbackInput {
    pub const ABI_VERSION: u32 = ::xabi::ABI_VERSION;
    pub const FULL_SIZE: usize = std::mem::size_of::<Self>();
    pub const MIN_SIZE: usize = Self::FULL_SIZE;
    pub fn validate(&self) -> ::xabi::Result<()> {
        ::xabi::validate_exact_size(
            self.size,
            Self::FULL_SIZE,
            stringify!(XabiV1DataCallbackInput),
        )?;
        ::xabi::validate_abi_version(
            self.abi_version,
            Self::ABI_VERSION,
            stringify!(XabiV1DataCallbackInput),
        )?;
        Ok(())
    }
    pub fn field_available(&self, field: &str) -> bool {
        match field {
            stringify!(callback) => self.size == Self::FULL_SIZE,
            _ => false,
        }
    }
}
impl<'a> CallbackInput<'a> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(callback: XabiV1BorrowedTraitCallback<'a>) -> Self {
        Self { callback }
    }
}
impl<'a> ::xabi::XabiType for CallbackInput<'a> {
    type Wire = XabiV1DataCallbackInput;
    const WIRE_TYPE_NAME: &'static str = stringify!(XabiV1DataCallbackInput);
    fn into_wire(self) -> Self::Wire {
        let mut wire = std::mem::MaybeUninit::<XabiV1DataCallbackInput>::zeroed();
        unsafe {
            let wire_ptr = wire.as_mut_ptr();
            std::ptr::addr_of_mut!((* wire_ptr).size)
                .write(std::mem::size_of::<XabiV1DataCallbackInput>());
            std::ptr::addr_of_mut!((* wire_ptr).abi_version)
                .write(XabiV1DataCallbackInput::ABI_VERSION);
            std::ptr::addr_of_mut!((* wire_ptr).callback)
                .write(::xabi::XabiType::into_wire(self.callback));
            wire.assume_init()
        }
    }
    unsafe fn from_wire(wire: *const Self::Wire) -> ::xabi::Result<Self> {
        let wire = unsafe {
            wire.as_ref()
                .ok_or(
                    ::xabi::Error::NullPointer(
                        concat!(stringify!(XabiV1DataCallbackInput), " pointer"),
                    ),
                )?
        };
        wire.validate()?;
        if !wire.field_available(stringify!(callback)) {
            return Err(
                ::xabi::Error::AbiMismatch(
                    format!(
                        "{} is missing required field {}",
                        stringify!(XabiV1DataCallbackInput), stringify!(callback),
                    ),
                ),
            );
        }
        Ok(Self {
            callback: unsafe {
                <XabiV1BorrowedTraitCallback<
                    'a,
                > as ::xabi::XabiType>::from_wire(std::ptr::addr_of!(wire.callback))
            }?,
        })
    }
    fn collect_xabi_layout(collector: &mut dyn ::xabi::XabiLayoutCollector) {
        <XabiV1BorrowedTraitCallback<
            'static,
        > as ::xabi::XabiType>::collect_xabi_layout(collector);
        const __XABI_FIELDS: &[::xabi::XabiFieldLayout] = &[
            ::xabi::XabiFieldLayout::new(
                "size",
                std::mem::offset_of!(XabiV1DataCallbackInput, size),
                "usize",
            ),
            ::xabi::XabiFieldLayout::new(
                "abi_version",
                std::mem::offset_of!(XabiV1DataCallbackInput, abi_version),
                "u32",
            ),
            ::xabi::XabiFieldLayout::new(
                stringify!(callback),
                std::mem::offset_of!(XabiV1DataCallbackInput, callback),
                <XabiV1BorrowedTraitCallback<
                    'static,
                > as ::xabi::XabiType>::WIRE_TYPE_NAME,
            ),
        ];
        collector
            .push(
                ::xabi::XabiLayoutItem::Type(
                    ::xabi::XabiTypeLayout::new(
                        concat!(
                            module_path!(), "::", stringify!(XabiV1DataCallbackInput)
                        ),
                        ::xabi::XabiLayoutStability::Fixed,
                        std::mem::size_of::<XabiV1DataCallbackInput>(),
                        std::mem::align_of::<XabiV1DataCallbackInput>(),
                        __XABI_FIELDS,
                    ),
                ),
            );
    }
    fn retain_module(&mut self, module: &std::sync::Arc<::xabi::ModuleHandle>) {
        <XabiV1BorrowedTraitCallback<
            'a,
        > as ::xabi::XabiType>::retain_module(&mut self.callback, module);
    }
}
