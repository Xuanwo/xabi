use std::collections::HashMap;
use std::path::Path;

use access_like_abi::{AccessHandle, Result};

pub struct Registry {
    accessors: HashMap<String, AccessHandle>,
}

impl Registry {
    pub fn new() -> Self {
        Self {
            accessors: HashMap::new(),
        }
    }

    /// # Safety
    ///
    /// `path` must point to a trusted native library that exports a valid xabi manifest and follows
    /// the access-like ABI ownership and lifetime contracts.
    pub unsafe fn register(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let module = unsafe { xabi::Module::load(path) }?;
        for (name, access) in AccessHandle::xabi_load_all(&module)? {
            self.accessors.insert(name, access);
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&AccessHandle> {
        self.accessors.get(name)
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

    use access_like_abi::{
        BytesRange, Entry, Error, OpCopier, OpCopy, OpCreateDir, OpDelete, OpList, OpPresign,
        OpRead, OpRename, OpStat, OpWrite, Result, ENTRY_MODE_DIR, ENTRY_MODE_FILE, PRESIGN_READ,
    };

    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn loads_cdylib_and_calls_access_like_plugin() -> Result<()> {
        let plugin_path = build_plugin_cdylib();

        let mut registry = Registry::new();
        unsafe {
            registry.register(&plugin_path)?;
        }

        let access = registry
            .get("demo-access")
            .ok_or_else(|| Error::other("access export was not registered"))?;
        let info = access.info()?;
        assert_eq!(info.scheme, "memory");
        assert_eq!(info.root, "/");
        assert!(info.native_capability.read);
        assert!(info.native_capability.copy);

        access.create_dir("docs/", OpCreateDir::default()).await?;

        let mut write_args = OpWrite::default();
        write_args.if_not_exists = true;
        let (_, mut writer) = access.write("docs/readme.txt", write_args).await?;
        writer.write(b"hello ").await?;
        writer.write(b"access").await?;
        let written = writer.close().await?;
        assert_eq!(written.mode, ENTRY_MODE_FILE);
        assert_eq!(written.content_length, Some(12));

        let stat = access
            .stat("docs/readme.txt", OpStat::default())
            .await?
            .metadata;
        assert_eq!(stat.content_length, Some(12));

        let mut read_args = OpRead::default();
        read_args.range = BytesRange::new(Some(0), Some(5));
        let (read_rp, mut reader) = access.read("docs/readme.txt", read_args).await?;
        assert_eq!(
            read_rp
                .metadata
                .as_ref()
                .and_then(|metadata| metadata.content_length),
            Some(12)
        );
        assert_eq!(reader.read().await?, b"hell".to_vec());
        assert_eq!(reader.read_all().await?, b"o".to_vec());

        let mut list_args = OpList::default();
        list_args.recursive = true;
        let (_, mut lister) = access.list("docs/", list_args).await?;
        let mut entries = Vec::new();
        while let Some(entry) = lister.next().await? {
            entries.push(entry);
        }
        assert_eq!(
            entries,
            vec![Entry::new("docs/readme.txt".to_string(), stat.clone())]
        );

        let mut copier_opts = OpCopier::default();
        copier_opts.chunk = Some(4);
        let (_, mut copier) = access
            .copy(
                "docs/readme.txt",
                "docs/copy.txt",
                OpCopy::default(),
                copier_opts,
            )
            .await?;
        assert_eq!(copier.next().await?, Some(4));
        assert_eq!(copier.next().await?, None);
        let copied = copier.close().await?;
        assert_eq!(copied.content_length, Some(12));

        access
            .rename("docs/copy.txt", "docs/renamed.txt", OpRename::default())
            .await?;
        let renamed = access
            .stat("docs/renamed.txt", OpStat::default())
            .await?
            .metadata;
        assert_eq!(renamed.content_length, Some(12));

        let presigned = access
            .presign("docs/renamed.txt", OpPresign::new(60_000, PRESIGN_READ))
            .await?;
        assert_eq!(presigned.request.method, "GET");
        assert!(presigned.request.uri.contains("docs/renamed.txt"));

        let (_, mut deleter) = access.delete().await?;
        deleter
            .delete("docs/renamed.txt", OpDelete::default())
            .await?;
        deleter.close().await?;
        let missing = access
            .stat("docs/renamed.txt", OpStat::default())
            .await
            .expect_err("deleted object should not stat");
        let missing = Error::from(missing);
        assert_eq!(missing.kind, Error::KIND_NOT_FOUND);

        let root_meta = access.stat("docs/", OpStat::default()).await?.metadata;
        assert_eq!(root_meta.mode, ENTRY_MODE_DIR);

        Ok(())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn python_package_exposes_access_plugin() -> Result<()> {
        let package_root = build_python_plugin_package();
        let script = workspace_root().join("examples/access-like/host/python/check_package.py");
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

        let stdout =
            String::from_utf8(output.stdout).map_err(|err| Error::other(err.to_string()))?;
        let values = parse_key_values(&stdout);
        let plugin_path = values
            .get("path")
            .ok_or_else(|| Error::other("python output has no path"))?;
        assert_eq!(values.get("registered"), Some(plugin_path));
        assert_eq!(
            values.get("abi_id").map(String::as_str),
            Some("opendal.Access")
        );
        assert_eq!(values.get("name").map(String::as_str), Some("demo-access"));
        assert_eq!(values.get("version").map(String::as_str), Some("1"));

        Ok(())
    }

    fn build_plugin_cdylib() -> PathBuf {
        let workspace = workspace_root();
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = Command::new(cargo)
            .args(["build", "-p", "access-like-plugin"])
            .current_dir(&workspace)
            .status()
            .expect("failed to run cargo build for access-like-plugin");
        assert!(status.success(), "failed to build access-like-plugin");

        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let path = workspace
            .join("target")
            .join(profile)
            .join(dynamic_library_filename("access_like_plugin"));
        assert!(
            path.exists(),
            "export cdylib does not exist: {}",
            path.display()
        );
        path
    }

    fn build_python_plugin_package() -> PathBuf {
        let workspace = workspace_root();
        let target_dir = workspace.join("target/access-python-plugin");
        let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
        let status = Command::new(cargo)
            .args(["build", "-p", "access-like-plugin", "--features", "python"])
            .env("CARGO_TARGET_DIR", &target_dir)
            .current_dir(&workspace)
            .status()
            .expect("failed to run cargo build for access-like-plugin python package");
        assert!(
            status.success(),
            "failed to build access-like-plugin with python feature"
        );

        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let native = target_dir
            .join(profile)
            .join(dynamic_library_filename("access_like_plugin"));
        assert!(
            native.exists(),
            "python export native library does not exist: {}",
            native.display()
        );

        let package_root = workspace.join("target/access-python-package");
        let package_dir = package_root.join("access_like_plugin");
        if package_dir.exists() {
            fs::remove_dir_all(&package_dir).expect("failed to clean python package directory");
        }
        fs::create_dir_all(&package_dir).expect("failed to create python package directory");

        fs::copy(
            workspace.join("examples/access-like/plugin/python/access_like_plugin/__init__.py"),
            package_dir.join("__init__.py"),
        )
        .expect("failed to copy python package __init__.py");
        fs::copy(
            &native,
            package_dir.join(format!("_access_like_plugin{}", python_extension_suffix())),
        )
        .expect("failed to copy python extension module");

        package_root
    }

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("host package lives under workspace/examples/access-like/host")
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
