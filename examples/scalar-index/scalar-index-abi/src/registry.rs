use std::collections::HashMap;
use std::path::Path;

use crate::{Result, ScalarIndexPlugin, XabiScalarIndexPluginHandle};

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
        let library = unsafe { xabi::Module::load(path) }?;
        for (name, plugin) in XabiScalarIndexPluginHandle::xabi_load_all(&library)? {
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
