use async_plugin_abi::{AsyncPlugin, BuildInput, Result};

#[derive(Default)]
struct DemoAsyncPlugin;

#[xabi::module]
mod exports {
    use super::*;

    #[xabi::xabi(name = "demo-async-plugin", version = 1)]
    impl AsyncPlugin for DemoAsyncPlugin {
        fn name(&self) -> String {
            "demo-async-plugin".to_string()
        }

        async fn build(&self, input: BuildInput) -> Result<Vec<u8>> {
            futures_util_like_yield().await;
            Ok(format!("built:{}", input.value).into_bytes())
        }

        async fn load(&self, details: &[u8]) -> Result<()> {
            futures_util_like_yield().await;
            if details.starts_with(b"built:") {
                Ok(())
            } else {
                Err(async_plugin_abi::Error::new("invalid details"))
            }
        }
    }
}

async fn futures_util_like_yield() {
    std::future::ready(()).await
}
