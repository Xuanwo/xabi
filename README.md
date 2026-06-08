# xabi

`xabi` is a user-space cross-language ABI experiment for object-safe traits.

The current workspace implements the scalar-index fixture with raw xabi macros:

- `crates/xabi`: FFI-safe primitives, manifest types, panic guards, and a `libloading` based loader.
- `examples/scalar-index/scalar-index-abi`: domain traits plus macro-generated ABI vtables and
  host adapters.
- `examples/scalar-index/plugin`: an independently built `cdylib` plugin.
- `examples/scalar-index/host`: an end-to-end host test that loads the plugin through `xabi_manifest`.
- `examples/scalar-index/plugin/python`: a small Python package wrapper for the same Rust plugin
  when built with the `python` feature.

Run the P0 verification with:

```sh
cargo test -p scalar-index-host
```

Check ABI layout compatibility with:

```sh
cargo run -p xtask -- abi check
```

When an ABI change is intentional, regenerate the target-specific snapshot with:

```sh
cargo run -p xtask -- abi snapshot
```

Snapshot changes must be reviewed with the corresponding `abi_version`, `size`, and capability
compatibility rules in mind. Additive vtable methods belong at the tail, and required vtable fields
must remain inside `MIN_SIZE`.

Host registration uses the user-facing plugin API:

```rust
registry.register(path)?;
```

The scalar-index fixture uses `#[xabi::xabi]` and `#[xabi::module]` for vtable layout, manifest
export, host handles, borrowed callback handles, and panic-guarded FFI thunks.

The host tests also validate the PyO3 package path: the Rust plugin is built with
`--features python`, copied into a Python package as a native extension module, imported from
Python, and then registered back into the Rust host through:

```python
scalar_index_plugin.register(registry)
```
