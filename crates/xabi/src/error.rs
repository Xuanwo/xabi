use std::error::Error as StdError;
use std::fmt;

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

/// Result type used by xabi runtime helpers.
///
/// ```
/// fn validate() -> xabi::Result<()> {
///     xabi::validate_abi_version(xabi::ABI_VERSION, xabi::ABI_VERSION, "example")
/// }
///
/// validate().unwrap();
/// ```
pub type Result<T> = std::result::Result<T, Error>;

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
