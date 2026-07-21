use std::ptr::NonNull;
use std::slice;

use crate::{ABI_VERSION, Error, ModuleHandle, OK, Result, validate_abi_version, validate_size};

/// Borrowed UTF-8 string passed across the ABI boundary.
///
/// The pointer is borrowed. Producers must ensure the backing bytes outlive the
/// call that receives this value.
///
/// ```
/// let value = xabi::XabiStr::from_borrowed("hello");
/// let decoded = unsafe { value.as_str() }.unwrap();
/// assert_eq!(decoded, "hello");
/// ```
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiStr {
    /// Pointer to the first byte.
    pub ptr: *const u8,
    /// Number of bytes.
    pub len: usize,
}

unsafe impl Send for XabiStr {}
unsafe impl Sync for XabiStr {}

impl XabiStr {
    /// Create an empty borrowed string.
    ///
    /// ```
    /// let value = xabi::XabiStr::empty();
    /// assert_eq!(unsafe { value.as_str() }.unwrap(), "");
    /// ```
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    /// Borrow a `'static` Rust string.
    ///
    /// ```
    /// static VALUE: &str = "xabi";
    /// let value = xabi::XabiStr::from_static(VALUE);
    /// assert_eq!(unsafe { value.as_str() }.unwrap(), VALUE);
    /// ```
    pub const fn from_static(value: &'static str) -> Self {
        Self {
            ptr: value.as_ptr(),
            len: value.len(),
        }
    }

    /// Borrow a Rust string for the duration of a single ABI call.
    ///
    /// ```
    /// let input = String::from("borrowed");
    /// let value = xabi::XabiStr::from_borrowed(&input);
    /// assert_eq!(unsafe { value.as_str() }.unwrap(), "borrowed");
    /// ```
    pub fn from_borrowed(value: &str) -> Self {
        Self {
            ptr: value.as_ptr(),
            len: value.len(),
        }
    }

    /// Decode the borrowed bytes as UTF-8.
    ///
    /// # Safety
    ///
    /// `ptr` must be valid for reads of `len` bytes for the returned borrow's lifetime, and the
    /// bytes must be valid UTF-8.
    pub unsafe fn as_str(&self) -> Result<&str> {
        let bytes = unsafe { self.as_bytes() }?;
        std::str::from_utf8(bytes).map_err(|err| Error::InvalidUtf8(err.to_string()))
    }

    /// Borrow the raw bytes.
    ///
    /// # Safety
    ///
    /// `ptr` must be valid for reads of `len` bytes for the returned borrow's lifetime.
    pub unsafe fn as_bytes(&self) -> Result<&[u8]> {
        if self.len == 0 {
            return Ok(&[]);
        }
        let ptr = NonNull::new(self.ptr as *mut u8).ok_or(Error::NullPointer("XabiStr::ptr"))?;
        Ok(unsafe { slice::from_raw_parts(ptr.as_ptr(), self.len) })
    }
}

/// Borrowed typed slice passed across the ABI boundary.
///
/// ```
/// let items = [1_u32, 2, 3];
/// let slice = xabi::XabiSlice::from_slice(&items);
/// assert_eq!(unsafe { slice.as_slice() }.unwrap(), &items);
/// ```
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiSlice<T> {
    /// Pointer to the first item.
    pub ptr: *const T,
    /// Number of items.
    pub len: usize,
}

unsafe impl<T: Send> Send for XabiSlice<T> {}
unsafe impl<T: Sync> Sync for XabiSlice<T> {}

impl<T> XabiSlice<T> {
    /// Create an empty borrowed slice.
    ///
    /// ```
    /// let slice = xabi::XabiSlice::<u8>::empty();
    /// assert!(unsafe { slice.as_slice() }.unwrap().is_empty());
    /// ```
    pub const fn empty() -> Self {
        Self {
            ptr: std::ptr::null(),
            len: 0,
        }
    }

    /// Borrow a Rust slice for the duration of a single ABI call.
    ///
    /// ```
    /// let items = [1_u8, 2, 3];
    /// let slice = xabi::XabiSlice::from_slice(&items);
    /// assert_eq!(unsafe { slice.as_slice() }.unwrap(), &items);
    /// ```
    pub fn from_slice(value: &[T]) -> Self {
        Self {
            ptr: value.as_ptr(),
            len: value.len(),
        }
    }

