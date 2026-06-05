from pathlib import Path

from . import _scalar_index_plugin as _native

TRAIT_ID = _native.trait_id()
PLUGIN_NAME = _native.native_plugin_name()
IMPL_VERSION = _native.impl_version()


def plugin_path() -> str:
    return str(Path(_native.__file__).resolve())


def register(registry):
    return registry.register(plugin_path())
