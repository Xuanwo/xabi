pub use xabi::{XabiBytes, XabiOwnedBytes, XabiSlice};

pub fn borrowed(value: &[u8]) -> XabiBytes {
    XabiBytes::from_slice(value)
}

pub fn owned(value: Vec<u8>) -> XabiOwnedBytes {
    XabiOwnedBytes::from_vec(value)
}