    /// Borrow the raw slice.
    ///
    /// # Safety
    ///
    /// `ptr` must be valid for reads of `len * size_of::<T>()` bytes for the returned borrow's
    /// lifetime.
    pub unsafe fn as_slice(&self) -> Result<&[T]> {
        if self.len == 0 {
            return Ok(&[]);
        }
        let ptr = NonNull::new(self.ptr as *mut T).ok_or(Error::NullPointer("XabiSlice::ptr"))?;
        Ok(unsafe { slice::from_raw_parts(ptr.as_ptr(), self.len) })
    }
}

/// Borrowed byte slice passed across the ABI boundary.
///
/// ```
/// let bytes = xabi::XabiBytes::from_slice(b"xabi");
/// assert_eq!(unsafe { bytes.as_slice() }.unwrap(), b"xabi");
/// ```
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiBytes(pub XabiSlice<u8>);

unsafe impl Send for XabiBytes {}
unsafe impl Sync for XabiBytes {}

impl XabiBytes {
    /// Create an empty borrowed byte slice.
    ///
    /// ```
    /// let bytes = xabi::XabiBytes::empty();
    /// assert!(unsafe { bytes.as_slice() }.unwrap().is_empty());
    /// ```
    pub const fn empty() -> Self {
        Self(XabiSlice::empty())
    }

    /// Borrow a Rust byte slice for the duration of a single ABI call.
    ///
    /// ```
    /// let bytes = xabi::XabiBytes::from_slice(b"abc");
    /// assert_eq!(unsafe { bytes.as_slice() }.unwrap(), b"abc");
    /// ```
    pub fn from_slice(value: &[u8]) -> Self {
        Self(XabiSlice::from_slice(value))
    }

    /// Borrow the raw bytes.
    ///
    /// # Safety
    ///
    /// The wrapped pointer must be valid for reads of `len` bytes for the returned borrow's
    /// lifetime.
    pub unsafe fn as_slice(&self) -> Result<&[u8]> {
        unsafe { self.0.as_slice() }
    }
}

/// Raw owned-byte descriptor passed across the ABI boundary.
///
/// This wire type is `Copy` so it can be embedded in C-compatible ABI values,
/// but copying it does not duplicate ownership. Generated safe code adopts it
/// into [`XabiOwnedBytesOwner`]. Raw consumers must transfer each descriptor
/// into an owner, or call one of the consuming helpers, at most once.
///
/// ```
/// let owned = xabi::XabiOwnedBytes::from_vec(vec![1, 2, 3]);
/// let bytes = unsafe { owned.to_vec_and_free() }.unwrap();
/// assert_eq!(bytes, vec![1, 2, 3]);
/// ```
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiOwnedBytes {
    /// Pointer to the first owned byte.
    pub ptr: *mut u8,
    /// Number of owned bytes.
    pub len: usize,
    /// Function that frees `ptr` and `len`.
    pub free: unsafe extern "C" fn(*mut u8, usize),
}

/// Safe RAII owner for a producer-owned byte payload.
///
/// The owner validates the raw pointer and length once when it adopts an
/// [`XabiOwnedBytes`] descriptor. It then provides safe, read-only access to the
/// bytes and calls the producer's `free` callback exactly once when dropped.
/// The owner is intentionally neither `Copy` nor `Clone`.
/// Generated module handles also keep the producer module loaded for the
/// owner's lifetime so the callback remains callable.
///
/// Use [`XabiOwnedBytesOwner::into_vec`] when a Rust-owned copy is required.
///
/// ```
/// let bytes = xabi::XabiOwnedBytesOwner::from_vec(vec![1, 2, 3]);
/// assert_eq!(bytes.as_slice(), &[1, 2, 3]);
/// assert_eq!(bytes.into_vec(), vec![1, 2, 3]);
/// ```
pub struct XabiOwnedBytesOwner {
    raw: XabiOwnedBytes,
    module: Option<std::sync::Arc<ModuleHandle>>,
}

// Owned byte payloads may be returned by futures that the xabi async contract
// allows executors to move between threads. Producers must therefore provide
// storage that is safe to read and release from those executor threads.
unsafe impl Send for XabiOwnedBytesOwner {}
unsafe impl Sync for XabiOwnedBytesOwner {}

impl XabiOwnedBytesOwner {
    /// Create an empty owned byte payload.
    pub fn empty() -> Self {
        Self {
            raw: XabiOwnedBytes::empty(),
            module: None,
        }
    }

