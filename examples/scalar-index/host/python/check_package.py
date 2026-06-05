import scalar_index_plugin


class Registry:
    def __init__(self):
        self.paths = []

    def register_dylib(self, path):
        self.paths.append(path)
        return path


registry = Registry()
registered = scalar_index_plugin.register(registry)

print(f"path={scalar_index_plugin.plugin_path()}")
print(f"registered={registered}")
print(f"trait_id={scalar_index_plugin.TRAIT_ID}")
print(f"name={scalar_index_plugin.PLUGIN_NAME}")
print(f"version={scalar_index_plugin.IMPL_VERSION}")
