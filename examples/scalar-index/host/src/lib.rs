use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use scalar_index_abi::{Result, ScalarIndexPlugin, XabiScalarIndexPluginHandle, TRAIT_ID};

pub struct Registry {
    plugins: HashMap<String, Box<dyn ScalarIndexPlugin>>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// # Safety
    ///
    /// `path` must point to a trusted native library that exports a valid xabi manifest and follows
    /// the scalar-index ABI ownership and lifetime contracts.
    pub unsafe fn register(&mut self, path: impl AsRef<Path>) -> Result<()> {
        self.register_path(path)
    }

    unsafe fn register_path(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let library = xabi::Module::load(path)?;
        let handle = library.handle();
        for export in library.exports()? {
            let abi_id = export.abi_id.as_str()?;
            if abi_id != TRAIT_ID {
                continue;
            }
            let name = export.name.as_str()?.to_string();
            let plugin = unsafe {
                XabiScalarIndexPluginHandle::xabi_from_export(export, Arc::clone(&handle))?
            };
            self.plugins.insert(name, Box::new(plugin));
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&dyn ScalarIndexPlugin> {
        self.plugins.get(name).map(|plugin| plugin.as_ref())
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use scalar_index_abi::{InMemoryArrowStream, IndexBuildProgress, IndexStore, OpTrain, Result};

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn loads_cdylib_and_calls_scalar_index_plugin() -> Result<()> {
        let plugin_path = build_plugin_cdylib();

        let mut registry = Registry::new();
        unsafe {
            registry.register(&plugin_path)?;
        }

        let plugin = registry
            .get("demo-scalar-index")
            .ok_or_else(|| scalar_index_abi::Error::new("export was not registered"))?;
        assert_eq!(plugin.name(), "demo-scalar-index");
        assert_eq!(plugin.version(), 1);

        let store = Arc::new(MemoryStore::default());
        let progress = Arc::new(MemoryProgress::default());
        let mut stream = InMemoryArrowStream::new([3, 5, 8]);

        let trained = plugin
            .train_index(
                stream.handle(),
                store.clone(),
                progress.clone(),
                OpTrain::new(2),
            )
            .await?;

        assert_eq!(trained.rows_seen, 16);
        assert_eq!(trained.progress_events, 2);
        assert_eq!(trained.details, b"demo:index:rows=16");
        assert_eq!(progress.rows(), vec![16, 16]);
        assert_eq!(store.get("index.details"), Some(b"rows=16".to_vec()));

        let stats = plugin.load_statistics(trained.details.clone()).await?;
        assert_eq!(stats.as_deref(), Some("statistics:18"));

        let index = plugin
            .load_index(trained.details.clone(), store.clone())
            .await?;
        assert_eq!(index.search("abc").await?, "demo:index:rows=16|query=abc");
        assert_eq!(
            store.get("index.loaded"),
            Some(b"demo:index:rows=16".to_vec())
        );

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn python_package_exposes_plugin_and_host_registers_it() -> Result<()> {
        let package_root = build_python_plugin_package();
        let script = workspace_root().join("examples/scalar-index/host/python/check_package.py");
        let output = Command::new(python_command())
            .arg(&script)
            .env("PYTHONPATH", &package_root)
            .output()
            .expect("failed to run python package check");
        assert!(
            output.status.success(),
            "python package check failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8(output.stdout)
            .map_err(|err| scalar_index_abi::Error::new(err.to_string()))?;
        let values = parse_key_values(&stdout);
        let plugin_path = values
            .get("path")
            .ok_or_else(|| scalar_index_abi::Error::new("python output has no path"))?;
        assert_eq!(values.get("registered"), Some(plugin_path));
        assert_eq!(
            values.get("abi_id").map(String::as_str),
            Some("lance.ScalarIndexPlugin")
        );
        assert_eq!(
            values.get("name").map(String::as_str),
            Some("demo-scalar-index")
        );
        assert_eq!(values.get("version").map(String::as_str), Some("1"));

        Ok(())
    }

    #[derive(Default)]
    struct MemoryStore {
        values: Mutex<HashMap<String, Vec<u8>>>,
    }

    impl MemoryStore {
        fn get(&self, path: &str) -> Option<Vec<u8>> {
            self.values.lock().unwrap().get(path).cloned()
        }
    }

    #[async_trait]
    impl IndexStore for MemoryStore {
        async fn put(&self, path: &str, data: &[u8]) -> Result<()> {
            self.values
                .lock()
                .unwrap()
                .insert(path.to_string(), data.to_vec());
            Ok(())
        }
    }

    #[derive(Default)]
    struct MemoryProgress {
        rows: Mutex<Vec<i64>>,
    }

    impl MemoryProgress {
        fn rows(&self) -> Vec<i64> {
            self.rows.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl IndexBuildProgress for MemoryProgress {
        async fn update(&self, rows: i64) -> Result<()> {
            self.rows.lock().unwrap().push(rows);
            Ok(())
        }
    }

    fn build_plugin_cdylib() -> PathBuf {
        let workspace = workspace_root();
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = Command::new(cargo)
            .args(["build", "-p", "scalar-index-plugin"])
            .current_dir(&workspace)
            .status()
            .expect("failed to run cargo build for scalar-index-plugin");
        assert!(status.success(), "failed to build scalar-index-plugin");

        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let filename = dynamic_library_filename("scalar_index_plugin");
        let path = workspace.join("target").join(profile).join(filename);
        assert!(
            path.exists(),
            "export cdylib does not exist: {}",
            path.display()
        );
        path
    }

    fn build_python_plugin_package() -> PathBuf {
        let workspace = workspace_root();
        let target_dir = workspace.join("target/python-plugin");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = Command::new(cargo)
            .args(["build", "-p", "scalar-index-plugin", "--features", "python"])
            .env("CARGO_TARGET_DIR", &target_dir)
            .current_dir(&workspace)
            .status()
            .expect("failed to run cargo build for scalar-index-export python package");
        assert!(
            status.success(),
            "failed to build scalar-index-export with python feature"
        );

        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let native = target_dir
            .join(profile)
            .join(dynamic_library_filename("scalar_index_plugin"));
        assert!(
            native.exists(),
            "python export native library does not exist: {}",
            native.display()
        );

        let package_root = workspace.join("target/python-package");
        let package_dir = package_root.join("scalar_index_plugin");
        if package_dir.exists() {
            fs::remove_dir_all(&package_dir).expect("failed to clean python package directory");
        }
        fs::create_dir_all(&package_dir).expect("failed to create python package directory");

        fs::copy(
            workspace.join("examples/scalar-index/plugin/python/scalar_index_plugin/__init__.py"),
            package_dir.join("__init__.py"),
        )
        .expect("failed to copy python package __init__.py");
        fs::copy(
            &native,
            package_dir.join(format!("_scalar_index_plugin{}", python_extension_suffix())),
        )
        .expect("failed to copy python extension module");

        package_root
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("host package lives under workspace/examples/scalar-index/host")
            .to_path_buf()
    }

    fn parse_key_values(stdout: &str) -> HashMap<String, String> {
        stdout
            .lines()
            .filter_map(|line| {
                line.split_once('=')
                    .map(|(key, value)| (key.to_string(), value.to_string()))
            })
            .collect()
    }

    fn python_extension_suffix() -> String {
        let output = Command::new(python_command())
            .args([
                "-c",
                "import sysconfig; print(sysconfig.get_config_var('EXT_SUFFIX') or '.so')",
            ])
            .output()
            .expect("failed to query python extension suffix");
        assert!(
            output.status.success(),
            "failed to query python extension suffix\nstderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("python extension suffix is not UTF-8")
            .trim()
            .to_string()
    }

    fn python_command() -> String {
        std::env::var("PYTHON").unwrap_or_else(|_| {
            if cfg!(target_os = "windows") {
                "python".to_string()
            } else {
                "python3".to_string()
            }
        })
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
