#[allow(async_fn_in_trait)]
pub trait Factory: Send + Sync + 'static {
    fn make(
        &self,
        name: &str,
    ) -> impl std::future::Future<Output = Result<impl Child + 'static, Error>> + Send;
    #[doc(hidden)]
    const __XABI_ID: &'static str = FACTORY_TRAIT_ID;
    #[doc(hidden)]
    const __XABI_VERSION: u32 = ABI_VERSION;
    #[doc(hidden)]
    fn __xabi_export(value: Self) -> *mut std::ffi::c_void
    where
        Self: Sized,
    {
        <XabiV1AbiTraitFactory as ::xabi::XabiContract<Self>>::export(value)
    }
}
pub struct XabiV1AbiTraitFactory;
impl XabiV1AbiTraitFactory {
    pub const ID: &'static str = FACTORY_TRAIT_ID;
    pub const VERSION: u32 = ABI_VERSION;
    pub fn xabi_export<P: Factory>(value: P) -> *mut XabiV1VtableTraitFactory {
        <Self as ::xabi::XabiContract<P>>::export(value) as *mut XabiV1VtableTraitFactory
    }
    unsafe extern "C" fn make<P: Factory>(
        instance: *mut std::ffi::c_void,
        name: ::xabi::XabiStr,
        out: *mut ::xabi::XabiFuture,
    ) -> i32 {
        ::xabi::catch_unwind_code(|| {
            let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
            let Some(out) = (unsafe { out.as_mut() }) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
            let Ok(name) = (unsafe { name.as_str() }) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
            let name = name.to_string();
            *out = ::xabi::XabiFuture::from_result_bytes(async move {
                plugin
                    .make(&name)
                    .await
                    .map(|value| {
                        let raw = XabiV1AbiTraitChild::xabi_export(value);
                        let wire = XabiV1OwnedRefTraitChild {
                            size: std::mem::size_of::<XabiV1OwnedRefTraitChild>(),
                            abi_version: XabiV1OwnedRefTraitChild::ABI_VERSION,
                            vtable: raw,
                        };
                        let bytes = unsafe {
                            std::slice::from_raw_parts(
                                std::ptr::addr_of!(wire).cast::<u8>(),
                                std::mem::size_of::<XabiV1OwnedRefTraitChild>(),
                            )
                        };
                        bytes.to_vec()
                    })
            });
            ::xabi::OK
        })
    }
    unsafe extern "C" fn __xabi_destroy<P: Factory>(instance: *mut std::ffi::c_void) {
        if !instance.is_null() {
            drop(unsafe { Box::from_raw(instance as *mut P) });
        }
    }
    unsafe extern "C" fn __xabi_release(vtable: *mut XabiV1VtableTraitFactory) {
        let Some(vtable) = (unsafe { vtable.as_mut() }) else {
            return;
        };
        unsafe { (vtable.destroy)(vtable.instance) };
        drop(unsafe { Box::from_raw(vtable) });
    }
    fn __xabi_impl_ref<P: Factory>(
        instance: *mut std::ffi::c_void,
    ) -> Option<&'static P> {
        unsafe { (instance as *const P).as_ref() }
    }
}
#[repr(C)]
pub struct XabiV1VtableTraitFactory {
    pub size: usize,
    pub abi_version: u32,
    pub capabilities: u64,
    pub instance: *mut std::ffi::c_void,
    pub destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    pub release: unsafe extern "C" fn(*mut XabiV1VtableTraitFactory),
    pub make: unsafe extern "C" fn(
        *mut std::ffi::c_void,
        ::xabi::XabiStr,
        *mut ::xabi::XabiFuture,
    ) -> i32,
}
impl XabiV1VtableTraitFactory {
    pub const ABI_VERSION: u32 = ABI_VERSION;
    pub const MIN_SIZE: usize = std::mem::offset_of!(XabiV1VtableTraitFactory, release)
        + std::mem::size_of::<unsafe extern "C" fn(*mut XabiV1VtableTraitFactory)>();
    pub const FULL_SIZE: usize = std::mem::size_of::<Self>();
    pub fn validate(&self) -> ::xabi::Result<()> {
        ::xabi::validate_size(
            self.size,
            Self::MIN_SIZE,
            stringify!(XabiV1VtableTraitFactory),
        )?;
        ::xabi::validate_abi_version(
            self.abi_version,
            Self::ABI_VERSION,
            stringify!(XabiV1VtableTraitFactory),
        )?;
        if self.instance.is_null() {
            return Err(
                ::xabi::Error::NullPointer(
                    concat!(stringify!(XabiV1VtableTraitFactory), "::instance"),
                ),
            );
        }
        Ok(())
    }
    pub fn field_available(&self, field: &str) -> bool {
        match field {
            stringify!(make) => {
                let field_end = std::mem::offset_of!(XabiV1VtableTraitFactory, make)
                    + std::mem::size_of_val(&self.make);
                self.size >= field_end
            }
            "destroy" => {
                let field_end = std::mem::offset_of!(XabiV1VtableTraitFactory, destroy)
                    + std::mem::size_of_val(&self.destroy);
                self.size >= field_end
            }
            "release" => {
                let field_end = std::mem::offset_of!(XabiV1VtableTraitFactory, release)
                    + std::mem::size_of_val(&self.release);
                self.size >= field_end
            }
            _ => false,
        }
    }
}
pub struct XabiV1HandleTraitFactory {
    vtable: std::ptr::NonNull<XabiV1VtableTraitFactory>,
    _module: std::sync::Arc<::xabi::ModuleHandle>,
}
unsafe impl Send for XabiV1HandleTraitFactory {}
unsafe impl Sync for XabiV1HandleTraitFactory {}
impl XabiV1HandleTraitFactory {
    pub unsafe fn xabi_from_vtable(
        vtable: *mut XabiV1VtableTraitFactory,
        module: std::sync::Arc<::xabi::ModuleHandle>,
    ) -> ::xabi::Result<Self> {
        let vtable = std::ptr::NonNull::new(vtable)
            .ok_or(
                ::xabi::Error::NullPointer(
                    concat!(stringify!(XabiV1VtableTraitFactory), " pointer"),
                ),
            )?;
        unsafe { vtable.as_ref() }.validate()?;
        Ok(Self { vtable, _module: module })
    }
    pub unsafe fn xabi_from_export(
        export: &::xabi::XabiExport,
        module: std::sync::Arc<::xabi::ModuleHandle>,
    ) -> ::xabi::Result<Self> {
        export.validate()?;
        let abi_id = unsafe { export.abi_id.as_str() }?;
        if abi_id != FACTORY_TRAIT_ID {
            return Err(
                ::xabi::Error::Export(
                    format!(
                        "module export has abi_id {abi_id}, expected {}",
                        FACTORY_TRAIT_ID,
                    ),
                ),
            );
        }
        if export.contract_version != ABI_VERSION {
            return Err(
                ::xabi::Error::AbiMismatch(
                    format!(
                        "module export {} has contract version {}, expected {}",
                        FACTORY_TRAIT_ID, export.contract_version, ABI_VERSION,
                    ),
                ),
            );
        }
        let raw = unsafe { (export.make)() } as *mut XabiV1VtableTraitFactory;
        unsafe { Self::xabi_from_vtable(raw, module) }
    }
    pub unsafe fn xabi_load(module: &::xabi::Module) -> ::xabi::Result<Self> {
        let handle = module.handle();
        let mut version_mismatch = None;
        for export in module.exports()? {
            let abi_id = unsafe { export.abi_id.as_str() }?;
            if abi_id == FACTORY_TRAIT_ID {
                if export.contract_version == ABI_VERSION {
                    return unsafe { Self::xabi_from_export(export, handle) };
                }
                version_mismatch = Some(export.contract_version);
            }
        }
        if let Some(actual) = version_mismatch {
            return Err(
                ::xabi::Error::AbiMismatch(
                    format!(
                        "module contains xabi export {} with contract version {}, expected {}",
                        FACTORY_TRAIT_ID, actual, ABI_VERSION,
                    ),
                ),
            );
        }
        Err(
            ::xabi::Error::Export(
                format!("module does not contain xabi export {}", FACTORY_TRAIT_ID,),
            ),
        )
    }
    pub fn xabi_module(&self) -> std::sync::Arc<::xabi::ModuleHandle> {
        std::sync::Arc::clone(&self._module)
    }
    fn vtable(&self) -> &XabiV1VtableTraitFactory {
        unsafe { self.vtable.as_ref() }
    }
    pub async fn make(
        &self,
        name: &str,
    ) -> std::result::Result<XabiV1HandleTraitChild, ::xabi::XabiCallError<Error>> {
        let vtable = self.vtable();
        if !vtable.field_available(stringify!(make)) {
            return Err(
                ::xabi::XabiCallError::Runtime(
                    ::xabi::Error::AbiMismatch(
                        format!(
                            "Xabi.{} is not available in this vtable", stringify!(make),
                        ),
                    ),
                ),
            );
        }
        let mut future = ::xabi::XabiFuture::empty();
        let code = unsafe {
            (vtable
                .make)(
                vtable.instance,
                ::xabi::XabiStr::from_borrowed(name),
                &mut future,
            )
        };
        ::xabi::status_to_result(code, concat!("Xabi.", stringify!(make)))
            .map_err(::xabi::XabiCallError::Runtime)?;
        let bytes = ::xabi::XabiTypedFuture::<Error>::new(future)
            .map_err(::xabi::XabiCallError::Runtime)?
            .await?;
        let payload = ::xabi::XabiOwnedBytes::from_vec(bytes);
        let wire = unsafe {
            <XabiV1OwnedRefTraitChild as ::xabi::XabiType>::from_payload(payload)
                .map_err(::xabi::XabiCallError::Runtime)?
        };
        unsafe {
            XabiV1HandleTraitChild::xabi_from_vtable(wire.vtable, self.xabi_module())
                .map_err(::xabi::XabiCallError::Runtime)
        }
    }
}
#[derive(Clone, Copy)]
pub struct XabiV1BorrowedTraitFactory {
    vtable: std::ptr::NonNull<XabiV1VtableTraitFactory>,
}
unsafe impl Send for XabiV1BorrowedTraitFactory {}
unsafe impl Sync for XabiV1BorrowedTraitFactory {}
impl XabiV1BorrowedTraitFactory {
    pub unsafe fn xabi_from_vtable(
        vtable: *const XabiV1VtableTraitFactory,
    ) -> ::xabi::Result<Self> {
        let vtable = std::ptr::NonNull::new(vtable as *mut XabiV1VtableTraitFactory)
            .ok_or(
                ::xabi::Error::NullPointer(
                    concat!(stringify!(XabiV1VtableTraitFactory), " pointer"),
                ),
            )?;
        unsafe { vtable.as_ref() }.validate()?;
        Ok(Self { vtable })
    }
    pub fn xabi_as_ptr(&self) -> *const XabiV1VtableTraitFactory {
        self.vtable.as_ptr()
    }
    fn vtable(&self) -> &XabiV1VtableTraitFactory {
        unsafe { self.vtable.as_ref() }
    }
    pub async fn make(
        &self,
        name: &str,
    ) -> std::result::Result<XabiV1OwnedTraitChild, ::xabi::XabiCallError<Error>> {
        let vtable = self.vtable();
        if !vtable.field_available(stringify!(make)) {
            return Err(
                ::xabi::XabiCallError::Runtime(
                    ::xabi::Error::AbiMismatch(
                        format!(
                            "Xabi.{} is not available in this vtable", stringify!(make),
                        ),
                    ),
                ),
            );
        }
        let mut future = ::xabi::XabiFuture::empty();
        let code = unsafe {
            (vtable
                .make)(
                vtable.instance,
                ::xabi::XabiStr::from_borrowed(name),
                &mut future,
            )
        };
        ::xabi::status_to_result(code, concat!("Xabi.", stringify!(make)))
            .map_err(::xabi::XabiCallError::Runtime)?;
        let bytes = ::xabi::XabiTypedFuture::<Error>::new(future)
            .map_err(::xabi::XabiCallError::Runtime)?
            .await?;
        let payload = ::xabi::XabiOwnedBytes::from_vec(bytes);
        let wire = unsafe {
            <XabiV1OwnedRefTraitChild as ::xabi::XabiType>::from_payload(payload)
                .map_err(::xabi::XabiCallError::Runtime)?
        };
        unsafe {
            XabiV1OwnedTraitChild::xabi_from_vtable(wire.vtable)
                .map_err(::xabi::XabiCallError::Runtime)
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiV1RefTraitFactory {
    pub size: usize,
    pub abi_version: u32,
    pub vtable: *const XabiV1VtableTraitFactory,
}
unsafe impl Send for XabiV1RefTraitFactory {}
unsafe impl Sync for XabiV1RefTraitFactory {}
impl XabiV1RefTraitFactory {
    pub const ABI_VERSION: u32 = ABI_VERSION;
    pub const MIN_SIZE: usize = std::mem::size_of::<Self>();
    pub fn validate(&self) -> ::xabi::Result<()> {
        ::xabi::validate_size(
            self.size,
            Self::MIN_SIZE,
            stringify!(XabiV1RefTraitFactory),
        )?;
        ::xabi::validate_abi_version(
            self.abi_version,
            Self::ABI_VERSION,
            stringify!(XabiV1RefTraitFactory),
        )?;
        if self.vtable.is_null() {
            return Err(
                ::xabi::Error::NullPointer(
                    concat!(stringify!(XabiV1RefTraitFactory), "::vtable"),
                ),
            );
        }
        Ok(())
    }
}
impl ::xabi::XabiType for XabiV1BorrowedTraitFactory {
    type Wire = XabiV1RefTraitFactory;
    fn into_wire(self) -> Self::Wire {
        XabiV1RefTraitFactory {
            size: std::mem::size_of::<XabiV1RefTraitFactory>(),
            abi_version: XabiV1RefTraitFactory::ABI_VERSION,
            vtable: self.vtable.as_ptr(),
        }
    }
    unsafe fn from_wire(wire: *const Self::Wire) -> ::xabi::Result<Self> {
        let wire = unsafe {
            wire.as_ref()
                .ok_or(
                    ::xabi::Error::NullPointer(
                        concat!(stringify!(XabiV1RefTraitFactory), " pointer"),
                    ),
                )?
        };
        wire.validate()?;
        unsafe { Self::xabi_from_vtable(wire.vtable) }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiV1OwnedRefTraitFactory {
    pub size: usize,
    pub abi_version: u32,
    pub vtable: *mut XabiV1VtableTraitFactory,
}
unsafe impl Send for XabiV1OwnedRefTraitFactory {}
unsafe impl Sync for XabiV1OwnedRefTraitFactory {}
impl XabiV1OwnedRefTraitFactory {
    pub const ABI_VERSION: u32 = ABI_VERSION;
    pub const MIN_SIZE: usize = std::mem::size_of::<Self>();
    pub fn validate(&self) -> ::xabi::Result<()> {
        ::xabi::validate_size(
            self.size,
            Self::MIN_SIZE,
            stringify!(XabiV1OwnedRefTraitFactory),
        )?;
        ::xabi::validate_abi_version(
            self.abi_version,
            Self::ABI_VERSION,
            stringify!(XabiV1OwnedRefTraitFactory),
        )?;
        if self.vtable.is_null() {
            return Err(
                ::xabi::Error::NullPointer(
                    concat!(stringify!(XabiV1OwnedRefTraitFactory), "::vtable"),
                ),
            );
        }
        Ok(())
    }
}
impl ::xabi::XabiType for XabiV1OwnedRefTraitFactory {
    type Wire = XabiV1OwnedRefTraitFactory;
    fn into_wire(self) -> Self::Wire {
        self
    }
    unsafe fn from_wire(wire: *const Self::Wire) -> ::xabi::Result<Self> {
        let wire = unsafe {
            wire.as_ref()
                .ok_or(
                    ::xabi::Error::NullPointer(
                        concat!(stringify!(XabiV1OwnedRefTraitFactory), " pointer"),
                    ),
                )?
        };
        wire.validate()?;
        Ok(*wire)
    }
}
pub struct XabiV1OwnedTraitFactory {
    vtable: std::ptr::NonNull<XabiV1VtableTraitFactory>,
}
unsafe impl Send for XabiV1OwnedTraitFactory {}
unsafe impl Sync for XabiV1OwnedTraitFactory {}
impl XabiV1OwnedTraitFactory {
    pub fn new<P: Factory>(value: P) -> Self {
        let vtable = XabiV1AbiTraitFactory::xabi_export(value);
        let vtable = std::ptr::NonNull::new(vtable)
            .expect("generated xabi export returned a null vtable");
        Self { vtable }
    }
    pub unsafe fn xabi_from_vtable(
        vtable: *mut XabiV1VtableTraitFactory,
    ) -> ::xabi::Result<Self> {
        let vtable = std::ptr::NonNull::new(vtable)
            .ok_or(
                ::xabi::Error::NullPointer(
                    concat!(stringify!(XabiV1VtableTraitFactory), " pointer"),
                ),
            )?;
        unsafe { vtable.as_ref() }.validate()?;
        Ok(Self { vtable })
    }
    pub fn xabi_as_ptr(&self) -> *const XabiV1VtableTraitFactory {
        self.vtable.as_ptr()
    }
    pub fn xabi_borrow(&self) -> XabiV1BorrowedTraitFactory {
        XabiV1BorrowedTraitFactory {
            vtable: self.vtable,
        }
    }
}
impl Drop for XabiV1OwnedTraitFactory {
    fn drop(&mut self) {
        unsafe {
            (self.vtable.as_ref().release)(self.vtable.as_ptr());
        }
    }
}
impl std::fmt::Debug for XabiV1HandleTraitFactory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(XabiV1HandleTraitFactory))
            .field("abi_id", &FACTORY_TRAIT_ID)
            .finish_non_exhaustive()
    }
}
impl Drop for XabiV1HandleTraitFactory {
    fn drop(&mut self) {
        unsafe {
            (self.vtable().release)(self.vtable.as_ptr());
        }
    }
}
impl<P> ::xabi::XabiContract<P> for XabiV1AbiTraitFactory
where
    P: Factory,
{
    const ID: &'static str = FACTORY_TRAIT_ID;
    fn export(plugin: P) -> *mut std::ffi::c_void {
        let instance = Box::into_raw(Box::new(plugin)) as *mut std::ffi::c_void;
        let vtable = XabiV1VtableTraitFactory {
            size: std::mem::size_of::<XabiV1VtableTraitFactory>(),
            abi_version: ABI_VERSION,
            capabilities: ::xabi::CAP_NONE,
            instance,
            destroy: XabiV1AbiTraitFactory::__xabi_destroy::<P>,
            release: XabiV1AbiTraitFactory::__xabi_release,
            make: XabiV1AbiTraitFactory::make::<P>,
        };
        Box::into_raw(Box::new(vtable)) as *mut std::ffi::c_void
    }
}
