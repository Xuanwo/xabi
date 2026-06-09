use std::error::Error as StdError;
use std::fmt;

use crate::{ABI_VERSION, XabiOwnedBytes, validate_abi_version, validate_size};

/// Wire representation for [`Error`] when it is used as an [`crate::XabiType`].
#[repr(C)]
#[derive(Clone, Copy)]
pub struct XabiErrorWire {
    /// Size of this structure in bytes.
    pub size: usize,
    /// ABI version for this structure.
    pub abi_version: u32,
    /// Numeric error kind.
    pub kind: u32,
}

impl XabiErrorWire {
    /// ABI version expected by this structure.
    pub const ABI_VERSION: u32 = ABI_VERSION;
    /// Minimum required size for the current error wire representation.
    pub const MIN_SIZE: usize =
        std::mem::offset_of!(XabiErrorWire, kind) + std::mem::size_of::<u32>();
    /// Full size of this error wire representation.
    pub const FULL_SIZE: usize = std::mem::size_of::<Self>();

    /// Validate the wire layout.
    pub fn validate(&self) -> Result<()> {
        validate_size(self.size, Self::MIN_SIZE, "XabiErrorWire")?;
        validate_abi_version(self.abi_version, Self::ABI_VERSION, "XabiErrorWire")?;
        Ok(())
    }
}

/// Error type used by xabi runtime helpers.
#[derive(Debug)]
pub enum Error {
    /// An ABI layout or version did not match the expected contract.
    AbiMismatch(String),
    /// A foreign byte buffer was expected to contain UTF-8 but did not.
    InvalidUtf8(String),
    /// A dynamic library could not be loaded.
    LoadLibrary(String),
    /// A symbol could not be loaded from a dynamic library.
    LoadSymbol(String, String),
    /// A required pointer was null.
    NullPointer(&'static str),
    /// An export returned an error payload or invalid status.
    Export(String),
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
            Error::Export(message) => f.write_str(message),
        }
    }
}

impl StdError for Error {}

impl crate::XabiType for Error {
    type Wire = XabiErrorWire;
    const WIRE_TYPE_NAME: &'static str = "XabiErrorWire";

    fn into_wire(self) -> Self::Wire {
        let kind = match self {
            Error::AbiMismatch(_) => 1,
            Error::InvalidUtf8(_) => 2,
            Error::LoadLibrary(_) => 3,
            Error::LoadSymbol(_, _) => 4,
            Error::NullPointer(_) => 5,
            Error::Export(_) => 6,
        };
        XabiErrorWire {
            size: std::mem::size_of::<XabiErrorWire>(),
            abi_version: XabiErrorWire::ABI_VERSION,
            kind,
        }
    }

    unsafe fn from_wire(wire: *const Self::Wire) -> Result<Self> {
        let wire = unsafe {
            wire.as_ref()
                .ok_or(Error::NullPointer("XabiErrorWire pointer"))?
        };
        wire.validate()?;
        Ok(Error::Export(format!("xabi error kind {}", wire.kind)))
    }

    fn into_payload(self) -> XabiOwnedBytes {
        XabiOwnedBytes::from_string(self.to_string())
    }

    unsafe fn from_payload(payload: XabiOwnedBytes) -> Result<Self> {
        let message = unsafe { payload.to_string_and_free() }?;
        Ok(Error::Export(message))
    }
}

/// Error returned by generated host-side xabi handles.
///
/// `Runtime` means the local xabi runtime rejected the call, vtable, or payload.
/// `Export` means the loaded implementation returned its contract error type.
#[derive(Debug)]
pub enum XabiCallError<E> {
    /// The xabi runtime failed before a typed export error could be decoded.
    Runtime(Error),
    /// The implementation returned a typed error payload.
    Export(E),
}

impl<E> From<Error> for XabiCallError<E> {
    fn from(value: Error) -> Self {
        Self::Runtime(value)
    }
}

impl<E: fmt::Display> fmt::Display for XabiCallError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(err) => err.fmt(f),
            Self::Export(err) => err.fmt(f),
        }
    }
}

impl<E> StdError for XabiCallError<E>
where
    E: StdError + 'static,
{
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Runtime(err) => Some(err),
            Self::Export(err) => Some(err),
        }
    }
}

/// Result type used by xabi runtime helpers.
///
/// ```
/// fn validate() -> xabi::Result<()> {
///     xabi::validate_abi_version(xabi::ABI_VERSION, xabi::ABI_VERSION, "example")
/// }
///
/// validate().unwrap();
/// ```
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_load_symbol_includes_symbol_name() {
        let err = Error::LoadSymbol("xabi_manifest".to_string(), "missing".to_string());

        assert_eq!(
            err.to_string(),
            "failed to load symbol xabi_manifest: missing"
        );
    }
}
