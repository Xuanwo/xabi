use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use scalar_index_abi::{
    IndexBuildProgressRef, IndexBuildProgressVTable, IndexStoreRef, IndexStoreVTable, OpTrain,
    ScalarIndexPluginVTable, ScalarIndexVTable, XabiV1DataError, XabiV1DataOpTrain,
    XabiV1DataTrainInput, XabiV1DataTrainOutput, XabiV1OpaqueArrowStreamHandle,
    XabiV1OwnedRefTraitScalarIndexAbi,
};
use xabi::{XabiBytes, XabiExport, XabiManifest, XabiOwnedBytes, XabiResult, XabiSlice, XabiStr};

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
    let expected = normalize_line_endings(&expected);
    let actual = normalize_line_endings(&actual);
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

fn normalize_line_endings(value: &str) -> String {
    value.replace("\r\n", "\n")
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

    type_layout::<XabiStr>(
        &mut out,
        "xabi::XabiStr",
        &[
            field!("ptr", XabiStr, ptr, "*const u8"),
            field!("len", XabiStr, len, "usize"),
        ],
    );
    type_layout::<XabiSlice<u8>>(
        &mut out,
        "xabi::XabiSlice<u8>",
        &[
            field!("ptr", XabiSlice<u8>, ptr, "*const u8"),
            field!("len", XabiSlice<u8>, len, "usize"),
        ],
    );
    type_layout::<XabiBytes>(
        &mut out,
        "xabi::XabiBytes",
        &[tuple_field!("0", XabiBytes, 0, "XabiSlice<u8>")],
    );
    type_layout::<XabiOwnedBytes>(
        &mut out,
        "xabi::XabiOwnedBytes",
        &[
            field!("ptr", XabiOwnedBytes, ptr, "*mut u8"),
            field!("len", XabiOwnedBytes, len, "usize"),
            field!(
                "free",
                XabiOwnedBytes,
                free,
                "unsafe extern \"C\" fn(*mut u8, usize)"
            ),
        ],
    );
    type_layout::<XabiResult>(
        &mut out,
        "xabi::XabiResult",
        &[
            field!("code", XabiResult, code, "i32"),
            field!("payload", XabiResult, payload, "XabiOwnedBytes"),
        ],
    );
    type_layout::<XabiExport>(
        &mut out,
        "xabi::XabiExport",
        &[
            field!("abi_id", XabiExport, abi_id, "XabiStr"),
            field!("name", XabiExport, name, "XabiStr"),
            field!("version", XabiExport, version, "u32"),
            field!(
                "make",
                XabiExport,
                make,
                "unsafe extern \"C\" fn() -> *mut c_void"
            ),
        ],
    );
    type_layout::<XabiManifest>(
        &mut out,
        "xabi::XabiManifest",
        &[
            field!("size", XabiManifest, size, "usize"),
            field!("abi_version", XabiManifest, abi_version, "u32"),
            field!("exports", XabiManifest, exports, "XabiSlice<XabiExport>"),
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
                "unsafe extern \"C\" fn(*mut c_void, XabiStr, XabiBytes, *mut XabiFuture) -> i32"
            ),
            vtable_field!(
                "destroy",
                IndexStoreVTable,
                destroy,
                "unsafe extern \"C\" fn(*mut c_void)"
            ),
            vtable_field!(
                "release",
                IndexStoreVTable,
                release,
                "unsafe extern \"C\" fn(*mut IndexStoreVTable)"
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
                "unsafe extern \"C\" fn(*mut c_void, *const i64, *mut XabiFuture) -> i32"
            ),
            vtable_field!(
                "destroy",
                IndexBuildProgressVTable,
                destroy,
                "unsafe extern \"C\" fn(*mut c_void)"
            ),
            vtable_field!(
                "release",
                IndexBuildProgressVTable,
                release,
                "unsafe extern \"C\" fn(*mut IndexBuildProgressVTable)"
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
                "unsafe extern \"C\" fn(*mut c_void) -> XabiOwnedBytes"
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
                "load_statistics",
                ScalarIndexPluginVTable,
                load_statistics,
                "unsafe extern \"C\" fn(*mut c_void, XabiBytes, *mut XabiFuture) -> i32"
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
                "unsafe extern \"C\" fn(*mut c_void, XabiStr, *mut XabiFuture) -> i32"
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
        &[field!(
            "requested_partitions",
            OpTrain,
            requested_partitions,
            "u32"
        )],
    );
    type_layout::<XabiV1DataOpTrain>(
        &mut out,
        "scalar_index_abi::XabiV1DataOpTrain",
        &[
            field!("size", XabiV1DataOpTrain, size, "usize"),
            field!("abi_version", XabiV1DataOpTrain, abi_version, "u32"),
            field!(
                "requested_partitions",
                XabiV1DataOpTrain,
                requested_partitions,
                "u32"
            ),
        ],
    );
    type_layout::<XabiV1OpaqueArrowStreamHandle>(
        &mut out,
        "scalar_index_abi::XabiV1OpaqueArrowStreamHandle",
        &[
            field!("size", XabiV1OpaqueArrowStreamHandle, size, "usize"),
            field!(
                "abi_version",
                XabiV1OpaqueArrowStreamHandle,
                abi_version,
                "u32"
            ),
            field!(
                "stream",
                XabiV1OpaqueArrowStreamHandle,
                stream,
                "*mut ArrowArrayStream"
            ),
        ],
    );
    type_layout::<IndexStoreRef>(
        &mut out,
        "scalar_index_abi::IndexStoreRef",
        &[
            field!("size", IndexStoreRef, size, "usize"),
            field!("abi_version", IndexStoreRef, abi_version, "u32"),
            field!("vtable", IndexStoreRef, vtable, "*const IndexStoreVTable"),
        ],
    );
    type_layout::<IndexBuildProgressRef>(
        &mut out,
        "scalar_index_abi::IndexBuildProgressRef",
        &[
            field!("size", IndexBuildProgressRef, size, "usize"),
            field!("abi_version", IndexBuildProgressRef, abi_version, "u32"),
            field!(
                "vtable",
                IndexBuildProgressRef,
                vtable,
                "*const IndexBuildProgressVTable"
            ),
        ],
    );
    type_layout::<XabiV1DataTrainInput>(
        &mut out,
        "scalar_index_abi::XabiV1DataTrainInput",
        &[
            field!("size", XabiV1DataTrainInput, size, "usize"),
            field!("abi_version", XabiV1DataTrainInput, abi_version, "u32"),
            field!(
                "data",
                XabiV1DataTrainInput,
                data,
                "XabiV1OpaqueArrowStreamHandle"
            ),
            field!("store", XabiV1DataTrainInput, store, "IndexStoreRef"),
            field!(
                "progress",
                XabiV1DataTrainInput,
                progress,
                "IndexBuildProgressRef"
            ),
            field!("op", XabiV1DataTrainInput, op, "XabiV1DataOpTrain"),
        ],
    );
    type_layout::<XabiV1DataTrainOutput>(
        &mut out,
        "scalar_index_abi::XabiV1DataTrainOutput",
        &[
            field!("size", XabiV1DataTrainOutput, size, "usize"),
            field!("abi_version", XabiV1DataTrainOutput, abi_version, "u32"),
            field!("rows_seen", XabiV1DataTrainOutput, rows_seen, "i64"),
            field!(
                "progress_events",
                XabiV1DataTrainOutput,
                progress_events,
                "u32"
            ),
            field!("details", XabiV1DataTrainOutput, details, "XabiOwnedBytes"),
        ],
    );
    type_layout::<XabiV1OwnedRefTraitScalarIndexAbi>(
        &mut out,
        "scalar_index_abi::XabiV1OwnedRefTraitScalarIndexAbi",
        &[
            field!("size", XabiV1OwnedRefTraitScalarIndexAbi, size, "usize"),
            field!(
                "abi_version",
                XabiV1OwnedRefTraitScalarIndexAbi,
                abi_version,
                "u32"
            ),
            field!(
                "vtable",
                XabiV1OwnedRefTraitScalarIndexAbi,
                vtable,
                "*mut ScalarIndexVTable"
            ),
        ],
    );
    type_layout::<XabiV1DataError>(
        &mut out,
        "scalar_index_abi::XabiV1DataError",
        &[
            field!("size", XabiV1DataError, size, "usize"),
            field!("abi_version", XabiV1DataError, abi_version, "u32"),
            field!("message", XabiV1DataError, message, "XabiOwnedBytes"),
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
