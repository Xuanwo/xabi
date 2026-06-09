pub const TRAIT_ID: &str = "xabi.test.AsyncPlugin";
pub const ABI_VERSION: u32 = 1;

#[xabi::data]
#[derive(Clone, Copy)]
pub struct BuildInput {
    pub value: u64,
}

#[xabi::xabi(id = TRAIT_ID, version = ABI_VERSION)]
pub trait AsyncPlugin {
    fn name(&self) -> String;

    async fn build(&self, input: BuildInput) -> xabi::Result<Vec<u8>>;

    async fn load(&self, details: &[u8]) -> xabi::Result<()>;
}

#[cfg(test)]
mod tests {
    #[test]
    fn abi_is_stable() {
        xabi_assert::assert_abi!(super::XabiV1AbiTraitAsyncPlugin);
    }
}
