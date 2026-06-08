#[allow(async_fn_in_trait)]
pub trait DemoPlugin: Send + Sync + 'static {
    fn name(&self) -> String;
    fn build(
        &self,
        input: BuildInput,
    ) -> impl std::future::Future<Output = Result<Vec<u8>>> + Send;
    fn load(
        &self,
        details: &[u8],
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    #[doc(hidden)]
    const __XABI_ID: &'static str = TRAIT_ID;
    #[doc(hidden)]
    fn __xabi_export(value: Self) -> *mut std::ffi::c_void
    where
        Self: Sized,
    {
        <XabiV1AbiTraitDemoPlugin as ::xabi::XabiContract<Self>>::export(value)
    }
}
pub struct XabiV1AbiTraitDemoPlugin;
impl XabiV1AbiTraitDemoPlugin {
    pub const ID: &'static str = TRAIT_ID;
    pub const VERSION: u32 = ABI_VERSION;
    unsafe extern "C" fn name<P: DemoPlugin>(
        instance: *mut std::ffi::c_void,
    ) -> ::xabi::XabiOwnedBytes {
        ::xabi::catch_unwind_owned(|| {
            let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                return ::xabi::XabiOwnedBytes::empty();
            };
            ::xabi::XabiOwnedBytes::from_string(plugin.name())
        })
    }
    unsafe extern "C" fn build<P: DemoPlugin>(
        instance: *mut std::ffi::c_void,
        input: *const <BuildInput as ::xabi::XabiType>::Wire,
        out: *mut ::xabi::XabiFuture,
    ) -> i32 {
        ::xabi::catch_unwind_code(|| {
            let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
            let Some(out) = (unsafe { out.as_mut() }) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
            let Ok(input) = (unsafe {
                <BuildInput as ::xabi::XabiType>::from_wire(input)
            }) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
            let future = async move { plugin.build(input).await };
            *out = ::xabi::XabiFuture::from_result_bytes(future);
            ::xabi::OK
        })
    }
    unsafe extern "C" fn load<P: DemoPlugin>(
        instance: *mut std::ffi::c_void,
        details: ::xabi::XabiBytes,
        out: *mut ::xabi::XabiFuture,
    ) -> i32 {
        ::xabi::catch_unwind_code(|| {
            let Some(plugin) = Self::__xabi_impl_ref::<P>(instance) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
            let Some(out) = (unsafe { out.as_mut() }) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
            let Ok(details) = (unsafe { details.as_slice() }) else {
                return ::xabi::ERR_INVALID_ARGUMENT;
            };
            let details = details.to_vec();
            let future = async move { plugin.load(&details).await.map(|()| Vec::new()) };
            *out = ::xabi::XabiFuture::from_result_bytes(future);
            ::xabi::OK
        })
    }
    unsafe extern "C" fn __xabi_destroy<P: DemoPlugin>(instance: *mut std::ffi::c_void) {
        if !instance.is_null() {
            drop(unsafe { Box::from_raw(instance as *mut P) });
        }
    }
    unsafe extern "C" fn __xabi_release(vtable: *mut XabiV1VtableTraitDemoPlugin) {
        let Some(vtable) = (unsafe { vtable.as_mut() }) else {
            return;
        };
        unsafe { (vtable.destroy)(vtable.instance) };
        drop(unsafe { Box::from_raw(vtable) });
    }
    fn __xabi_impl_ref<P: DemoPlugin>(
        instance: *mut std::ffi::c_void,
    ) -> Option<&'static P> {
        unsafe { (instance as *const P).as_ref() }
    }
}
#[repr(C)]
pub struct XabiV1VtableTraitDemoPlugin {
    pub size: usize,
    pub abi_version: u32,
    pub capabilities: u64,
    pub instance: *mut std::ffi::c_void,
    pub name: unsafe extern "C" fn(*mut std::ffi::c_void) -> ::xabi::XabiOwnedBytes,
    pub build: unsafe extern "C" fn(
        *mut std::ffi::c_void,
        *const <BuildInput as ::xabi::XabiType>::Wire,
        *mut ::xabi::XabiFuture,
    ) -> i32,
    pub load: unsafe extern "C" fn(
        *mut std::ffi::c_void,
        ::xabi::XabiBytes,
        *mut ::xabi::XabiFuture,
    ) -> i32,
    pub destroy: unsafe extern "C" fn(*mut std::ffi::c_void),
    pub release: unsafe extern "C" fn(*mut XabiV1VtableTraitDemoPlugin),
}
impl XabiV1VtableTraitDemoPlugin {
    pub const ABI_VERSION: u32 = ABI_VERSION;
    pub const MIN_SIZE: usize = std::mem::size_of::<Self>();
    pub fn validate(&self) -> ::xabi::Result<()> {
        ::xabi::validate_size(
            self.size,
            Self::MIN_SIZE,
            stringify!(XabiV1VtableTraitDemoPlugin),
        )?;
        ::xabi::validate_abi_version(
            self.abi_version,
            Self::ABI_VERSION,
            stringify!(XabiV1VtableTraitDemoPlugin),
        )?;
        Ok(())
    }
    pub fn field_available(&self, field: &str) -> bool {
        match field {
            stringify!(name) => {
                let field_end = std::mem::offset_of!(XabiV1VtableTraitDemoPlugin, name)
                    + std::mem::size_of_val(&self.name);
                self.size >= field_end
            }
            stringify!(build) => {
                let field_end = std::mem::offset_of!(XabiV1VtableTraitDemoPlugin, build)
                    + std::mem::size_of_val(&self.build);
                self.size >= field_end
            }
            stringify!(load) => {
                let field_end = std::mem::offset_of!(XabiV1VtableTraitDemoPlugin, load)
                    + std::mem::size_of_val(&self.load);
                self.size >= field_end
            }
            "destroy" => {
                let field_end = std::mem::offset_of!(
                    XabiV1VtableTraitDemoPlugin, destroy
                ) + std::mem::size_of_val(&self.destroy);
                self.size >= field_end
            }
            "release" => {
                let field_end = std::mem::offset_of!(
                    XabiV1VtableTraitDemoPlugin, release
                ) + std::mem::size_of_val(&self.release);
                self.size >= field_end
            }
            _ => false,
        }
    }
}
pub struct XabiV1HandleTraitDemoPlugin {
    vtable: std::ptr::NonNull<XabiV1VtableTraitDemoPlugin>,
    _module: std::sync::Arc<::xabi::ModuleHandle>,
}
unsafe impl Send for XabiV1HandleTraitDemoPlugin {}
unsafe impl Sync for XabiV1HandleTraitDemoPlugin {}
impl XabiV1HandleTraitDemoPlugin {
    pub unsafe fn xabi_from_vtable(
        vtable: *mut XabiV1VtableTraitDemoPlugin,
        module: std::sync::Arc<::xabi::ModuleHandle>,
    ) -> ::xabi::Result<Self> {
        let vtable = std::ptr::NonNull::new(vtable)
            .ok_or(
                ::xabi::Error::NullPointer(
                    concat!(stringify!(XabiV1VtableTraitDemoPlugin), " pointer"),
                ),
            )?;
        unsafe { vtable.as_ref() }.validate()?;
        Ok(Self { vtable, _module: module })
    }
    pub unsafe fn xabi_from_export(
        export: &::xabi::XabiExport,
        module: std::sync::Arc<::xabi::ModuleHandle>,
    ) -> ::xabi::Result<Self> {
        let abi_id = unsafe { export.abi_id.as_str() }?;
        if abi_id != TRAIT_ID {
            return Err(
                ::xabi::Error::Export(
                    format!("module export has abi_id {abi_id}, expected {}", TRAIT_ID,),
                ),
            );
        }
        let raw = unsafe { (export.make)() } as *mut XabiV1VtableTraitDemoPlugin;
        unsafe { Self::xabi_from_vtable(raw, module) }
    }
    pub unsafe fn xabi_load(module: &::xabi::Module) -> ::xabi::Result<Self> {
        let handle = module.handle();
        for export in module.exports()? {
            let abi_id = unsafe { export.abi_id.as_str() }?;
            if abi_id == TRAIT_ID {
                return unsafe { Self::xabi_from_export(export, handle) };
            }
        }
        Err(
            ::xabi::Error::Export(
                format!("module does not contain xabi export {}", TRAIT_ID,),
            ),
        )
    }
    fn vtable(&self) -> &XabiV1VtableTraitDemoPlugin {
        unsafe { self.vtable.as_ref() }
    }
    pub fn name(&self) -> ::xabi::Result<String> {
        let out = unsafe { (self.vtable().name)(self.vtable().instance) };
        unsafe { out.to_string_and_free() }
    }
    pub async fn build(
        &self,
        input: BuildInput,
    ) -> std::result::Result<Vec<u8>, ::xabi::XabiCallError<::xabi::Error>> {
        let wire = <BuildInput as ::xabi::XabiType>::into_wire(input);
        let mut future = ::xabi::XabiFuture::empty();
        let code = unsafe {
            (self.vtable().build)(self.vtable().instance, &wire, &mut future)
        };
        ::xabi::status_to_result(code, concat!("Xabi.", stringify!(build)))
            .map_err(::xabi::XabiCallError::Runtime)?;
        ::xabi::XabiTypedFuture::<::xabi::Error>::new(future)
            .map_err(::xabi::XabiCallError::Runtime)?
            .await
    }
    pub async fn load(
        &self,
        details: &[u8],
    ) -> std::result::Result<(), ::xabi::XabiCallError<::xabi::Error>> {
        let mut future = ::xabi::XabiFuture::empty();
        let code = unsafe {
            (self
                .vtable()
                .load)(
                self.vtable().instance,
                ::xabi::XabiBytes::from_slice(details),
                &mut future,
            )
        };
        ::xabi::status_to_result(code, concat!("Xabi.", stringify!(load)))
            .map_err(::xabi::XabiCallError::Runtime)?;
        let bytes = ::xabi::XabiTypedFuture::<::xabi::Error>::new(future)
            .map_err(::xabi::XabiCallError::Runtime)?
            .await?;
        if bytes.is_empty() {
            Ok(())
        } else {
            Err(
                ::xabi::XabiCallError::Runtime(
                    ::xabi::Error::Export(
                        concat!(
                            "Xabi.", stringify!(load),
                            " returned a non-empty unit payload"
                        )
                            .to_string(),
                    ),
                ),
            )
        }
    }
}
impl std::fmt::Debug for XabiV1HandleTraitDemoPlugin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(stringify!(XabiV1HandleTraitDemoPlugin))
            .field("abi_id", &TRAIT_ID)
            .finish_non_exhaustive()
    }
}
impl Drop for XabiV1HandleTraitDemoPlugin {
    fn drop(&mut self) {
        unsafe {
            (self.vtable().release)(self.vtable.as_ptr());
        }
    }
}
impl<P> ::xabi::XabiContract<P> for XabiV1AbiTraitDemoPlugin
where
    P: DemoPlugin,
{
    const ID: &'static str = TRAIT_ID;
    fn export(plugin: P) -> *mut std::ffi::c_void {
        let instance = Box::into_raw(Box::new(plugin)) as *mut std::ffi::c_void;
        let vtable = XabiV1VtableTraitDemoPlugin {
            size: std::mem::size_of::<XabiV1VtableTraitDemoPlugin>(),
            abi_version: ABI_VERSION,
            capabilities: 0,
            instance,
            name: XabiV1AbiTraitDemoPlugin::name::<P>,
            build: XabiV1AbiTraitDemoPlugin::build::<P>,
            load: XabiV1AbiTraitDemoPlugin::load::<P>,
            destroy: XabiV1AbiTraitDemoPlugin::__xabi_destroy::<P>,
            release: XabiV1AbiTraitDemoPlugin::__xabi_release,
        };
        Box::into_raw(Box::new(vtable)) as *mut std::ffi::c_void
    }
}
