pub use xabi::{FfiBytes, FfiOwned, FfiSlice};

pub fn borrowed(value: &[u8]) -> FfiBytes {
    FfiBytes::from_slice(value)
}

pub fn owned(value: Vec<u8>) -> FfiOwned {
    FfiOwned::from_vec(value)
}
