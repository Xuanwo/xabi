//! Low-level macros for hand-written ABI fixtures.
//!
//! Most users should prefer [`crate::xabi`] and [`crate::module`]. The `raw`
//! module is kept for ABI fixtures, extension crates, and interoperability code
//! that must spell out C ABI structures manually.

/// Define a `#[repr(C)]` vtable with xabi header fields.
///
/// ```
/// xabi::raw::vtable! {
///     pub struct DemoVTable {
///         abi_version = 1;
///         call: unsafe extern "C" fn(*mut std::ffi::c_void) -> i32,
///         release: unsafe extern "C" fn(*mut DemoVTable),
///     }
/// }
///
/// unsafe extern "C" fn call(_instance: *mut std::ffi::c_void) -> i32 {
///     xabi::OK
/// }
///
/// unsafe extern "C" fn release(_vtable: *mut DemoVTable) {}
///
/// let vtable = DemoVTable {
///     size: std::mem::size_of::<DemoVTable>(),
///     abi_version: 1,
///     capabilities: 0,
///     instance: std::ptr::null_mut(),
///     call,
///     release,
/// };
/// vtable.validate().unwrap();
/// ```
pub use crate::__xabi_raw_vtable as vtable;

/// Check whether a vtable field is present in the reported vtable size.
///
/// ```
/// xabi::raw::vtable! {
///     pub struct DemoVTable {
///         abi_version = 1;
///         first: unsafe extern "C" fn(),
///         second: unsafe extern "C" fn(),
///     }
/// }
///
/// unsafe extern "C" fn noop() {}
///
/// let vtable = DemoVTable {
///     size: std::mem::offset_of!(DemoVTable, second),
///     abi_version: 1,
///     capabilities: 0,
///     instance: std::ptr::null_mut(),
///     first: noop,
///     second: noop,
/// };
/// assert!(xabi::raw::field_available!(&vtable, DemoVTable, first));
/// assert!(!xabi::raw::field_available!(&vtable, DemoVTable, second));
/// ```
pub use crate::__xabi_raw_field_available as field_available;

/// Export a static `xabi_manifest` symbol.
///
/// ```no_run
/// unsafe extern "C" fn make() -> *mut std::ffi::c_void {
///     std::ptr::null_mut()
/// }
///
/// xabi::raw::manifest! {
///     exports: [
///         {
///             abi_id: "xabi.example.Raw",
///             name: "raw-demo",
///             version: 1,
///             make: make,
///         },
///     ]
/// }
/// ```
pub use crate::__xabi_raw_manifest as manifest;

/// Wrap an FFI function body with panic-to-status conversion.
///
/// ```
/// xabi::raw::ffi_code! {
///     unsafe extern "C" fn call() -> i32 {
///         xabi::OK
///     }
/// }
///
/// assert_eq!(unsafe { call() }, xabi::OK);
/// ```
pub use crate::__xabi_raw_ffi_code as ffi_code;

/// Wrap an FFI function body with panic-to-owned-payload conversion.
///
/// ```
/// xabi::raw::ffi_owned! {
///     unsafe extern "C" fn name() -> xabi::XabiOwnedBytes {
///         xabi::XabiOwnedBytes::from_string("demo".to_string())
///     }
/// }
///
/// let value = unsafe { name() };
/// assert_eq!(unsafe { value.to_string_and_free() }.unwrap(), "demo");
/// ```
pub use crate::__xabi_raw_ffi_owned as ffi_owned;

/// Wrap a void FFI function body with panic catching.
///
/// ```
/// static CALLED: std::sync::atomic::AtomicBool =
///     std::sync::atomic::AtomicBool::new(false);
///
/// xabi::raw::ffi_void! {
///     unsafe extern "C" fn call() {
///         CALLED.store(true, std::sync::atomic::Ordering::SeqCst);
///     }
/// }
///
/// unsafe { call() };
/// assert!(CALLED.load(std::sync::atomic::Ordering::SeqCst));
/// ```
pub use crate::__xabi_raw_ffi_void as ffi_void;

/// Define a host-side handle around a raw vtable pointer.
///
/// ```no_run
/// #[derive(Debug)]
/// struct Error(String);
///
/// impl Error {
///     fn new(message: impl Into<String>) -> Self {
///         Self(message.into())
///     }
/// }
///
/// impl From<xabi::Error> for Error {
///     fn from(value: xabi::Error) -> Self {
///         Self(value.to_string())
///     }
/// }
///
/// xabi::raw::vtable! {
///     pub struct DemoVTable {
///         abi_version = 1;
///         release: unsafe extern "C" fn(*mut DemoVTable),
///     }
/// }
///
/// xabi::raw::handle! {
///     pub struct DemoHandle for DemoVTable {
///         error = Error;
///     }
/// }
/// ```
pub use crate::__xabi_raw_handle as handle;

/// Define a host-side export handle that validates manifest ABI IDs.
///
/// ```no_run
/// #[derive(Debug)]
/// struct Error(String);
///
/// impl Error {
///     fn new(message: impl Into<String>) -> Self {
///         Self(message.into())
///     }
/// }
///
/// impl From<xabi::Error> for Error {
///     fn from(value: xabi::Error) -> Self {
///         Self(value.to_string())
///     }
/// }
///
/// xabi::raw::vtable! {
///     pub struct DemoVTable {
///         abi_version = 1;
///         release: unsafe extern "C" fn(*mut DemoVTable),
///     }
/// }
///
/// xabi::raw::export_handle! {
///     pub struct DemoHandle for DemoVTable {
///         error = Error;
///         abi_id = "xabi.example.Raw";
///     }
/// }
/// ```
pub use crate::__xabi_raw_export_handle as export_handle;
