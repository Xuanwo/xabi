use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use scalar_index_abi::{ForeignScalarIndexPlugin, Result, ScalarIndexPlugin, TRAIT_ID};

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
    pub unsafe fn register_dylib(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let library = xabi::LoadedLibrary::open(path)?;
        let handle = library.handle();
        for entry in library.entries()? {
            let trait_id = entry.trait_id.as_str()?;
            if trait_id != TRAIT_ID {
                continue;
            }
            let name = entry.name.as_str()?.to_string();
            let plugin = ForeignScalarIndexPlugin::from_entry(entry, Arc::clone(&handle))?;
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
            registry.register_dylib(&plugin_path)?;
        }

        let plugin = registry
            .get("demo-scalar-index")
            .ok_or_else(|| scalar_index_abi::Error::new("plugin was not registered"))?;
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
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = manifest_dir
            .ancestors()
            .nth(3)
            .expect("host package lives under workspace/examples/scalar-index/host");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = Command::new(cargo)
            .args(["build", "-p", "scalar-index-plugin"])
            .current_dir(workspace)
            .status()
            .expect("failed to run cargo build for scalar-index-plugin");
        assert!(status.success(), "failed to build scalar-index-plugin");

        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let filename = dynamic_library_filename("scalar_index_plugin");
        let path = workspace.join("target").join(profile).join(filename);
        assert!(
            path.exists(),
            "plugin cdylib does not exist: {}",
            path.display()
        );
        path
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
