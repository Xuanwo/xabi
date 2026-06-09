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
