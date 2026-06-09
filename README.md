# xabi

`xabi` generates a stable native ABI from Rust traits.

The intended use case is a host application that wants to load third-party Rust
implementations from a dynamic library without asking users to hand-write C ABI
vtables, exported symbols, panic guards, async polling glue, or host-side
handles.

`xabi` is not a plugin framework. A plugin registry, package format, discovery
protocol, permission model, or product-specific lifecycle belongs in the host
project. `xabi` only owns the contract boundary.

## What It Generates

Given a Rust trait:

```rust
pub const TRAIT_ID: &str = "dev.example.index";
pub const ABI_VERSION: u32 = 1;

#[xabi::data]
#[derive(Clone, Copy)]
pub struct TrainInput {
    pub rows_seen: u64,
}

#[xabi::xabi(id = TRAIT_ID, version = ABI_VERSION)]
pub trait IndexPlugin {
    fn name(&self) -> String;

    fn version(&self) -> u32;

    async fn train(&self, input: TrainInput) -> xabi::Result<Vec<u8>>;

    fn details_as_json(&self, details: &[u8]) -> xabi::Result<Option<String>>;
}
```

`xabi` generates:

- a versioned C-compatible vtable,
- export thunks for sync and async methods,
- panic guards that convert unwinds to ABI status codes,
- host-side owned and borrowed handles,
- `xabi_manifest` integration for dynamic modules,
- typed error payload encoding,
- composable optional payload encoding through `Option<T>` where `T: XabiType`,
- stable wire layouts for `#[xabi::data]` values.

Generated ABI artifacts use an explicit `XabiV1` prefix, for example:

```rust
XabiV1AbiTraitIndexPlugin
XabiV1VtableTraitIndexPlugin
XabiV1HandleTraitIndexPlugin
XabiV1BorrowedTraitIndexPlugin
XabiV1OwnedTraitIndexPlugin
```

These names are ABI artifacts. Domain crates should usually re-export only the
handles or helper APIs they want users to see.

## Export A Module

An implementation crate exports one or more implementations with
`#[xabi::module]`:

```rust
#[derive(Default)]
pub struct DemoPlugin;

#[xabi::module]
mod exports {
    use super::*;

    #[xabi::xabi(name = "demo", version = 1)]
    impl IndexPlugin for DemoPlugin {
        fn name(&self) -> String {
            "demo".to_string()
        }

        fn version(&self) -> u32 {
            1
        }

        async fn train(&self, input: TrainInput) -> xabi::Result<Vec<u8>> {
            Ok(input.rows_seen.to_le_bytes().to_vec())
        }

        fn details_as_json(&self, details: &[u8]) -> xabi::Result<Option<String>> {
            let value = std::str::from_utf8(details)
                .map_err(|err| xabi::Error::Export(err.to_string()))?;
            Ok(Some(format!(r#"{{"details":"{value}"}}"#)))
        }
    }
}
```

The exported crate is built as a `cdylib`. The module macro emits the manifest
symbol that hosts load. The implementation `version` is stored as the export
version; the trait ABI version is stored separately and checked before the
generated host calls the export constructor.

## Load From A Host

The host loads a trusted dynamic library and asks the generated handle to find a
matching export:

```rust
let module = unsafe { xabi::load(path)? };
let plugin = unsafe { XabiV1HandleTraitIndexPlugin::xabi_load(&module)? };

let name = plugin.name()?;
let bytes = plugin.train(TrainInput::new(42)).await?;
```

Loading native code is unsafe. The host must trust the library and must define
the higher-level registration policy.

## Data, Handles, And Returns

Use `#[xabi::data]` for values that cross the ABI by value or as typed error
payloads:

```rust
#[xabi::data]
pub struct BuildError {
    pub message: String,
}
```

Each field is lowered through its own `XabiType::Wire`, so nested xabi data,
strings, owned bytes, callback refs, and opaque handles follow one recursive
rule.

Use `#[xabi::opaque]` for non-null pointer handles owned by another standard or
domain:

```rust
#[xabi::opaque]
#[derive(Clone, Copy)]
pub struct ArrowStreamHandle {
    stream: *mut ArrowArrayStream,
}
```

Trait object returns are represented as `impl Trait`:

```rust
#[xabi::xabi(id = FACTORY_ID, version = 1)]
pub trait Factory {
    async fn open(&self, name: &str) -> xabi::Result<impl IndexPlugin + 'static>;
}
```

The exporter turns the concrete Rust value into the returned trait's vtable.
The host decodes it into the generated handle while preserving the dynamic
module lifetime.

Borrowed callback traits use the same mechanism. A host can export a local
callback as `XabiV1OwnedTrait*`, pass `xabi_borrow()` to the plugin, and the
plugin calls the generated borrowed handle.

## ABI Stability Model

Extensible ABI descriptors and generated wire structs start with:

```rust
size: usize,
abi_version: u32,
```

