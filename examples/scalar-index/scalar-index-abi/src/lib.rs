mod arrow;
mod host;
mod plugin;

pub use arrow::{
    drain_arrow_stream, ArrowArray, ArrowArrayStream, ArrowSchema, ArrowStreamHandle,
    InMemoryArrowStream, XabiArrowStreamHandle,
};
pub use host::{
    validate_progress_vtable, validate_store_vtable, BorrowedIndexBuildProgress,
    BorrowedIndexStore, HostIndexBuildProgress, HostIndexStore, IndexBuildProgress,
    IndexBuildProgressAbi, IndexBuildProgressVTable, IndexStore, IndexStoreAbi, IndexStoreVTable,
    OwnedIndexBuildProgress, OwnedIndexStore,
    XabiV1RefTraitIndexBuildProgressAbi as IndexBuildProgressRef,
    XabiV1RefTraitIndexStoreAbi as IndexStoreRef,
};
pub use plugin::{
    drain_stream_for_plugin, LoadedScalarIndex, OpTrain, ScalarIndex, ScalarIndexAbi,
    ScalarIndexPlugin, ScalarIndexPluginAbi, ScalarIndexPluginVTable, ScalarIndexVTable,
    TrainInput, TrainOutput, XabiScalarIndexHandle, XabiScalarIndexPluginHandle,
    XabiV1DataLoadedScalarIndex, XabiV1DataOpTrain, XabiV1DataTrainInput, XabiV1DataTrainOutput,
    ABI_VERSION, INDEX_TRAIT_ID, TRAIT_ID,
};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

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

impl xabi::XabiType for Error {
    type Wire = xabi::XabiErrorWire;

    fn into_wire(self) -> Self::Wire {
        xabi::XabiErrorWire {
            size: std::mem::size_of::<xabi::XabiErrorWire>(),
            abi_version: xabi::XabiErrorWire::ABI_VERSION,
            kind: 1,
        }
    }

    unsafe fn from_wire(wire: *const Self::Wire) -> xabi::Result<Self> {
        let wire = unsafe {
            wire.as_ref()
                .ok_or(xabi::Error::NullPointer("scalar_index_abi::Error wire"))?
        };
        wire.validate()?;
        Ok(Self::new(format!("scalar index error kind {}", wire.kind)))
    }

    fn into_payload(self) -> xabi::XabiOwnedBytes {
        xabi::XabiOwnedBytes::from_string(self.message)
    }

    unsafe fn from_payload(payload: xabi::XabiOwnedBytes) -> xabi::Result<Self> {
        let message = unsafe { payload.to_string_and_free() }?;
        Ok(Self::new(message))
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
