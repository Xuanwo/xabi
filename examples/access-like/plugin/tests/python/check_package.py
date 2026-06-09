import access_like_plugin


class Registry:
    def __init__(self):
        self.paths = []

    def register(self, path):
        self.paths.append(path)
        return path


registry = Registry()
registered = access_like_plugin.register(registry)

print(f"path={access_like_plugin.plugin_path()}")
print(f"registered={registered}")
print(f"abi_id={access_like_plugin.TRAIT_ID}")
print(f"name={access_like_plugin.PLUGIN_NAME}")
print(f"version={access_like_plugin.EXPORT_VERSION}")