Hosts validate the required prefix and generated handles do not read fields
beyond the reported size. Vtable methods live after the stable release prefix,
so a shorter vtable reports an ABI mismatch instead of reading unavailable tail
fields. Additive fields are appended to the tail. Breaking changes require a new
ABI version.

Small primitive carriers such as `XabiStr`, `XabiSlice`, `XabiBytes`,
`XabiOwnedBytes`, and `XabiResult` have fixed layouts. Extending their field
sets is a breaking runtime ABI change.

The contract identity is:

- a stable trait `id`,
- a contract ABI version carried in `XabiExport::contract_version`,
- the generated vtable and wire layouts.

The Rust trait name is not the runtime identity. It is used to generate Rust
API artifacts.

Check the fixture layouts with:

```sh
cargo run -p xtask -- abi check
```

When an ABI change is intentional, update the snapshot:

```sh
cargo run -p xtask -- abi snapshot
```

Review snapshot changes with the append-only layout rule in mind.

## Examples

- `examples/async-plugin`: minimal async trait export and host loading.
- `examples/scalar-index`: a richer fixture with nested data, callbacks, an
  opaque Arrow stream handle, an object return, a Rust host, and a Python
  package wrapper that registers the native library back into the host.
- `examples/access-like`: an OpenDAL `Access`-shaped fixture with all accessor
  operations, returned reader/writer/lister/deleter/copier handles, a Rust host,
  and a Python package wrapper for the native plugin.

Run the main end-to-end fixture:

```sh
cargo test -p scalar-index-host
cargo test -p access-like-host
```

Run all workspace tests:

```sh
cargo test --workspace
```

## How xabi Differs From Existing Choices

`xabi` sits in a narrow space: native, in-process, Rust-authored contracts with
explicit ABI stability.

| Existing choice | Difference |
| --- | --- |
| Hand-written C ABI with `libloading` | `xabi` still uses native dynamic loading, but users write Rust traits and data structs. Vtables, manifests, error payloads, async polling, panic guards, and host handles are generated. |
| Rust stable-ABI libraries | `xabi` does not try to make Rust's general ABI stable. It generates a small C-compatible ABI per contract and keeps the public model close to Rust trait definitions. |
| Wasm Component Model | Wasm gives portability, sandboxing, and a language-neutral component boundary. `xabi` chooses trusted native dynamic libraries and direct FFI for hosts that need the Rust implementation to run in-process. |
| Protobuf, gRPC, Arrow Flight, or IPC | Those are serialization and transport boundaries. `xabi` is an in-process ABI boundary; it avoids a service process and leaves persistence or network transport to the host. |
| PyO3, N-API, JNI, or other language bindings | Those expose Rust to a specific language runtime. `xabi` defines the native host/plugin contract; a PyO3 package can be used only as distribution glue for the native library. |
| A full plugin framework | `xabi` intentionally does not define discovery, registries, configuration, permissions, or lifecycle policy. The host project owns those product decisions. |

The tradeoff is intentional. `xabi` provides less runtime infrastructure than a
component system and less language reach than an RPC protocol, but it gives a
small, auditable ABI for Rust hosts that want dynamic native extension points.

### Compared With `abi_stable` And `stabby`

Community crates such as [`abi_stable`](https://docs.rs/abi_stable/) and
[`stabby`](https://docs.rs/stabby/) solve an adjacent problem: they provide
reusable ABI-stable Rust-like types and trait-object machinery. `xabi` takes a
different axis: contract-first generation from host-owned Rust traits.

| Project | Primary model | Where `xabi` differs |
| --- | --- | --- |
| [`abi_stable`](https://docs.rs/abi_stable/) | Interface, implementation, and user crates built around `StableAbi`, `#[sabi_trait]`, prefix types, runtime layout checks, and ffi-safe standard-library replacements. | `abi_stable` is a good fit when a project wants to adopt its module model and ABI-safe type ecosystem. `xabi` keeps the public contract as ordinary Rust traits and data structs, then generates the contract-specific vtable, manifest export, panic guards, typed error path, async polling glue, and host handles. |
| [`stabby`](https://docs.rs/stabby/) | ABI as a library: `IStable`, `#[stabby::stabby]`, `repr(stabby)`, stable dyn pointers, closures, futures, and compact ABI-stable representations for options, results, strings, vectors, and enums. | `stabby` is a good fit when preserving a rich Rust-like ABI type universe and compact layout rules is the main goal. `xabi` intentionally avoids making a general Rust data-layout scheme the user-facing API; it lowers only the selected `#[xabi::xabi]` and `#[xabi::data]` boundary surface and snapshots those generated layouts per contract. |

The practical distinction is ownership of the abstraction. With `abi_stable` or
`stabby`, the domain API is usually shaped by the chosen stable-ABI type system.
With `xabi`, the host crate owns the domain trait, and the generated ABI is an
implementation detail with explicit names and snapshot fixtures.

## Current Status

`xabi` is experimental. The repository is still iterating on generated API
shape, naming, and ABI fixtures. Treat the ABI snapshot checks as part of the
design process, not as a release promise.
