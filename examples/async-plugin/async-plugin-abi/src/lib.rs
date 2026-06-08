use std::fmt;

pub const TRAIT_ID: &str = "xabi.test.AsyncPlugin";
pub const ABI_VERSION: u32 = 1;

#[derive(Debug)]
pub enum Error {
    Xabi(xabi::Error),
    Message(String),
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}

impl From<xabi::Error> for Error {
    fn from(value: xabi::Error) -> Self {
        Self::Xabi(value)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xabi(err) => err.fmt(f),
            Self::Message(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BuildInput {
    pub size: usize,
    pub value: u64,
}

impl BuildInput {
    pub fn new(value: u64) -> Self {
        Self {
            size: std::mem::size_of::<Self>(),
            value,
        }
    }

    pub fn validate(&self) -> Result<()> {
        xabi::validate_size(self.size, std::mem::size_of::<Self>(), "BuildInput")?;
        Ok(())
    }

    /// # Safety
    ///
    /// `ptr` must be valid for reads of a [`BuildInput`] value.
    pub unsafe fn from_ptr<'a>(ptr: *const Self) -> Result<&'a Self> {
        let input = unsafe {
            ptr.as_ref()
                .ok_or_else(|| Error::new("BuildInput pointer is null"))?
        };
        input.validate()?;
        Ok(input)
    }
}

#[xabi::xabi(id = TRAIT_ID, version = ABI_VERSION, error = Error)]
pub trait AsyncPlugin {
    fn name(&self) -> String;

    async fn build(&self, input: BuildInput) -> Result<Vec<u8>>;

    async fn load(&self, details: &[u8]) -> Result<()>;
}
