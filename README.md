# xabi

`xabi` is a user-space cross-language ABI experiment for object-safe traits.

The current workspace implements the P0 scalar-index fixture by hand:

- `crates/xabi`: FFI-safe primitives, manifest types, panic guards, and a `libloading` based loader.
- `examples/scalar-index/scalar-index-abi`: handwritten stable vtables and host adapters.
- `examples/scalar-index/plugin`: an independently built `cdylib` plugin.
- `examples/scalar-index/host`: an end-to-end host test that loads the plugin through `xabi_manifest`.

Run the P0 verification with:

```sh
cargo test -p scalar-index-host
```
