mod arrow;
mod error;
mod host;
mod plugin;

pub use arrow::{
    ArrowArray, ArrowArrayStream, ArrowSchema, ArrowStreamHandle, InMemoryArrowStream,
    drain_arrow_stream,
};
pub use error::Error;
pub use host::{
    BorrowedIndexBuildProgress, BorrowedIndexStore, HostIndexBuildProgress, HostIndexStore,
    IndexBuildProgress, IndexBuildProgressAbi, IndexStore, IndexStoreAbi, OwnedIndexBuildProgress,
    OwnedIndexStore,
};
pub use plugin::{
    ABI_VERSION, INDEX_TRAIT_ID, OpTrain, ScalarIndex, ScalarIndexAbi, ScalarIndexPlugin,
    ScalarIndexPluginAbi, TRAIT_ID, TrainInput, TrainOutput, XabiScalarIndexHandle,
    XabiScalarIndexPluginHandle, drain_stream_for_plugin,
};

pub type Result<T> = std::result::Result<T, Error>;

pub fn code_to_result(code: i32, context: &str) -> Result<()> {
    match code {
        xabi::OK => Ok(()),
        xabi::ERR_PANIC => Err(Error::new(format!("{context}: export panicked"))),
        xabi::ERR_EXPORT => Err(Error::new(format!("{context}: export returned an error"))),
        xabi::ERR_HOST => Err(Error::new(format!(
            "{context}: host callback returned an error"
        ))),
        xabi::ERR_INVALID_ARGUMENT => Err(Error::new(format!("{context}: invalid argument"))),
        other => Err(Error::new(format!("{context}: unknown xabi code {other}"))),
    }
}
