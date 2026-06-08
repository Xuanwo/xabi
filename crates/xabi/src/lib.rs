//! Stable ABI building blocks for Rust dynamic module systems.
//!
//! `xabi` provides a small C-compatible ABI surface for hosts and dynamically
//! loaded modules. The high-level path is:
//!
//! - define an ABI trait with [`xabi`],
//! - aggregate exported implementations with [`module`],
//! - load the dynamic module with [`load`] or [`Module::load`],
//! - use the generated `XabiV1HandleTrait*` handle on the host side.
//!
//! # Define a trait ABI
//!
//! ```
//! pub const TRAIT_ID: &str = "xabi.example.Demo";
//! pub const ABI_VERSION: u32 = 1;
//!
//! #[xabi::data]
//! #[derive(Clone, Copy)]
//! pub struct BuildInput {
//!     pub value: u64,
//! }
//!
//! #[xabi::xabi(id = TRAIT_ID, version = ABI_VERSION)]
//! pub trait Demo {
//!     fn name(&self) -> String;
//!     async fn build(&self, input: BuildInput) -> xabi::Result<Vec<u8>>;
//!     async fn load(&self, details: &[u8]) -> xabi::Result<()>;
//! }
//! ```
//!
//! # Export an implementation
//!
//! ```no_run
//! # pub const TRAIT_ID: &str = "xabi.example.Demo";
//! # pub const ABI_VERSION: u32 = 1;
//! # #[xabi::data]
//! # #[derive(Clone, Copy)]
//! # pub struct BuildInput { pub value: u64 }
//! # #[xabi::xabi(id = TRAIT_ID, version = ABI_VERSION)]
//! # pub trait Demo {
//! #     fn name(&self) -> String;
//! #     async fn build(&self, input: BuildInput) -> xabi::Result<Vec<u8>>;
//! #     async fn load(&self, details: &[u8]) -> xabi::Result<()>;
//! # }
//! #[derive(Default)]
//! struct DemoImpl;
//!
//! #[xabi::module]
//! mod exports {
//!     use super::*;
//!
//!     #[xabi::xabi(name = "demo", version = 1)]
//!     impl Demo for DemoImpl {
//!         fn name(&self) -> String {
//!             "demo".to_string()
//!         }
//!
//!         async fn build(&self, input: BuildInput) -> xabi::Result<Vec<u8>> {
//!             Ok(input.value.to_le_bytes().to_vec())
//!         }
//!
//!         async fn load(&self, _details: &[u8]) -> xabi::Result<()> {
//!             Ok(())
//!         }
//!     }
//! }
//! # fn main() {}
//! ```

#![deny(missing_docs)]

mod contract;
mod error;
mod ffi;
mod future;
mod library;
mod status;

pub use contract::{SendPtr, XabiContract, XabiType};
pub use error::{Error, Result, XabiCallError, XabiErrorWire};
pub use ffi::{XabiBytes, XabiOwnedBytes, XabiResult, XabiSlice, XabiStr};
pub use future::{XabiFuture, XabiFutureHandle, XabiTypedFuture, XabiWaker};
pub use library::{load, Module, ModuleHandle, XabiExport, XabiManifest};
pub use status::{
    catch_unwind_code, catch_unwind_or, catch_unwind_owned, status_to_result, validate_abi_version,
    validate_size, ABI_VERSION, ERR_EXPORT, ERR_HOST, ERR_INVALID_ARGUMENT, ERR_PANIC, OK,
    POLL_PENDING, POLL_READY,
};

/// Mark a Rust item as participating in xabi ABI generation.
///
/// On traits this macro defines an ABI contract. Inside a [`module`] it marks
/// an implementation export. Users may import this macro with `use xabi::xabi;`
/// and write `#[xabi(...)]`.
pub use xabi_macros::xabi;

/// Aggregate implementation exports from an inline Rust module.
///
/// The macro collects `#[xabi]` implementation items and emits the
/// `xabi_manifest` symbol for the dynamic module.
pub use xabi_macros::module;

/// Mark a Rust struct as a stable xabi data type.
///
/// The macro generates a versioned wire type and implements [`XabiType`].
pub use xabi_macros::data;
