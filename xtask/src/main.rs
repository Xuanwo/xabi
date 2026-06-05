use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use scalar_index_abi::{
    IndexBuildProgressVTable, IndexStoreVTable, OpTrain, RpTrain, ScalarIndexPluginVTable,
    ScalarIndexVTable,
};
use xabi::{FfiBytes, FfiOwned, FfiResult, FfiSlice, FfiStr, PluginEntry, PluginManifest};
use xabi_arrow::{ArrowArray, ArrowArrayStream, ArrowSchema};

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [cmd, subcmd] if cmd == "abi" && subcmd == "snapshot" => {
            let path = snapshot_path()?;
            write_snapshot(&path)?;
            println!("{}", path.display());
            Ok(())
        }
        [cmd, subcmd] if cmd == "abi" && subcmd == "check" => check_snapshot(),
        _ => Err("usage: cargo run -p xtask -- abi <snapshot|check>".to_string()),
    }
}

fn write_snapshot(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::write(path, render_snapshot()?)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))
}

fn check_snapshot() -> Result<(), String> {
    let path = snapshot_path()?;
    let expected = fs::read_to_string(&path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let actual = render_snapshot()?;
    if expected == actual {
        println!("ABI snapshot matches {}", path.display());
        return Ok(());
    }

    let expected_lines = expected.lines().collect::<Vec<_>>();
    let actual_lines = actual.lines().collect::<Vec<_>>();
    let mut first_diff = None;
    for index in 0..expected_lines.len().max(actual_lines.len()) {
        if expected_lines.get(index) != actual_lines.get(index) {
            first_diff = Some(index);
            break;
        }
    }

    let Some(index) = first_diff else {
        return Err(format!("ABI snapshot mismatch: {}", path.display()));
    };
    Err(format!(
        "ABI snapshot mismatch at line {}\nexpected: {}\nactual:   {}\nrun `cargo run -p xtask -- abi snapshot` only after intentionally changing the ABI",
        index + 1,
        expected_lines.get(index).copied().unwrap_or("<missing>"),
        actual_lines.get(index).copied().unwrap_or("<missing>"),
    ))
}

fn snapshot_path() -> Result<PathBuf, String> {
    Ok(workspace_root()?
        .join("abi/snapshots")
        .join(format!("{}.txt", host_triple()?)))
}

fn workspace_root() -> Result<PathBuf, String> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "xtask manifest has no parent".to_string())
}

fn host_triple() -> Result<String, String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .map_err(|err| format!("failed to run rustc -vV: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "rustc -vV failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|err| format!("rustc -vV output is not UTF-8: {err}"))?;
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_string))
        .ok_or_else(|| "rustc -vV did not report host triple".to_string())
}

macro_rules! field {
    ($name:literal, $ty:ty, $field:tt, $field_ty:literal) => {
        Field {
            name: $name,
            offset: std::mem::offset_of!($ty, $field),
            ty: $field_ty,
        }
    };
}

macro_rules! tuple_field {
    ($name:literal, $ty:ty, $field:tt, $field_ty:literal) => {
        Field {
            name: $name,
            offset: std::mem::offset_of!($ty, $field),
            ty: $field_ty,
        }
    };
}

macro_rules! vtable_field {
    ($name:literal, $ty:ty, $field:tt, $field_ty:literal) => {
        Field {
            name: $name,
            offset: std::mem::offset_of!($ty, $field),
            ty: $field_ty,
        }
    };
}

