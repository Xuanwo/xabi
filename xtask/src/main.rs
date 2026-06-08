use std::collections::BTreeMap;
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
use xabi::{
    XabiBytes, XabiExport, XabiManifest, XabiOption, XabiOwnedBytes, XabiResult, XabiSlice, XabiStr,
};

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
    let compatibility = match compare_compatibility(&expected, &actual) {
        Ok(()) => "append-only compatible; update the snapshot if this ABI change is intentional"
            .to_string(),
        Err(err) => format!("breaking or unparsable ABI change: {err}"),
    };
    Err(format!(
        "ABI snapshot mismatch at line {}\nexpected: {}\nactual:   {}\ncompatibility: {}\nrun `cargo run -p xtask -- abi snapshot` only after intentionally changing the ABI",
        index + 1,
        expected_lines.get(index).copied().unwrap_or("<missing>"),
        actual_lines.get(index).copied().unwrap_or("<missing>"),
        compatibility,
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

fn compare_compatibility(expected: &str, actual: &str) -> Result<(), String> {
    let expected = parse_snapshot(expected)?;
    let actual = parse_snapshot(actual)?;

    if expected.format != actual.format {
        return Err(format!(
            "snapshot format changed from {} to {}",
            expected.format, actual.format
        ));
    }
    if expected.target != actual.target {
        return Err(format!(
            "target changed from {} to {}",
            expected.target, actual.target
        ));
    }

    for (name, expected_ty) in &expected.types {
        let actual_ty = actual
            .types
            .get(name)
            .ok_or_else(|| format!("type {name} was removed"))?;
        if actual_ty.size < expected_ty.size {
            return Err(format!(
                "type {name} shrank from {} to {}",
                expected_ty.size, actual_ty.size
            ));
        }
        if actual_ty.align != expected_ty.align {
            return Err(format!(
                "type {name} alignment changed from {} to {}",
                expected_ty.align, actual_ty.align
            ));
        }
        for (field_name, expected_field) in &expected_ty.fields {
            let actual_field = actual_ty
                .fields
                .get(field_name)
                .ok_or_else(|| format!("type {name} field {field_name} was removed"))?;
            if actual_field.offset != expected_field.offset || actual_field.ty != expected_field.ty
            {
                return Err(format!(
                    "type {name} field {field_name} changed from offset={} type={} to offset={} type={}",
                    expected_field.offset,
                    expected_field.ty,
                    actual_field.offset,
                    actual_field.ty,
                ));
            }
        }
    }

    for (name, expected_vtable) in &expected.vtables {
        let actual_vtable = actual
            .vtables
            .get(name)
            .ok_or_else(|| format!("vtable {name} was removed"))?;
        if actual_vtable.full_size < expected_vtable.full_size {
            return Err(format!(
                "vtable {name} shrank from {} to {}",
                expected_vtable.full_size, actual_vtable.full_size
            ));
        }
        if actual_vtable.min_size > expected_vtable.min_size {
            return Err(format!(
                "vtable {name} minimum prefix grew from {} to {}",
                expected_vtable.min_size, actual_vtable.min_size
            ));
        }
    }

    Ok(())
}

#[derive(Default)]
struct Snapshot {
    format: String,
    target: String,
    types: BTreeMap<String, TypeLayout>,
    vtables: BTreeMap<String, VTableLayout>,
}

#[derive(Default)]
struct TypeLayout {
    size: usize,
    align: usize,
    fields: BTreeMap<String, FieldLayout>,
}

struct FieldLayout {
    offset: usize,
    ty: String,
}

#[derive(Default)]
struct VTableLayout {
    full_size: usize,
    min_size: usize,
}

enum SnapshotEntry {
    Type(String),
    VTable(String),
}

fn parse_snapshot(input: &str) -> Result<Snapshot, String> {
    let mut snapshot = Snapshot::default();
    let mut entry = None;

    for line in input.lines() {
        if line.is_empty() {
            entry = None;
            continue;
        }
        if let Some(format) = line.strip_prefix("format=") {
            snapshot.format = format.to_string();
            continue;
        }
        if let Some(target) = line.strip_prefix("target=") {
            snapshot.target = target.to_string();
            continue;
        }
        if let Some(name) = line.strip_prefix("type ") {
            snapshot
                .types
                .entry(name.to_string())
                .or_insert_with(TypeLayout::default);
            entry = Some(SnapshotEntry::Type(name.to_string()));
            continue;
        }
        if let Some(name) = line.strip_prefix("vtable ") {
            snapshot
                .vtables
                .entry(name.to_string())
                .or_insert_with(VTableLayout::default);
            entry = Some(SnapshotEntry::VTable(name.to_string()));
            continue;
        }

        let Some(entry) = &entry else {
            return Err(format!("line outside snapshot entry: {line}"));
        };
        let trimmed = line.trim_start();
        match entry {
            SnapshotEntry::Type(name) => parse_type_line(
                snapshot
                    .types
                    .get_mut(name)
                    .expect("type entry exists while parsing"),
                trimmed,
            )?,
            SnapshotEntry::VTable(name) => parse_vtable_line(
                snapshot
                    .vtables
                    .get_mut(name)
                    .expect("vtable entry exists while parsing"),
                trimmed,
            )?,
        }
    }

    if snapshot.format.is_empty() {
        return Err("snapshot format is missing".to_string());
    }
    if snapshot.target.is_empty() {
        return Err("snapshot target is missing".to_string());
    }

    Ok(snapshot)
}

fn parse_type_line(layout: &mut TypeLayout, line: &str) -> Result<(), String> {
    if let Some(value) = line.strip_prefix("size=") {
        layout.size = parse_usize(value, "type size")?;
        return Ok(());
    }
    if let Some(value) = line.strip_prefix("align=") {
        layout.align = parse_usize(value, "type align")?;
        return Ok(());
    }
    let Some(rest) = line.strip_prefix("field.") else {
        return Err(format!("unsupported type line: {line}"));
    };
    let Some((name, rest)) = rest.split_once(" offset=") else {
        return Err(format!("field line is missing offset: {line}"));
    };
    let Some((offset, ty)) = rest.split_once(" type=") else {
        return Err(format!("field line is missing type: {line}"));
    };
    layout.fields.insert(
        name.to_string(),
        FieldLayout {
            offset: parse_usize(offset, "field offset")?,
            ty: ty.to_string(),
        },
    );
    Ok(())
}

fn parse_vtable_line(layout: &mut VTableLayout, line: &str) -> Result<(), String> {
    if let Some(value) = line.strip_prefix("full_size=") {
        layout.full_size = parse_usize(value, "vtable full_size")?;
        return Ok(());
    }
    if let Some(value) = line.strip_prefix("min_size=") {
        layout.min_size = parse_usize(value, "vtable min_size")?;
        return Ok(());
    }
    Err(format!("unsupported vtable line: {line}"))
}

fn parse_usize(value: &str, context: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|err| format!("invalid {context} `{value}`: {err}"))
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
    type_layout::<XabiOption>(
        &mut out,
        "xabi::XabiOption",
        &[
            field!("size", XabiOption, size, "usize"),
            field!("abi_version", XabiOption, abi_version, "u32"),
            field!("is_some", XabiOption, is_some, "u8"),
            field!("payload", XabiOption, payload, "XabiOwnedBytes"),
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
            field!("size", XabiExport, size, "usize"),
            field!("abi_version", XabiExport, abi_version, "u32"),
            field!("abi_id", XabiExport, abi_id, "XabiStr"),
            field!("name", XabiExport, name, "XabiStr"),
            field!("contract_version", XabiExport, contract_version, "u32"),
            field!("export_version", XabiExport, export_version, "u32"),
            field!("capabilities", XabiExport, capabilities, "u64"),
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
            vtable_field!(
                "put",
                IndexStoreVTable,
                put,
                "unsafe extern \"C\" fn(*mut c_void, XabiStr, XabiBytes, *mut XabiFuture) -> i32"
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
            vtable_field!(
                "update",
                IndexBuildProgressVTable,
                update,
                "unsafe extern \"C\" fn(*mut c_void, *const i64, *mut XabiFuture) -> i32"
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
            vtable_field!(
                "search",
                ScalarIndexVTable,
                search,
                "unsafe extern \"C\" fn(*mut c_void, XabiStr, *mut XabiFuture) -> i32"
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