    /// Move a Rust byte vector into an owned xabi payload.
    pub fn from_vec(value: Vec<u8>) -> Self {
        Self {
            raw: XabiOwnedBytes::from_vec(value),
            module: None,
        }
    }

    /// Adopt a raw producer-owned byte descriptor without copying its payload.
    ///
    /// Validation failures still call the producer's `free` callback exactly
    /// once before returning the error.
    ///
    /// # Safety
    ///
    /// `raw` must be a uniquely owned descriptor produced by a compatible xabi
    /// implementation. Its `free` callback must be valid to call exactly once
    /// with `raw.ptr` and `raw.len`, including when pointer/length validation
    /// fails. If the pointer/length representation passes validation for a
    /// non-empty payload, `raw.ptr` must remain valid for immutable reads of
    /// `raw.len` bytes until the returned owner is dropped. The storage and
    /// callback must be safe to use on any thread to which the owner is sent.
    pub unsafe fn from_raw(raw: XabiOwnedBytes) -> Result<Self> {
        let owner = Self { raw, module: None };
        owner.validate()?;
        Ok(owner)
    }

    /// Borrow the validated payload as a read-only byte slice.
    pub fn as_slice(&self) -> &[u8] {
        if self.raw.len == 0 {
            return &[];
        }

        // SAFETY: `from_raw` validated the non-null pointer and range, and its
        // caller guarantees the storage remains readable for the owner's life.
        unsafe { slice::from_raw_parts(self.raw.ptr, self.raw.len) }
    }

    /// Return the number of bytes in this payload.
    pub fn len(&self) -> usize {
        self.raw.len
    }

    /// Return whether this payload is empty.
    pub fn is_empty(&self) -> bool {
        self.raw.len == 0
    }

    /// Copy the payload into a Rust-owned byte vector.
    ///
    /// The producer allocation is released exactly once after the copy, or
    /// while unwinding if allocation of the destination vector panics.
    pub fn into_vec(self) -> Vec<u8> {
        self.as_slice().to_vec()
    }

    pub(crate) fn into_raw(self) -> XabiOwnedBytes {
        if self.module.is_some() {
            let copied = self.as_slice().to_vec();
            drop(self);
            return XabiOwnedBytes::from_vec(copied);
        }

        let raw = self.raw;
        std::mem::forget(self);
        raw
    }

    pub(crate) fn retain_module(&mut self, module: &std::sync::Arc<ModuleHandle>) {
        if self.module.is_none() {
            self.module = Some(std::sync::Arc::clone(module));
        }
    }

    fn validate(&self) -> Result<()> {
        validate_owned_bytes(&self.raw)
    }
}

impl AsRef<[u8]> for XabiOwnedBytesOwner {
    fn as_ref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl From<Vec<u8>> for XabiOwnedBytesOwner {
    fn from(value: Vec<u8>) -> Self {
        Self::from_vec(value)
    }
}

impl From<XabiOwnedBytesOwner> for Vec<u8> {
    fn from(value: XabiOwnedBytesOwner) -> Self {
        value.into_vec()
    }
}

impl Drop for XabiOwnedBytesOwner {
    fn drop(&mut self) {
        unsafe { (self.raw.free)(self.raw.ptr, self.raw.len) };
    }
}

/// Optional xabi payload.
///
/// `is_some` distinguishes `None` from `Some(T)` even when `T` encodes to an
/// empty payload, such as an empty string or empty byte vector.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiOption {
    /// Size of this structure in bytes.
    pub size: usize,
    /// ABI version for this structure.
    pub abi_version: u32,
    /// `1` when `payload` contains an encoded value, `0` for `None`.
    pub is_some: u8,
    /// Owned encoded payload for the contained value.
    pub payload: XabiOwnedBytes,
}

impl XabiOption {
    /// ABI version expected by this structure.
    pub const ABI_VERSION: u32 = ABI_VERSION;
    /// Minimum required size for the current option representation.
    pub const MIN_SIZE: usize =
        std::mem::offset_of!(XabiOption, payload) + std::mem::size_of::<XabiOwnedBytes>();
    /// Full size of this option representation.
    pub const FULL_SIZE: usize = std::mem::size_of::<Self>();

    /// Create a `None` wire value.
    pub fn none() -> Self {
        Self::new(0, XabiOwnedBytes::empty())
    }

