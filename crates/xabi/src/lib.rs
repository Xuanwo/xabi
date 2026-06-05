use std::error::Error as StdError;
use std::ffi::c_void;
use std::fmt;
use std::marker::PhantomData;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::ptr::NonNull;
use std::slice;
use std::sync::Arc;

pub const ABI_VERSION: u32 = 1;

pub const OK: i32 = 0;
pub const ERR_PANIC: i32 = -1;
pub const ERR_PLUGIN: i32 = -2;
pub const ERR_HOST: i32 = -3;
pub const ERR_INVALID_ARGUMENT: i32 = -4;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiStr {
    pub ptr: *const u8,
    pub len: usize,
}

unsafe impl Send for FfiStr {}
unsafe impl Sync for FfiStr {}

impl FfiStr {
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    pub const fn from_static(value: &'static str) -> Self {
        Self {
            ptr: value.as_ptr(),
            len: value.len(),
        }
    }

    pub fn from_borrowed(value: &str) -> Self {
        Self {
            ptr: value.as_ptr(),
            len: value.len(),
        }
    }

    /// # Safety
    ///
    /// `ptr` must be valid for reads of `len` bytes for the returned borrow's lifetime, and the
    /// bytes must be valid UTF-8.
    pub unsafe fn as_str(&self) -> Result<&str> {
        let bytes = self.as_bytes()?;
        std::str::from_utf8(bytes).map_err(|err| Error::InvalidUtf8(err.to_string()))
    }

