use async_plugin_abi::{AsyncPlugin, BuildInput};

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

        async fn build(&self, input: BuildInput) -> xabi::Result<Vec<u8>> {
            futures_util_like_yield().await;
            Ok(format!("built:{}", input.value).into_bytes())
        }

        async fn load(&self, details: &[u8]) -> xabi::Result<()> {
            futures_util_like_yield().await;
            if details.starts_with(b"built:") {
                Ok(())
            } else {
                Err(xabi::Error::Export("invalid details".to_string()))
            }
        }
    }
}

async fn futures_util_like_yield() {
    std::future::ready(()).await
}