    /// Create a `Some` wire value from an encoded payload.
    pub fn some(payload: XabiOwnedBytes) -> Self {
        Self::new(1, payload)
    }

    fn new(is_some: u8, payload: XabiOwnedBytes) -> Self {
        let mut wire = std::mem::MaybeUninit::<Self>::zeroed();
        unsafe {
            let wire_ptr = wire.as_mut_ptr();
            std::ptr::addr_of_mut!((*wire_ptr).size).write(std::mem::size_of::<Self>());
            std::ptr::addr_of_mut!((*wire_ptr).abi_version).write(Self::ABI_VERSION);
            std::ptr::addr_of_mut!((*wire_ptr).is_some).write(is_some);
            std::ptr::addr_of_mut!((*wire_ptr).payload).write(payload);
            wire.assume_init()
        }
    }

    /// Validate the option layout and discriminant.
    pub fn validate(&self) -> Result<()> {
        validate_size(self.size, Self::MIN_SIZE, "XabiOption")?;
        validate_abi_version(self.abi_version, Self::ABI_VERSION, "XabiOption")?;
        match self.is_some {
            0 => {
                if self.payload.len != 0 {
                    return Err(Error::AbiMismatch(
                        "XabiOption none payload must be empty".to_string(),
                    ));
                }
                Ok(())
            }
            1 => Ok(()),
            other => Err(Error::AbiMismatch(format!(
                "XabiOption discriminant {other} is not 0 or 1"
            ))),
        }
    }
}

impl XabiOwnedBytes {
    /// Create an empty owned payload.
    ///
    /// ```
    /// let owned = xabi::XabiOwnedBytes::empty();
    /// assert!(unsafe { owned.to_vec_and_free() }.unwrap().is_empty());
    /// ```
    pub fn empty() -> Self {
        Self {
            ptr: std::ptr::null_mut(),
            len: 0,
            free: free_owned_bytes,
        }
    }

    /// Move a Rust `Vec<u8>` into an ABI-owned payload.
    ///
    /// ```
    /// let owned = xabi::XabiOwnedBytes::from_vec(vec![42]);
    /// assert_eq!(unsafe { owned.to_vec_and_free() }.unwrap(), vec![42]);
    /// ```
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

    /// Move a Rust `String` into an ABI-owned UTF-8 payload.
    ///
    /// ```
    /// let owned = xabi::XabiOwnedBytes::from_string("hello".to_string());
    /// assert_eq!(unsafe { owned.to_string_and_free() }.unwrap(), "hello");
    /// ```
    pub fn from_string(value: String) -> Self {
        Self::from_vec(value.into_bytes())
    }

    /// Copy the payload, then call the producer-provided free function.
    ///
    /// # Safety
    ///
    /// `ptr`, `len`, and `free` must come from the producer of this value. This consumes the
    /// payload and must be called at most once for a given `XabiOwnedBytes`.
    pub unsafe fn to_vec_and_free(self) -> Result<Vec<u8>> {
        let owner = unsafe { XabiOwnedBytesOwner::from_raw(self) }?;
        Ok(owner.into_vec())
    }

    /// Decode the payload as UTF-8, then call the producer-provided free function.
    ///
    /// # Safety
    ///
    /// Same requirements as [`XabiOwnedBytes::to_vec_and_free`], and the payload must contain UTF-8.
    pub unsafe fn to_string_and_free(self) -> Result<String> {
        String::from_utf8(unsafe { self.to_vec_and_free() }?)
            .map_err(|err| Error::InvalidUtf8(err.to_string()))
    }

    /// Copy the payload without freeing it.
    ///
    /// # Safety
    ///
    /// `ptr` must be valid for reads of `len` bytes. This copies the payload and does not call
    /// `free`.
    pub unsafe fn to_vec(&self) -> Result<Vec<u8>> {
        validate_owned_bytes(self)?;
        if self.len == 0 {
            return Ok(Vec::new());
        }
        Ok(unsafe { slice::from_raw_parts(self.ptr, self.len).to_vec() })
    }
}

fn validate_owned_bytes(value: &XabiOwnedBytes) -> Result<()> {
    if value.len == 0 {
        return Ok(());
    }
    if value.ptr.is_null() {
        return Err(Error::NullPointer("XabiOwnedBytes::ptr"));
    }
    if value.len > isize::MAX as usize {
        return Err(Error::AbiMismatch(format!(
            "XabiOwnedBytes length {} exceeds isize::MAX",
            value.len
        )));
    }
    if (value.ptr as usize).checked_add(value.len).is_none() {
        return Err(Error::AbiMismatch(
            "XabiOwnedBytes pointer range overflows the address space".to_string(),
        ));
    }
    Ok(())
}

