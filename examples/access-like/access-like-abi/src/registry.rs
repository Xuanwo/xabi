use std::collections::HashMap;
use std::path::Path;

use crate::{AccessHandle, Result};

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
