use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::{Error, Result, XabiOwnedBytes};

/// Version of xabi's core runtime structures.
pub const ABI_VERSION: u32 = 1;

/// Successful FFI status code.
pub const OK: i32 = 0;
/// The callee caught a panic before it crossed the ABI boundary.
pub const ERR_PANIC: i32 = -1;
/// The export reported an error.
pub const ERR_EXPORT: i32 = -2;
/// A host callback reported an error.
pub const ERR_HOST: i32 = -3;
/// The caller provided an invalid argument.
pub const ERR_INVALID_ARGUMENT: i32 = -4;

/// A future completed and wrote an [`crate::XabiResult`].
pub const POLL_READY: i32 = 0;
/// A future is still pending.
pub const POLL_PENDING: i32 = 1;

/// Empty capability bitset for xabi descriptors.
pub const CAP_NONE: u64 = 0;

/// Convert a panic-catching FFI closure into a status code.
///
/// ```
/// assert_eq!(xabi::catch_unwind_code(|| xabi::OK), xabi::OK);
/// assert_eq!(xabi::catch_unwind_code(|| panic!("boom")), xabi::ERR_PANIC);
/// ```
pub fn catch_unwind_code(f: impl FnOnce() -> i32) -> i32 {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(code) => code,
        Err(_) => ERR_PANIC,
    }
}

/// Convert a panic-catching FFI closure into an owned byte payload.
///
/// ```
/// let owned = xabi::catch_unwind_owned(|| xabi::XabiOwnedBytes::from_string("ok".to_string()));
/// let value = unsafe { owned.to_string_and_free() }.unwrap();
/// assert_eq!(value, "ok");
/// ```
pub fn catch_unwind_owned(f: impl FnOnce() -> XabiOwnedBytes) -> XabiOwnedBytes {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(_) => XabiOwnedBytes::from_string("panic crossing xabi boundary".to_string()),
    }
}

/// Convert a panic-catching closure into a caller-provided default value.
///
/// ```
/// assert_eq!(xabi::catch_unwind_or(7, || 1), 1);
/// assert_eq!(xabi::catch_unwind_or(7, || panic!("boom")), 7);
/// ```
pub fn catch_unwind_or<T>(default: T, f: impl FnOnce() -> T) -> T {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(value) => value,
        Err(_) => default,
    }
}

/// Validate that an ABI structure is at least as large as the required prefix.
///
/// ```
/// xabi::validate_size(16, 8, "Example").unwrap();
/// assert!(xabi::validate_size(4, 8, "Example").is_err());
/// ```
pub fn validate_size(actual: usize, expected: usize, name: &'static str) -> Result<()> {
    if actual < expected {
        return Err(Error::AbiMismatch(format!(
            "{name} size {actual} is smaller than expected {expected}"
        )));
    }
    Ok(())
}

/// Validate that an ABI structure uses the expected version.
///
/// ```
/// xabi::validate_abi_version(1, 1, "Example").unwrap();
/// assert!(xabi::validate_abi_version(2, 1, "Example").is_err());
/// ```
pub fn validate_abi_version(actual: u32, expected: u32, name: &'static str) -> Result<()> {
    if actual != expected {
        return Err(Error::AbiMismatch(format!(
            "{name} abi_version {actual} does not match expected {expected}"
        )));
    }
    Ok(())
}

/// Convert a raw xabi status code into a [`Result`].
///
/// ```
/// xabi::status_to_result(xabi::OK, "Xabi.call").unwrap();
/// assert!(xabi::status_to_result(xabi::ERR_EXPORT, "Xabi.call").is_err());
/// ```
pub fn status_to_result(code: i32, context: &str) -> Result<()> {
    match code {
        OK => Ok(()),
        ERR_PANIC => Err(Error::Export(format!(
            "{context}: panic crossed xabi boundary"
        ))),
        ERR_EXPORT => Err(Error::Export(format!(
            "{context}: export returned an error"
        ))),
        ERR_HOST => Err(Error::Export(format!(
            "{context}: host callback returned an error"
        ))),
        ERR_INVALID_ARGUMENT => Err(Error::Export(format!("{context}: invalid argument"))),
        other => Err(Error::Export(format!(
            "{context}: unknown xabi code {other}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_to_result_reports_context() {
        let err = status_to_result(ERR_INVALID_ARGUMENT, "Xabi.method").unwrap_err();

        assert_eq!(err.to_string(), "Xabi.method: invalid argument");
    }

    #[test]
    fn catch_unwind_owned_returns_error_payload_on_panic() {
        let owned = catch_unwind_owned(|| panic!("boom"));
        let message = unsafe { owned.to_string_and_free() }.unwrap();

        assert_eq!(message, "panic crossing xabi boundary");
    }
}