unsafe extern "C" fn free_owned_bytes(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    let ptr = std::ptr::slice_from_raw_parts_mut(ptr, len);
    drop(unsafe { Box::from_raw(ptr) });
}

/// Status plus optional owned payload returned by the future poll ABI.
///
/// ```
/// let result = xabi::XabiResult::ok(xabi::XabiOwnedBytes::from_vec(vec![1]));
/// assert_eq!(result.code, xabi::OK);
/// assert_eq!(unsafe { result.payload.to_vec_and_free() }.unwrap(), vec![1]);
/// ```
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiResult {
    /// xabi status code for the completed operation.
    pub code: i32,
    /// Owned success or error payload.
    pub payload: XabiOwnedBytes,
}

impl XabiResult {
    /// Create an empty successful result.
    ///
    /// ```
    /// let result = xabi::XabiResult::empty();
    /// assert_eq!(result.code, xabi::OK);
    /// ```
    pub fn empty() -> Self {
        Self {
            code: OK,
            payload: XabiOwnedBytes::empty(),
        }
    }

    /// Create a successful result with an owned payload.
    ///
    /// ```
    /// let result = xabi::XabiResult::ok(xabi::XabiOwnedBytes::from_vec(vec![9]));
    /// assert_eq!(unsafe { result.payload.to_vec_and_free() }.unwrap(), vec![9]);
    /// ```
    pub fn ok(payload: XabiOwnedBytes) -> Self {
        Self { code: OK, payload }
    }

