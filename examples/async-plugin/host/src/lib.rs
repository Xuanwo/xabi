#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;

    use async_plugin_abi::{BuildInput, Result, XabiV1HandleTraitAsyncPlugin};

    #[tokio::test(flavor = "multi_thread")]
    async fn loads_cdylib_and_awaits_async_plugin_methods() -> Result<()> {
        let plugin_path = build_plugin_cdylib();
        let module = unsafe { xabi::load(plugin_path)? };
        let plugin = unsafe { XabiV1HandleTraitAsyncPlugin::xabi_load(&module)? };

        assert_eq!(plugin.name()?, "demo-async-plugin");

        let details = plugin.build(BuildInput::new(42)).await?;
        assert_eq!(details, b"built:42");
        plugin.load(&details).await?;

        let err = plugin
            .load(b"not-built")
            .await
            .expect_err("invalid details should fail");
        assert!(err.to_string().contains("invalid details"));

        Ok(())
    }

    fn build_plugin_cdylib() -> PathBuf {
        let workspace = workspace_root();
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = Command::new(cargo)
            .args(["build", "-p", "async-plugin"])
            .current_dir(&workspace)
            .status()
            .expect("failed to run cargo build for async-plugin");
        assert!(status.success(), "failed to build async-plugin");

        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let path = workspace
            .join("target")
            .join(profile)
            .join(dynamic_library_filename("async_plugin"));
        assert!(
            path.exists(),
            "export cdylib does not exist: {}",
            path.display()
        );
        path
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("workspace root exists")
            .to_path_buf()
    }

    fn dynamic_library_filename(stem: &str) -> String {
        if cfg!(target_os = "windows") {
            format!("{stem}.dll")
        } else if cfg!(target_os = "macos") {
            format!("lib{stem}.dylib")
        } else {
            format!("lib{stem}.so")
        }
    }
}
