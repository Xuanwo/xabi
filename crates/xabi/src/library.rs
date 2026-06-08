use std::ffi::c_void;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;

use crate::{validate_abi_version, validate_size, Error, Result, XabiSlice, XabiStr, ABI_VERSION};

/// Export descriptor in an xabi module manifest.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiExport {
    /// Stable ABI identifier implemented by this export.
    pub abi_id: XabiStr,
    /// Human-readable export name.
    pub name: XabiStr,
    /// Export version chosen by the module author.
    pub version: u32,
    /// Constructor that returns an ABI-specific vtable pointer.
    pub make: unsafe extern "C" fn() -> *mut c_void,
}

/// Static manifest exported by an xabi module.
///
/// Modules normally create this through [`crate::module`] or [`crate::raw::manifest`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiManifest {
    /// Size of this structure in bytes.
    pub size: usize,
    /// ABI version for this structure.
    pub abi_version: u32,
    /// Exports provided by this module.
    pub exports: XabiSlice<XabiExport>,
}

impl XabiManifest {
    /// Create a manifest from static exports.
    ///
    /// ```
    /// static EXPORTS: [xabi::XabiExport; 0] = [];
    /// let manifest = xabi::XabiManifest::new(&EXPORTS);
    /// manifest.validate().unwrap();
    /// ```
    pub const fn new(exports: &'static [XabiExport]) -> Self {
        Self {
            size: std::mem::size_of::<XabiManifest>(),
            abi_version: ABI_VERSION,
            exports: XabiSlice {
                ptr: exports.as_ptr(),
                len: exports.len(),
            },
        }
    }

    /// Validate manifest layout and export slice pointer.
    ///
    /// ```
    /// let manifest = xabi::XabiManifest {
    ///     size: 0,
    ///     abi_version: xabi::ABI_VERSION,
    ///     exports: xabi::XabiSlice::empty(),
    /// };
    /// assert!(manifest.validate().is_err());
    /// ```
    pub fn validate(&self) -> Result<()> {
        validate_manifest(self)
    }
}

/// Reference-counted handle for a loaded xabi module.
///
/// Keep this handle alive for as long as any function pointer or xabi handle
/// from the library may still be called.
pub struct ModuleHandle {
    pub(crate) library: libloading::Library,
}

impl ModuleHandle {
    /// Load a module library.
    ///
    /// # Safety
    ///
    /// Loading arbitrary native code is unsafe. The caller must trust the module at `path` and
    /// ensure loaded symbols are later used with their exact ABI signatures.
    ///
    /// ```no_run
    /// let handle = unsafe { xabi::ModuleHandle::load("./module.so") }?;
    /// # Ok::<_, xabi::Error>(())
    /// ```
    pub unsafe fn load(path: impl AsRef<Path>) -> Result<Arc<Self>> {
        let library = unsafe { libloading::Library::new(path.as_ref()) }
            .map_err(|err| Error::LoadLibrary(err.to_string()))?;
        Ok(Arc::new(Self { library }))
    }

    /// Load a typed symbol from this dynamic library.
    ///
    /// # Safety
    ///
    /// `T` must exactly match the symbol's real type and calling convention.
    ///
    /// ```no_run
    /// let handle = unsafe { xabi::ModuleHandle::load("./module.so") }?;
    /// let symbol = unsafe {
    ///     handle.get::<unsafe extern "C" fn() -> *const xabi::XabiManifest>(b"xabi_manifest")
    /// }?;
    /// # let _ = symbol;
    /// # Ok::<_, xabi::Error>(())
    /// ```
    pub unsafe fn get<T>(&self, symbol: &[u8]) -> Result<libloading::Symbol<'_, T>> {
        unsafe { self.library.get(symbol) }.map_err(|err| {
            Error::LoadSymbol(
                String::from_utf8_lossy(symbol).into_owned(),
                err.to_string(),
            )
        })
    }
}

/// Loaded xabi module with a validated manifest.
pub struct Module {
    handle: Arc<ModuleHandle>,
    manifest: NonNull<XabiManifest>,
}

