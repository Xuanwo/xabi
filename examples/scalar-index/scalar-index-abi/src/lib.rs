mod arrow;
mod host;
mod plugin;

pub use arrow::{
    ArrowArray, ArrowArrayStream, ArrowSchema, ArrowStreamHandle, InMemoryArrowStream,
    XabiV1OpaqueArrowStreamHandle, drain_arrow_stream,
};
pub use host::{
    BorrowedIndexBuildProgress, BorrowedIndexStore, HostIndexBuildProgress, HostIndexStore,
    IndexBuildProgress, IndexBuildProgressAbi, IndexBuildProgressVTable, IndexStore, IndexStoreAbi,
    IndexStoreVTable, OwnedIndexBuildProgress, OwnedIndexStore,
    XabiV1RefTraitIndexBuildProgressAbi as IndexBuildProgressRef,
    XabiV1RefTraitIndexStoreAbi as IndexStoreRef, validate_progress_vtable, validate_store_vtable,
};
pub use plugin::{
    ABI_VERSION, INDEX_TRAIT_ID, OpTrain, ScalarIndex, ScalarIndexAbi, ScalarIndexPlugin,
    ScalarIndexPluginAbi, ScalarIndexPluginVTable, ScalarIndexVTable, TRAIT_ID, TrainInput,
    TrainOutput, XabiScalarIndexHandle, XabiScalarIndexPluginHandle, XabiV1DataOpTrain,
    XabiV1DataTrainInput, XabiV1DataTrainOutput, XabiV1OwnedRefTraitScalarIndexAbi,
    drain_stream_for_plugin,
};

pub type Result<T> = std::result::Result<T, Error>;

#[xabi::data]
#[derive(Debug, Clone)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

impl From<xabi::Error> for Error {
    fn from(value: xabi::Error) -> Self {
        Self::new(value.to_string())
    }
}

impl From<xabi::XabiCallError<Error>> for Error {
    fn from(value: xabi::XabiCallError<Error>) -> Self {
        match value {
            xabi::XabiCallError::Runtime(err) => Self::from(err),
            xabi::XabiCallError::Export(err) => err,
        }
    }
}

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
