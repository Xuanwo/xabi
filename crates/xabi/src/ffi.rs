use std::ptr::NonNull;
use std::slice;

use crate::{ABI_VERSION, Error, OK, Result, validate_abi_version, validate_size};

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

/// Owned byte payload returned across the ABI boundary.
///
/// Consumers must call [`XabiOwnedBytes::to_vec_and_free`] or
/// [`XabiOwnedBytes::to_string_and_free`] at most once to release the producer's
/// allocation.
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
        let value = unsafe { self.to_vec() }?;
        unsafe { (self.free)(self.ptr, self.len) };
        Ok(value)
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
        if self.len == 0 {
            return Ok(Vec::new());
        }
        let ptr = NonNull::new(self.ptr).ok_or(Error::NullPointer("XabiOwnedBytes::ptr"))?;
        Ok(unsafe { slice::from_raw_parts(ptr.as_ptr(), self.len).to_vec() })
    }
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
}
