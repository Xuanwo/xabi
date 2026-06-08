#[xabi::data]
#[derive(Clone, Copy)]
pub struct BuildInput {
    pub rows_seen: u64,
}

#[xabi::xabi(id = "xabi.test.ShapePlugin", version = 1)]
pub trait ShapePlugin {
    fn name(&self) -> String;

    fn version(&self) -> u32;

    fn enabled(&self) -> bool;

    fn put(&self, details: &[u8]) -> xabi::Result<()>;

    fn optional_json(&self, details: &[u8]) -> xabi::Result<Option<Vec<u8>>>;

    fn build_sync(&self, input: BuildInput) -> xabi::Result<Vec<u8>>;

    async fn build_async(&self, input: BuildInput) -> xabi::Result<Vec<u8>>;

    async fn load_async(&self, details: &[u8]) -> xabi::Result<()>;
}