    /// Create an error result with an UTF-8 error message.
    ///
    /// ```
    /// let result = xabi::XabiResult::error(xabi::ERR_EXPORT, "failed");
    /// assert_eq!(result.code, xabi::ERR_EXPORT);
    /// assert_eq!(unsafe { result.payload.to_string_and_free() }.unwrap(), "failed");
    /// ```
    pub fn error(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            payload: XabiOwnedBytes::from_string(message.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn ffi_str_rejects_null_non_empty_pointer() {
        let value = XabiStr {
            ptr: std::ptr::null(),
            len: 1,
        };

        assert!(unsafe { value.as_bytes() }.is_err());
    }

    #[test]
    fn ffi_str_rejects_invalid_utf8() {
        let bytes = [0xff_u8];
        let value = XabiStr {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        };

        assert!(matches!(
            unsafe { value.as_str() },
            Err(Error::InvalidUtf8(_))
        ));
    }

    #[test]
    fn ffi_slice_rejects_null_non_empty_pointer() {
        let value = XabiSlice::<u8> {
            ptr: std::ptr::null(),
            len: 1,
        };

        assert!(unsafe { value.as_slice() }.is_err());
    }

    #[test]
    fn ffi_owned_rejects_null_non_empty_pointer() {
        let value = XabiOwnedBytes {
            ptr: std::ptr::null_mut(),
            len: 1,
            free: free_owned_bytes,
        };

        assert!(unsafe { value.to_vec() }.is_err());
    }

    #[test]
    fn owned_bytes_owner_decodes_without_copy_and_frees_once() {
        static FREES: AtomicUsize = AtomicUsize::new(0);

        unsafe extern "C" fn free(ptr: *mut u8, len: usize) {
            FREES.fetch_add(1, Ordering::SeqCst);
            let ptr = std::ptr::slice_from_raw_parts_mut(ptr, len);
            drop(unsafe { Box::from_raw(ptr) });
        }

        FREES.store(0, Ordering::SeqCst);
        let bytes = vec![1_u8, 2, 3].into_boxed_slice();
        let len = bytes.len();
        let ptr = Box::into_raw(bytes) as *mut u8;
        let owner = unsafe { XabiOwnedBytesOwner::from_raw(XabiOwnedBytes { ptr, len, free }) }
            .expect("valid descriptor");

        assert_send_sync::<XabiOwnedBytesOwner>();
        assert!(std::mem::needs_drop::<XabiOwnedBytesOwner>());
        assert_eq!(owner.as_slice(), &[1, 2, 3]);
        assert_eq!(owner.as_slice().as_ptr(), ptr);
        assert_eq!(owner.len(), 3);
        assert!(!owner.is_empty());

        let wire = crate::XabiType::into_wire(owner);
        let owner = unsafe {
            <XabiOwnedBytesOwner as crate::XabiType>::from_wire(std::ptr::addr_of!(wire))
        }
        .expect("wire descriptor is adopted");
        assert_eq!(owner.as_slice().as_ptr(), ptr);
        drop(owner);

        assert_eq!(FREES.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn owned_bytes_owner_explicit_vec_conversion_copies_and_frees() {
        static FREES: AtomicUsize = AtomicUsize::new(0);

        unsafe extern "C" fn free(ptr: *mut u8, len: usize) {
            FREES.fetch_add(1, Ordering::SeqCst);
            let ptr = std::ptr::slice_from_raw_parts_mut(ptr, len);
            drop(unsafe { Box::from_raw(ptr) });
        }

        FREES.store(0, Ordering::SeqCst);
        let bytes = vec![4_u8, 5, 6].into_boxed_slice();
        let len = bytes.len();
        let ptr = Box::into_raw(bytes) as *mut u8;
        let owner = unsafe { XabiOwnedBytesOwner::from_raw(XabiOwnedBytes { ptr, len, free }) }
            .expect("valid descriptor");

        let copied = owner.into_vec();

        assert_eq!(copied, vec![4, 5, 6]);
        assert_ne!(copied.as_ptr(), ptr);
        assert_eq!(FREES.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn owned_bytes_owner_defines_empty_and_invalid_payload_behavior() {
        static FREES: AtomicUsize = AtomicUsize::new(0);

        unsafe extern "C" fn count_free(_ptr: *mut u8, _len: usize) {
            FREES.fetch_add(1, Ordering::SeqCst);
        }

        FREES.store(0, Ordering::SeqCst);

        let empty = unsafe {
            XabiOwnedBytesOwner::from_raw(XabiOwnedBytes {
                ptr: std::ptr::null_mut(),
                len: 0,
                free: count_free,
            })
        }
        .expect("null empty descriptor is valid");
        assert!(empty.as_slice().is_empty());
        drop(empty);

        let empty = unsafe {
            XabiOwnedBytesOwner::from_raw(XabiOwnedBytes {
                ptr: NonNull::<u8>::dangling().as_ptr(),
                len: 0,
                free: count_free,
            })
        }
        .expect("non-null empty descriptor is valid");
        assert!(empty.is_empty());
        drop(empty);

        let null_non_empty = unsafe {
            XabiOwnedBytesOwner::from_raw(XabiOwnedBytes {
                ptr: std::ptr::null_mut(),
                len: 1,
                free: count_free,
            })
        };
        assert!(matches!(null_non_empty, Err(Error::NullPointer(_))));

        let oversized = unsafe {
            XabiOwnedBytesOwner::from_raw(XabiOwnedBytes {
                ptr: NonNull::<u8>::dangling().as_ptr(),
                len: isize::MAX as usize + 1,
                free: count_free,
            })
        };
        assert!(matches!(oversized, Err(Error::AbiMismatch(_))));

        assert_eq!(FREES.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn owned_bytes_owner_frees_on_early_return_and_unwind() {
        static FREES: AtomicUsize = AtomicUsize::new(0);
        static BYTES: &[u8] = b"owned";

        unsafe extern "C" fn count_free(_ptr: *mut u8, _len: usize) {
            FREES.fetch_add(1, Ordering::SeqCst);
        }

        fn owner() -> XabiOwnedBytesOwner {
            unsafe {
                XabiOwnedBytesOwner::from_raw(XabiOwnedBytes {
                    ptr: BYTES.as_ptr() as *mut u8,
                    len: BYTES.len(),
                    free: count_free,
                })
            }
            .expect("static descriptor is valid")
        }

        fn return_early() -> Result<()> {
            let _owner = owner();
            Err(Error::Export("return early".to_string()))
        }

        FREES.store(0, Ordering::SeqCst);
        let result = return_early();
        assert!(result.is_err());
        assert_eq!(FREES.load(Ordering::SeqCst), 1);

        let unwind = std::panic::catch_unwind(|| {
            let _owner = owner();
            panic!("test unwind");
        });
        assert!(unwind.is_err());
        assert_eq!(FREES.load(Ordering::SeqCst), 2);
    }
}
