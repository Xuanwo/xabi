from pathlib import Path

from . import _scalar_index_plugin as _native

TRAIT_ID = _native.abi_id()
PLUGIN_NAME = _native.native_plugin_name()
EXPORT_VERSION = _native.export_version()


def plugin_path() -> str:
    return str(Path(_native.__file__).resolve())


def register(registry):
    return registry.register(plugin_path())