    /// # Safety
    ///
    /// `ptr` must be valid for reads of `len` bytes for the returned borrow's lifetime.
    pub unsafe fn as_bytes(&self) -> Result<&[u8]> {
        if self.len == 0 {
            return Ok(&[]);
        }
        let ptr = NonNull::new(self.ptr as *mut u8).ok_or(Error::NullPointer("FfiStr::ptr"))?;
        Ok(slice::from_raw_parts(ptr.as_ptr(), self.len))
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiSlice<T> {
    pub ptr: *const T,
    pub len: usize,
}

unsafe impl<T: Send> Send for FfiSlice<T> {}
unsafe impl<T: Sync> Sync for FfiSlice<T> {}

impl<T> FfiSlice<T> {
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    pub fn from_slice(value: &[T]) -> Self {
        Self {
            ptr: value.as_ptr(),
            len: value.len(),
        }
    }

    /// # Safety
    ///
    /// `ptr` must be valid for reads of `len * size_of::<T>()` bytes for the returned borrow's
    /// lifetime.
    pub unsafe fn as_slice(&self) -> Result<&[T]> {
        if self.len == 0 {
            return Ok(&[]);
        }
        let ptr = NonNull::new(self.ptr as *mut T).ok_or(Error::NullPointer("FfiSlice::ptr"))?;
        Ok(slice::from_raw_parts(ptr.as_ptr(), self.len))
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiBytes(pub FfiSlice<u8>);

unsafe impl Send for FfiBytes {}
unsafe impl Sync for FfiBytes {}

impl FfiBytes {
    pub const fn empty() -> Self {
        Self(FfiSlice::empty())
    }

    pub fn from_slice(value: &[u8]) -> Self {
        Self(FfiSlice::from_slice(value))
    }

    /// # Safety
    ///
    /// The wrapped pointer must be valid for reads of `len` bytes for the returned borrow's
    /// lifetime.
    pub unsafe fn as_slice(&self) -> Result<&[u8]> {
        self.0.as_slice()
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiOwned {
    pub ptr: *mut u8,
    pub len: usize,
    pub free: unsafe extern "C" fn(*mut u8, usize),
}

impl FfiOwned {
    pub fn empty() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            len: 0,
            free: free_owned_bytes,
        }
    }

    pub fn from_vec(value: Vec<u8>) -> Self {
        if value.is_empty() {
            return Self::empty();
        }

        let boxed = value.into_boxed_slice();
        let len = boxed.len();
        let ptr = Box::into_raw(boxed) as *mut u8;

        Self {
            ptr,
            len,
            free: free_owned_bytes,
        }
    }

    pub fn from_string(value: String) -> Self {
        Self::from_vec(value.into_bytes())
    }

    /// # Safety
    ///
    /// `ptr`, `len`, and `free` must come from the producer of this value. This consumes the
    /// payload and must be called at most once for a given `FfiOwned`.
    pub unsafe fn to_vec_and_free(self) -> Result<Vec<u8>> {
        let value = self.to_vec()?;
        (self.free)(self.ptr, self.len);
        Ok(value)
    }

    /// # Safety
    ///
    /// Same requirements as [`FfiOwned::to_vec_and_free`], and the payload must contain UTF-8.
    pub unsafe fn to_string_and_free(self) -> Result<String> {
        String::from_utf8(self.to_vec_and_free()?)
            .map_err(|err| Error::InvalidUtf8(err.to_string()))
    }

    /// # Safety
    ///
    /// `ptr` must be valid for reads of `len` bytes. This copies the payload and does not call
    /// `free`.
    pub unsafe fn to_vec(&self) -> Result<Vec<u8>> {
        if self.len == 0 {
            return Ok(Vec::new());
        }
        let ptr = NonNull::new(self.ptr).ok_or(Error::NullPointer("FfiOwned::ptr"))?;
        Ok(slice::from_raw_parts(ptr.as_ptr(), self.len).to_vec())
    }
}

unsafe extern "C" fn free_owned_bytes(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let ptr = std::ptr::slice_from_raw_parts_mut(ptr, len);
    drop(Box::from_raw(ptr));
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiResult {
    pub code: i32,
    pub payload: FfiOwned,
}

impl FfiResult {
    pub fn ok(payload: FfiOwned) -> Self {
        Self { code: OK, payload }
    }

    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            payload: FfiOwned::from_string(message.into()),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginEntry {
    pub trait_id: FfiStr,
    pub name: FfiStr,
    pub impl_version: u32,
    pub make: unsafe extern "C" fn() -> *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PluginManifest {
    pub size: usize,
    pub abi_version: u32,
    pub entries: FfiSlice<PluginEntry>,
}

impl PluginManifest {
    pub const fn new(entries: &'static [PluginEntry]) -> Self {
        Self {
            size: std::mem::size_of::<PluginManifest>(),
            abi_version: ABI_VERSION,
            entries: FfiSlice {
                ptr: entries.as_ptr(),
                len: entries.len(),
            },
        }
    }

    pub fn validate(&self) -> Result<()> {
        validate_manifest(self)
    }
}

pub struct LibraryHandle {
    _library: libloading::Library,
}

impl LibraryHandle {
    /// # Safety
    ///
    /// Loading arbitrary native code is unsafe. The caller must trust the library at `path` and
    /// ensure loaded symbols are later used with their exact ABI signatures.
    pub unsafe fn open(path: impl AsRef<Path>) -> Result<Arc<Self>> {
        let library = libloading::Library::new(path.as_ref())
            .map_err(|err| Error::LoadLibrary(err.to_string()))?;
        Ok(Arc::new(Self { _library: library }))
    }

    /// # Safety
    ///
    /// `T` must exactly match the symbol's real type and calling convention.
    pub unsafe fn get<T>(&self, symbol: &[u8]) -> Result<libloading::Symbol<'_, T>> {
        self._library.get(symbol).map_err(|err| {
            Error::LoadSymbol(
                String::from_utf8_lossy(symbol).into_owned(),
                err.to_string(),
            )
        })
    }
}

pub struct LoadedLibrary {
    handle: Arc<LibraryHandle>,
    manifest: NonNull<PluginManifest>,
}

unsafe impl Send for LoadedLibrary {}
unsafe impl Sync for LoadedLibrary {}

impl LoadedLibrary {
    /// # Safety
    ///
    /// The dynamic library must export `xabi_manifest` with the expected `extern "C"` signature
    /// and must follow the xabi manifest, lifetime, and ownership contracts.
    pub unsafe fn open(path: impl AsRef<Path>) -> Result<Self> {
        let handle = LibraryHandle::open(path)?;
        let manifest = {
            let symbol: libloading::Symbol<'_, unsafe extern "C" fn() -> *const PluginManifest> =
                handle.get(b"xabi_manifest")?;
            symbol()
        };
        let manifest = NonNull::new(manifest as *mut PluginManifest)
            .ok_or(Error::NullPointer("PluginManifest"))?;
        manifest.as_ref().validate()?;
        Ok(Self { handle, manifest })
    }

    pub fn handle(&self) -> Arc<LibraryHandle> {
        Arc::clone(&self.handle)
    }

    pub fn entries(&self) -> Result<&[PluginEntry]> {
        unsafe { self.manifest.as_ref().entries.as_slice() }
    }
}

pub fn catch_unwind_code(f: impl FnOnce() -> i32) -> i32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(code) => code,
        Err(_) => ERR_PANIC,
    }
}

pub fn catch_unwind_owned(f: impl FnOnce() -> FfiOwned) -> FfiOwned {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(_) => FfiOwned::from_string("panic crossing xabi boundary".to_string()),
    }
}

pub fn validate_size(actual: usize, expected: usize, name: &'static str) -> Result<()> {
    if actual < expected {
        return Err(Error::AbiMismatch(format!(
            "{name} size {actual} is smaller than expected {expected}"
        )));
    }
    Ok(())
}

pub fn validate_abi_version(actual: u32, expected: u32, name: &'static str) -> Result<()> {
    if actual != expected {
        return Err(Error::AbiMismatch(format!(
            "{name} abi_version {actual} does not match expected {expected}"
        )));
    }
    Ok(())
}

pub fn status_to_result(code: i32, context: &str) -> Result<()> {
    match code {
        OK => Ok(()),
        ERR_PANIC => Err(Error::Plugin(format!(
            "{context}: panic crossed xabi boundary"
        ))),
        ERR_PLUGIN => Err(Error::Plugin(format!(
            "{context}: plugin returned an error"
        ))),
        ERR_HOST => Err(Error::Plugin(format!(
            "{context}: host callback returned an error"
        ))),
        ERR_INVALID_ARGUMENT => Err(Error::Plugin(format!("{context}: invalid argument"))),
        other => Err(Error::Plugin(format!(
            "{context}: unknown xabi code {other}"
        ))),
    }
}

#[macro_export]
macro_rules! __xabi_raw_vtable {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            abi_version = $abi_version:expr;
            $(@min_size($min_size:expr);)?
            $($field:ident: $field_ty:ty),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[repr(C)]
        $vis struct $name {
            pub size: usize,
            pub abi_version: u32,
            pub capabilities: u64,
            pub instance: *mut std::ffi::c_void,
            $(pub $field: $field_ty,)+
        }

        impl $name {
            pub const ABI_VERSION: u32 = $abi_version;
            pub const MIN_SIZE: usize = $crate::__xabi_select_min_size!(
                std::mem::size_of::<Self>()
                $(, $min_size)?
            );

            pub fn validate(&self) -> $crate::Result<()> {
                $crate::validate_size(
                    self.size,
                    Self::MIN_SIZE,
                    stringify!($name),
                )?;
                $crate::validate_abi_version(
                    self.abi_version,
                    Self::ABI_VERSION,
                    stringify!($name),
                )?;
                Ok(())
            }
        }
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_select_min_size {
    ($default:expr) => {
        $default
    };
    ($default:expr, $min_size:expr) => {
        $min_size
    };
}

#[macro_export]
macro_rules! __xabi_raw_field_available {
    ($vtable:expr, $vtable_ty:ty, $field:tt) => {{
        let size = ($vtable).size;
        let field_end =
            std::mem::offset_of!($vtable_ty, $field) + std::mem::size_of_val(&($vtable).$field);
        size >= field_end
    }};
}

#[macro_export]
macro_rules! __xabi_raw_manifest {
    (
        entries: [
            $(
                {
                    trait_id: $trait_id:expr,
                    name: $name:expr,
                    impl_version: $impl_version:expr,
                    make: $make:expr $(,)?
                }
            ),+ $(,)?
        ]
    ) => {
        #[no_mangle]
        pub extern "C" fn xabi_manifest() -> *const $crate::PluginManifest {
            &XABI_MANIFEST
        }

        static XABI_ENTRIES: [$crate::PluginEntry; $crate::__xabi_count_exprs!($($trait_id),+)] = [
            $(
                $crate::PluginEntry {
                    trait_id: $crate::FfiStr::from_static($trait_id),
                    name: $crate::FfiStr::from_static($name),
                    impl_version: $impl_version,
                    make: $make,
                },
            )+
        ];

        static XABI_MANIFEST: $crate::PluginManifest =
            $crate::PluginManifest::new(&XABI_ENTRIES);
    };
}

#[macro_export]
macro_rules! __xabi_raw_ffi_code {
    (
        $(#[$meta:meta])*
        $vis:vis unsafe extern "C" fn $name:ident(
            $($arg:ident: $arg_ty:ty),* $(,)?
        ) -> i32 $body:block
    ) => {
        $(#[$meta])*
        $vis unsafe extern "C" fn $name($($arg: $arg_ty),*) -> i32 {
            $crate::catch_unwind_code(|| $body)
        }
    };
}

#[macro_export]
macro_rules! __xabi_raw_ffi_owned {
    (
        $(#[$meta:meta])*
        $vis:vis unsafe extern "C" fn $name:ident(
            $($arg:ident: $arg_ty:ty),* $(,)?
        ) -> $ret:ty $body:block
    ) => {
        $(#[$meta])*
        $vis unsafe extern "C" fn $name($($arg: $arg_ty),*) -> $ret {
            $crate::catch_unwind_owned(|| $body)
        }
    };
}

#[macro_export]
macro_rules! __xabi_raw_ffi_void {
    (
        $(#[$meta:meta])*
        $vis:vis unsafe extern "C" fn $name:ident(
            $($arg:ident: $arg_ty:ty),* $(,)?
        ) $body:block
    ) => {
        $(#[$meta])*
        $vis unsafe extern "C" fn $name($($arg: $arg_ty),*) {
            let _ = $crate::catch_unwind_code(|| {
                $body
                $crate::OK
            });
        }
    };
}

#[macro_export]
macro_rules! __xabi_raw_foreign_handle {
    (
        $vis:vis struct $name:ident for $vtable:ty {
            error = $error:ty;
        }
    ) => {
        $vis struct $name {
            vtable: std::ptr::NonNull<$vtable>,
            _library: std::sync::Arc<$crate::LibraryHandle>,
        }

        unsafe impl Send for $name {}
        unsafe impl Sync for $name {}

        impl $name {
            /// # Safety
            ///
            /// `vtable` must be a valid owned vtable produced by the plugin, and `library` must
            /// keep the code backing all function pointers loaded.
            pub unsafe fn from_vtable(
                vtable: *mut $vtable,
                library: std::sync::Arc<$crate::LibraryHandle>,
            ) -> std::result::Result<Self, $error> {
                let vtable = std::ptr::NonNull::new(vtable)
                    .ok_or_else(|| <$error>::new(concat!(stringify!($vtable), " pointer is null")))?;
                vtable.as_ref().validate().map_err(<$error>::from)?;
                Ok(Self {
                    vtable,
                    _library: library,
                })
            }

            fn vtable(&self) -> &$vtable {
                unsafe { self.vtable.as_ref() }
            }
        }

        impl Drop for $name {
            fn drop(&mut self) {
                unsafe {
                    (self.vtable().release)(self.vtable.as_ptr());
                }
            }
        }
    };
}

#[macro_export]
macro_rules! __xabi_raw_foreign_plugin_handle {
    (
        $vis:vis struct $name:ident for $vtable:ty {
            error = $error:ty;
            trait_id = $trait_id:expr;
        }
    ) => {
        $crate::__xabi_raw_foreign_handle! {
            $vis struct $name for $vtable {
                error = $error;
            }
        }

        impl $name {
            /// # Safety
            ///
            /// `entry.make` must return a valid owned vtable that follows this trait ABI, and
            /// `library` must keep the code backing all function pointers loaded.
            pub unsafe fn from_entry(
                entry: &$crate::PluginEntry,
                library: std::sync::Arc<$crate::LibraryHandle>,
            ) -> std::result::Result<Self, $error> {
                let trait_id = entry.trait_id.as_str().map_err(<$error>::from)?;
                if trait_id != $trait_id {
                    return Err(<$error>::new(format!(
                        "plugin entry has trait_id {trait_id}, expected {}",
                        $trait_id
                    )));
                }

                let raw = (entry.make)() as *mut $vtable;
                Self::from_vtable(raw, library)
            }
        }
    };
}

pub mod raw {
    pub use crate::__xabi_raw_ffi_code as ffi_code;
    pub use crate::__xabi_raw_ffi_owned as ffi_owned;
    pub use crate::__xabi_raw_ffi_void as ffi_void;
    pub use crate::__xabi_raw_field_available as field_available;
    pub use crate::__xabi_raw_foreign_handle as foreign_handle;
    pub use crate::__xabi_raw_foreign_plugin_handle as foreign_plugin_handle;
    pub use crate::__xabi_raw_manifest as manifest;
    pub use crate::__xabi_raw_vtable as vtable;
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_count_exprs {
    ($($value:expr),* $(,)?) => {
        <[()]>::len(&[$($crate::__xabi_replace_expr!(($value) ())),*])
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __xabi_replace_expr {
    (($value:expr) $replacement:expr) => {
        $replacement
    };
}

fn validate_manifest(manifest: &PluginManifest) -> Result<()> {
    validate_size(
        manifest.size,
        std::mem::size_of::<PluginManifest>(),
        "PluginManifest",
    )?;
    validate_abi_version(manifest.abi_version, ABI_VERSION, "PluginManifest")?;
    if manifest.entries.len > 0 && manifest.entries.ptr.is_null() {
        return Err(Error::NullPointer("PluginManifest::entries"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_accepts_current_layout() {
        static ENTRIES: [PluginEntry; 0] = [];
        let manifest = PluginManifest::new(&ENTRIES);

        manifest.validate().expect("current manifest is valid");
    }

    #[test]
    fn manifest_rejects_short_layout() {
        let manifest = PluginManifest {
            size: std::mem::size_of::<PluginManifest>() - 1,
            abi_version: ABI_VERSION,
            entries: FfiSlice::empty(),
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn manifest_rejects_wrong_abi_version() {
        let manifest = PluginManifest {
            size: std::mem::size_of::<PluginManifest>(),
            abi_version: ABI_VERSION + 1,
            entries: FfiSlice::empty(),
        };

        assert!(manifest.validate().is_err());
    }

    #[test]
    fn manifest_rejects_non_empty_null_entries() {
        let manifest = PluginManifest {
            size: std::mem::size_of::<PluginManifest>(),
            abi_version: ABI_VERSION,
            entries: FfiSlice {
                ptr: std::ptr::null(),
                len: 1,
            },
        };

        assert!(manifest.validate().is_err());
    }
}

#[derive(Debug)]
pub enum Error {
    AbiMismatch(String),
    InvalidUtf8(String),
    LoadLibrary(String),
    LoadSymbol(String, String),
    NullPointer(&'static str),
    Plugin(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AbiMismatch(message) => f.write_str(message),
            Error::InvalidUtf8(message) => write!(f, "invalid UTF-8: {message}"),
            Error::LoadLibrary(message) => write!(f, "failed to load library: {message}"),
            Error::LoadSymbol(symbol, message) => {
                write!(f, "failed to load symbol {symbol}: {message}")
            }
            Error::NullPointer(name) => write!(f, "null pointer: {name}"),
            Error::Plugin(message) => f.write_str(message),
        }
    }
}

impl StdError for Error {}

pub type Result<T> = std::result::Result<T, Error>;

pub struct SendPtr<T> {
    value: usize,
    _marker: PhantomData<*mut T>,
}

impl<T> SendPtr<T> {
    pub fn new(ptr: *mut T) -> Self {
        Self {
            value: ptr as usize,
            _marker: PhantomData,
        }
    }

    pub fn as_ptr(self) -> *mut T {
        self.value as *mut T
    }
}

unsafe impl<T> Send for SendPtr<T> {}