unsafe impl Send for Module {}
unsafe impl Sync for Module {}

impl Module {
    /// Load a module and validate its `xabi_manifest` symbol.
    ///
    /// # Safety
    ///
    /// The module must export `xabi_manifest` with the expected `extern "C"` signature
    /// and must follow the xabi manifest, lifetime, and ownership contracts.
    ///
    /// ```no_run
    /// let module = unsafe { xabi::Module::load("./module.so") }?;
    /// for export in module.exports()? {
    ///     println!("{}", unsafe { export.name.as_str() }?);
    /// }
    /// # Ok::<_, xabi::Error>(())
    /// ```
    pub unsafe fn load(path: impl AsRef<Path>) -> Result<Self> {
        let handle = unsafe { ModuleHandle::load(path) }?;
        let manifest = {
            let symbol: libloading::Symbol<'_, unsafe extern "C" fn() -> *const XabiManifest> =
                unsafe { handle.get(b"xabi_manifest") }?;
            unsafe { symbol() }
        };
        let manifest = NonNull::new(manifest as *mut XabiManifest)
            .ok_or(Error::NullPointer("XabiManifest"))?;
        unsafe { manifest.as_ref().validate() }?;
        Ok(Self { handle, manifest })
    }

    /// Return the reference-counted library handle.
    ///
    /// ```
    /// # fn assert_send_sync<T: Send + Sync>() {}
    /// assert_send_sync::<xabi::Module>();
    /// ```
    pub fn handle(&self) -> Arc<ModuleHandle> {
        Arc::clone(&self.handle)
    }

    /// Borrow module exports.
    ///
    /// ```no_run
    /// let module = unsafe { xabi::Module::load("./module.so") }?;
    /// let exports = module.exports()?;
    /// # let _ = exports;
    /// # Ok::<_, xabi::Error>(())
    /// ```
    pub fn exports(&self) -> Result<&[XabiExport]> {
        unsafe { self.manifest.as_ref().exports.as_slice() }
    }
}

/// Load an xabi module from a native library path.
///
/// # Safety
///
/// Loading arbitrary native code is unsafe. The caller must trust the module at
/// `path` and ensure any loaded exports are used with their matching ABI.
///
/// ```no_run
/// let module = unsafe { xabi::load("./module.so") }?;
/// # let _ = module;
/// # Ok::<_, xabi::Error>(())
/// ```
pub unsafe fn load(path: impl AsRef<Path>) -> Result<Module> {
    unsafe { Module::load(path) }
}

fn validate_manifest(manifest: &XabiManifest) -> Result<()> {
    validate_size(
        manifest.size,
        std::mem::size_of::<XabiManifest>(),
        "XabiManifest",
    )?;
    validate_abi_version(manifest.abi_version, ABI_VERSION, "XabiManifest")?;
    if manifest.exports.len > 0 && manifest.exports.ptr.is_null() {
        return Err(Error::NullPointer("XabiManifest::exports"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_accepts_current_layout() {
        static EXPORTS: [XabiExport; 0] = [];
        let manifest = XabiManifest::new(&EXPORTS);

        manifest.validate().expect("current manifest is valid");
    }

    #[test]
    fn manifest_rejects_short_layout() {
        let manifest = XabiManifest {
            size: std::mem::size_of::<XabiManifest>() - 1,
            abi_version: ABI_VERSION,
            exports: XabiSlice::empty(),
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn manifest_rejects_wrong_abi_version() {
        let manifest = XabiManifest {
            size: std::mem::size_of::<XabiManifest>(),
            abi_version: ABI_VERSION + 1,
            exports: XabiSlice::empty(),
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn manifest_rejects_non_empty_null_exports() {
        let manifest = XabiManifest {
            size: std::mem::size_of::<XabiManifest>(),
            abi_version: ABI_VERSION,
            exports: XabiSlice {
                ptr: std::ptr::null(),
                len: 1,
            },
        };

        assert!(manifest.validate().is_err());
    }
}
