mod host;
mod plugin;

pub use host::{
    validate_progress_vtable, validate_store_vtable, IndexBuildProgress, IndexBuildProgressVTable,
    IndexStore, IndexStoreVTable,
};
pub use plugin::{
    cap, drain_stream_for_plugin, OpTrain, RpTrain, ScalarIndex, ScalarIndexPlugin,
    ScalarIndexPluginVTable, ScalarIndexVTable, TrainOutput, XabiScalarIndexHandle,
    XabiScalarIndexPluginHandle, ABI_VERSION, TRAIT_ID,
};
pub use xabi_arrow::{
    drain_arrow_stream, ArrowArray, ArrowArrayStream, ArrowSchema, ArrowStreamHandle,
    InMemoryArrowStream,
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