fn render_snapshot() -> Result<String, String> {
    let mut out = String::new();
    writeln!(out, "format=xabi-abi-snapshot-v1").unwrap();
    writeln!(out, "target={}", host_triple()?).unwrap();
    writeln!(out).unwrap();

    type_layout::<FfiStr>(
        &mut out,
        "xabi::FfiStr",
        &[
            field!("ptr", FfiStr, ptr, "*const u8"),
            field!("len", FfiStr, len, "usize"),
        ],
    );
    type_layout::<FfiSlice<u8>>(
        &mut out,
        "xabi::FfiSlice<u8>",
        &[
            field!("ptr", FfiSlice<u8>, ptr, "*const u8"),
            field!("len", FfiSlice<u8>, len, "usize"),
        ],
    );
    type_layout::<FfiBytes>(
        &mut out,
        "xabi::FfiBytes",
        &[tuple_field!("0", FfiBytes, 0, "FfiSlice<u8>")],
    );
    type_layout::<FfiOwned>(
        &mut out,
        "xabi::FfiOwned",
        &[
            field!("ptr", FfiOwned, ptr, "*mut u8"),
            field!("len", FfiOwned, len, "usize"),
            field!(
                "free",
                FfiOwned,
                free,
                "unsafe extern \"C\" fn(*mut u8, usize)"
            ),
        ],
    );
    type_layout::<FfiResult>(
        &mut out,
        "xabi::FfiResult",
        &[
            field!("code", FfiResult, code, "i32"),
            field!("payload", FfiResult, payload, "FfiOwned"),
        ],
    );
    type_layout::<PluginEntry>(
        &mut out,
        "xabi::PluginEntry",
        &[
            field!("trait_id", PluginEntry, trait_id, "FfiStr"),
            field!("name", PluginEntry, name, "FfiStr"),
            field!("impl_version", PluginEntry, impl_version, "u32"),
            field!(
                "make",
                PluginEntry,
                make,
                "unsafe extern \"C\" fn() -> *mut c_void"
            ),
        ],
    );
    type_layout::<PluginManifest>(
        &mut out,
        "xabi::PluginManifest",
        &[
            field!("size", PluginManifest, size, "usize"),
            field!("abi_version", PluginManifest, abi_version, "u32"),
            field!("entries", PluginManifest, entries, "FfiSlice<PluginEntry>"),
        ],
    );
    type_layout::<ArrowArray>(
        &mut out,
        "xabi_arrow::ArrowArray",
        &[
            field!("length", ArrowArray, length, "i64"),
            field!("null_count", ArrowArray, null_count, "i64"),
            field!("offset", ArrowArray, offset, "i64"),
            field!("n_buffers", ArrowArray, n_buffers, "i64"),
            field!("n_children", ArrowArray, n_children, "i64"),
            field!("buffers", ArrowArray, buffers, "*mut *const c_void"),
            field!("children", ArrowArray, children, "*mut *mut ArrowArray"),
            field!("dictionary", ArrowArray, dictionary, "*mut ArrowArray"),
            field!(
                "release",
                ArrowArray,
                release,
                "Option<unsafe extern \"C\" fn(*mut ArrowArray)>"
            ),
            field!("private_data", ArrowArray, private_data, "*mut c_void"),
        ],
    );
    type_layout::<ArrowSchema>(
        &mut out,
        "xabi_arrow::ArrowSchema",
        &[
            field!("format", ArrowSchema, format, "*const i8"),
            field!("name", ArrowSchema, name, "*const i8"),
            field!("metadata", ArrowSchema, metadata, "*const i8"),
            field!("flags", ArrowSchema, flags, "i64"),
            field!("n_children", ArrowSchema, n_children, "i64"),
            field!("children", ArrowSchema, children, "*mut *mut ArrowSchema"),
            field!("dictionary", ArrowSchema, dictionary, "*mut ArrowSchema"),
            field!(
                "release",
                ArrowSchema,
                release,
                "Option<unsafe extern \"C\" fn(*mut ArrowSchema)>"
            ),
            field!("private_data", ArrowSchema, private_data, "*mut c_void"),
        ],
    );
    type_layout::<ArrowArrayStream>(
        &mut out,
        "xabi_arrow::ArrowArrayStream",
        &[
            field!(
                "get_schema",
                ArrowArrayStream,
                get_schema,
                "Option<unsafe extern \"C\" fn(*mut ArrowArrayStream, *mut ArrowSchema) -> i32>"
            ),
            field!(
                "get_next",
                ArrowArrayStream,
                get_next,
                "Option<unsafe extern \"C\" fn(*mut ArrowArrayStream, *mut ArrowArray) -> i32>"
            ),
            field!(
                "get_last_error",
                ArrowArrayStream,
                get_last_error,
                "Option<unsafe extern \"C\" fn(*mut ArrowArrayStream) -> *const i8>"
            ),
            field!(
                "release",
                ArrowArrayStream,
                release,
                "Option<unsafe extern \"C\" fn(*mut ArrowArrayStream)>"
            ),
            field!(
                "private_data",
                ArrowArrayStream,
                private_data,
                "*mut c_void"
            ),
        ],
    );
    type_layout::<IndexStoreVTable>(
        &mut out,
        "scalar_index_abi::IndexStoreVTable",
        &[
            vtable_field!("size", IndexStoreVTable, size, "usize"),
            vtable_field!("abi_version", IndexStoreVTable, abi_version, "u32"),
            vtable_field!("capabilities", IndexStoreVTable, capabilities, "u64"),
            vtable_field!("instance", IndexStoreVTable, instance, "*mut c_void"),
            vtable_field!(
                "put",
                IndexStoreVTable,
                put,
                "unsafe extern \"C\" fn(*mut c_void, FfiStr, FfiBytes) -> i32"
            ),
        ],
    );
    min_size::<IndexStoreVTable>(
        &mut out,
        "scalar_index_abi::IndexStoreVTable",
        IndexStoreVTable::MIN_SIZE,
    );
    type_layout::<IndexBuildProgressVTable>(
        &mut out,
        "scalar_index_abi::IndexBuildProgressVTable",
        &[
            vtable_field!("size", IndexBuildProgressVTable, size, "usize"),
            vtable_field!("abi_version", IndexBuildProgressVTable, abi_version, "u32"),
            vtable_field!(
                "capabilities",
                IndexBuildProgressVTable,
                capabilities,
                "u64"
            ),
            vtable_field!(
                "instance",
                IndexBuildProgressVTable,
                instance,
                "*mut c_void"
            ),
            vtable_field!(
                "update",
                IndexBuildProgressVTable,
                update,
                "unsafe extern \"C\" fn(*mut c_void, i64) -> i32"
            ),
        ],
    );
    min_size::<IndexBuildProgressVTable>(
        &mut out,
        "scalar_index_abi::IndexBuildProgressVTable",
        IndexBuildProgressVTable::MIN_SIZE,
    );
    type_layout::<ScalarIndexPluginVTable>(
        &mut out,
        "scalar_index_abi::ScalarIndexPluginVTable",
        &[
            vtable_field!("size", ScalarIndexPluginVTable, size, "usize"),
            vtable_field!("abi_version", ScalarIndexPluginVTable, abi_version, "u32"),
            vtable_field!("capabilities", ScalarIndexPluginVTable, capabilities, "u64"),
            vtable_field!("instance", ScalarIndexPluginVTable, instance, "*mut c_void"),
            vtable_field!(
                "name",
                ScalarIndexPluginVTable,
                name,
                "unsafe extern \"C\" fn(*mut c_void) -> FfiOwned"
            ),
            vtable_field!(
                "version",
                ScalarIndexPluginVTable,
                version,
                "unsafe extern \"C\" fn(*mut c_void) -> u32"
            ),
            vtable_field!(
                "train_index",
                ScalarIndexPluginVTable,
                train_index,
                "unsafe extern \"C\" fn(...) -> i32"
            ),
            vtable_field!(
                "load_index",
                ScalarIndexPluginVTable,
                load_index,
                "unsafe extern \"C\" fn(...) -> i32"
            ),
            vtable_field!(
                "destroy",
                ScalarIndexPluginVTable,
                destroy,
                "unsafe extern \"C\" fn(*mut c_void)"
            ),
            vtable_field!(
                "release",
                ScalarIndexPluginVTable,
                release,
                "unsafe extern \"C\" fn(*mut ScalarIndexPluginVTable)"
            ),
            vtable_field!(
                "load_statistics",
                ScalarIndexPluginVTable,
                load_statistics,
                "unsafe extern \"C\" fn(*mut c_void, FfiBytes, *mut FfiOwned) -> i32"
            ),
        ],
    );
    min_size::<ScalarIndexPluginVTable>(
        &mut out,
        "scalar_index_abi::ScalarIndexPluginVTable",
        ScalarIndexPluginVTable::MIN_SIZE,
    );
    type_layout::<ScalarIndexVTable>(
        &mut out,
        "scalar_index_abi::ScalarIndexVTable",
        &[
            vtable_field!("size", ScalarIndexVTable, size, "usize"),
            vtable_field!("abi_version", ScalarIndexVTable, abi_version, "u32"),
            vtable_field!("capabilities", ScalarIndexVTable, capabilities, "u64"),
            vtable_field!("instance", ScalarIndexVTable, instance, "*mut c_void"),
            vtable_field!(
                "search",
                ScalarIndexVTable,
                search,
                "unsafe extern \"C\" fn(*mut c_void, FfiStr) -> FfiOwned"
            ),
            vtable_field!(
                "destroy",
                ScalarIndexVTable,
                destroy,
                "unsafe extern \"C\" fn(*mut c_void)"
            ),
            vtable_field!(
                "release",
                ScalarIndexVTable,
                release,
                "unsafe extern \"C\" fn(*mut ScalarIndexVTable)"
            ),
        ],
    );
    min_size::<ScalarIndexVTable>(
        &mut out,
        "scalar_index_abi::ScalarIndexVTable",
        ScalarIndexVTable::MIN_SIZE,
    );
    type_layout::<OpTrain>(
        &mut out,
        "scalar_index_abi::OpTrain",
        &[
            field!("size", OpTrain, size, "usize"),
            field!("requested_partitions", OpTrain, requested_partitions, "u32"),
        ],
    );
    type_layout::<RpTrain>(
        &mut out,
        "scalar_index_abi::RpTrain",
        &[
            field!("size", RpTrain, size, "usize"),
            field!("rows_seen", RpTrain, rows_seen, "i64"),
            field!("progress_events", RpTrain, progress_events, "u32"),
            field!("details", RpTrain, details, "FfiOwned"),
        ],
    );

    Ok(out)
}

#[derive(Clone, Copy)]
struct Field {
    name: &'static str,
    offset: usize,
    ty: &'static str,
}

fn type_layout<T>(out: &mut String, name: &str, fields: &[Field]) {
    writeln!(out, "type {name}").unwrap();
    writeln!(out, "  size={}", std::mem::size_of::<T>()).unwrap();
    writeln!(out, "  align={}", std::mem::align_of::<T>()).unwrap();
    for field in fields {
        writeln!(
            out,
            "  field.{} offset={} type={}",
            field.name, field.offset, field.ty
        )
        .unwrap();
    }
    writeln!(out).unwrap();
}

fn min_size<T>(out: &mut String, name: &str, min_size: usize) {
    writeln!(out, "vtable {name}").unwrap();
    writeln!(out, "  full_size={}", std::mem::size_of::<T>()).unwrap();
    writeln!(out, "  min_size={min_size}").unwrap();
    writeln!(out).unwrap();
}
