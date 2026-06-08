# AGENTS.md

## Project Boundaries

- `xabi` generates a stable native ABI from host-owned Rust traits. It is not a
  plugin framework; discovery, registries, package formats, permissions, and
  product lifecycle policy belong to host projects.
- The public user path is `#[xabi::xabi]` on traits, `#[xabi::module]` for
  export aggregation, `#[xabi::data]` for boundary data and typed errors,
  `#[xabi::opaque]` for non-null external handles, and generated `XabiV1*`
  handles on the host side.
- Do not expose `Plugin*`, `Foreign*`, `Ffi*`, or previous experimental naming
  as public API vocabulary. Generated ABI artifacts should use explicit
  `XabiV1*` names.
- Raw ABI construction is an implementation detail. Examples and downstream
  integrations must not hand-write vtables, thunks, `unsafe extern "C"` ABI
  fixtures, or call raw ABI helpers. If the public macros cannot express a real
  use case, extend `xabi` instead of reintroducing user-side scaffolding.

## ABI Design Rules

- ABI compatibility is the product. Any public ABI struct must preserve the
  `size` and `abi_version` prefix contract, validate minimum prefixes, and only
  append new fields at the tail. Breaking changes require a new ABI version.
- Trait identity is the explicit `id` plus ABI version and generated layout, not
  the Rust trait name.
- `async fn` support is first-class. Keep async syntax in the user-facing trait;
  `XabiFuture`, `XabiWaker`, and poll ABI details belong in generated/runtime
  glue unless maintaining those internals directly.
- `XabiType` is the single boundary-value contract. Domain payloads and errors
  should use `#[xabi::data]`; external pointer standards should use
  `#[xabi::opaque]`. Avoid adding xabi-owned crates for domain-specific formats
  such as bytes or Arrow unless the core contract itself changes.

## Workspace Conventions

- Repository documentation and code comments are written in English.
- `crates/xabi` owns runtime types, loading, validation, futures, and public
  docs. It has `#![deny(missing_docs)]`; every public API added there needs
  useful docs and, when practical, a doctest.
- `crates/xabi-macros` owns generated code. Macro output changes should be
  covered by focused tests and snapshot diffs, not only by example compilation.
- The examples are executable contract fixtures. Do not simplify them to hide a
  limitation; use them to force missing macro/runtime capability into `xabi`.

## Verification

- For ordinary changes, run:

  ```sh
  cargo fmt --check
  cargo test --workspace
  ```

- For ABI layout changes, also run:

  ```sh
  cargo run -p xtask -- abi check
  ```

- When an ABI snapshot change is intentional, update it explicitly:

  ```sh
  cargo run -p xtask -- abi snapshot
  ```

  Review the resulting diff for append-only layout compatibility and ABI version
  correctness before committing.
