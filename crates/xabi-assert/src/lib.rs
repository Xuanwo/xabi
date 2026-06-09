use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use xabi::{
    XabiContractLayout, XabiLayout, XabiLayoutItem, XabiLayoutStability, XabiTypeLayout,
    XabiVTableLayout,
};

/// Default directory used by [`assert_abi!`].
pub const DEFAULT_SNAPSHOT_DIR: &str = "xabi/snapshots";

/// Assert that a generated xabi contract layout matches its committed snapshot.
///
/// The first argument is a generated ABI type from `#[xabi::xabi]`. The optional
/// second argument overrides the snapshot directory, which defaults to
/// `xabi/snapshots`.
///
/// ```no_run
/// # struct XabiV1AbiTraitDemo;
/// # fn collect(_: &mut dyn xabi::XabiLayoutCollector) {}
/// # impl XabiV1AbiTraitDemo {
/// #     pub const XABI_LAYOUT: xabi::XabiLayout = xabi::XabiLayout {
/// #         package: "demo",
/// #         module: "demo",
/// #         contract: xabi::XabiContractLayout::new("demo.Demo", 1, "demo::Demo"),
/// #         collect,
/// #     };
/// # }
/// xabi_assert::assert_abi!(XabiV1AbiTraitDemo, "xabi/snapshots");
/// ```
#[macro_export]
macro_rules! assert_abi {
    ($abi:path $(,)?) => {{
        use $abi as __xabi_assert_abi;
        $crate::assert_layout_in(
            &__xabi_assert_abi::XABI_LAYOUT,
            env!("CARGO_MANIFEST_DIR"),
            $crate::DEFAULT_SNAPSHOT_DIR,
        );
    }};
    ($abi:path, $snapshot_dir:expr $(,)?) => {{
        use $abi as __xabi_assert_abi;
        $crate::assert_layout_in(
            &__xabi_assert_abi::XABI_LAYOUT,
            env!("CARGO_MANIFEST_DIR"),
            $snapshot_dir,
        );
    }};
}

/// Assert that a layout matches a snapshot directory.
///
/// `manifest_dir` is the Cargo manifest directory used as the base for relative
/// snapshot paths. Snapshots are stored under
/// `<snapshot-dir>/<contract-id>/<target>.txt`. Use [`assert_abi!`] in tests
/// unless a caller needs custom path resolution.
pub fn assert_layout_in(
    layout: &XabiLayout,
    manifest_dir: impl AsRef<Path>,
    snapshot_dir: impl AsRef<Path>,
) {
    let manifest_dir = manifest_dir.as_ref();
    let snapshot_dir = snapshot_dir.as_ref();
    let target = target_triple();
    let snapshot = collect_snapshot(layout, &target);
    let snapshot_path = contract_snapshot_path(manifest_dir, snapshot_dir, &target, layout);
    let actual = snapshot.render();

    if std::env::var_os("XABI_UPDATE").is_some() {
        if let Some(parent) = snapshot_path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|err| {
                panic!("failed to create {}: {err}", parent.display());
            });
        }
        fs::write(&snapshot_path, actual).unwrap_or_else(|err| {
            panic!("failed to write {}: {err}", snapshot_path.display());
        });
        return;
    }

    let expected = fs::read_to_string(&snapshot_path).unwrap_or_else(|err| {
        panic!(
            "failed to read ABI snapshot {}: {err}\nrun `XABI_UPDATE=1 cargo test` to create it",
            snapshot_path.display()
        );
    });
    let expected = normalize_line_endings(&expected);
    let actual = normalize_line_endings(&actual);
    if expected == actual {
        return;
    }

    panic!("{}", mismatch_message(&snapshot_path, &expected, &actual));
}

fn collect_snapshot(layout: &XabiLayout, target: &str) -> Snapshot {
    let mut items = Vec::new();
    (layout.collect)(&mut items);
    Snapshot::from_layout(layout.package, layout.contract, target, items)
}

fn contract_snapshot_path(
    manifest_dir: &Path,
    snapshot_dir: &Path,
    target: &str,
    layout: &XabiLayout,
) -> PathBuf {
    manifest_dir
        .join(snapshot_dir)
        .join(snapshot_component(layout.contract.abi_id))
        .join(format!("{target}.txt"))
}

fn snapshot_component(value: &str) -> String {
    let out = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if out.is_empty() {
        "contract".to_string()
    } else {
        out
    }
}

fn normalize_line_endings(value: &str) -> String {
    value.replace("\r\n", "\n")
}

fn target_triple() -> String {
    if let Ok(target) = std::env::var("XABI_TARGET") {
        return target;
    }

    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .unwrap_or_else(|err| panic!("failed to run rustc -vV: {err}"));
    if !output.status.success() {
        panic!(
            "rustc -vV failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let stdout = String::from_utf8(output.stdout)
        .unwrap_or_else(|err| panic!("rustc -vV output is not UTF-8: {err}"));
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("host: ").map(str::to_string))
        .unwrap_or_else(|| "rustc -vV did not report host triple".to_string())
}

fn mismatch_message(path: &Path, expected: &str, actual: &str) -> String {
    let expected_lines = expected.lines().collect::<Vec<_>>();
    let actual_lines = actual.lines().collect::<Vec<_>>();
    let index = (0..expected_lines.len().max(actual_lines.len()))
        .find(|index| expected_lines.get(*index) != actual_lines.get(*index));

    let Some(index) = index else {
        return format!("ABI snapshot mismatch: {}", path.display());
    };
    let compatibility = match compare_compatibility(expected, actual) {
        Ok(()) => "append-only compatible; update the snapshot if this ABI change is intentional"
            .to_string(),
        Err(err) => format!("breaking or unparsable ABI change: {err}"),
    };
    format!(
        "ABI snapshot mismatch at line {}\nexpected: {}\nactual:   {}\ncompatibility: {}\nrun `XABI_UPDATE=1 cargo test` only after intentionally changing the ABI",
        index + 1,
        expected_lines.get(index).copied().unwrap_or("<missing>"),
        actual_lines.get(index).copied().unwrap_or("<missing>"),
        compatibility,
    )
}

fn compare_compatibility(expected: &str, actual: &str) -> Result<(), String> {
    let expected = Snapshot::parse(expected)?;
    let actual = Snapshot::parse(actual)?;

    if expected.format != actual.format {
        return Err(format!(
            "snapshot format changed from {} to {}",
            expected.format, actual.format
        ));
    }
    if expected.package != actual.package {
        return Err(format!(
            "package changed from {} to {}",
            expected.package, actual.package
        ));
    }
    if expected.target != actual.target {
        return Err(format!(
            "target changed from {} to {}",
            expected.target, actual.target
        ));
    }

    let expected_contract = expected
        .contract
        .as_ref()
        .ok_or_else(|| "expected snapshot contract is missing".to_string())?;
    let actual_contract = actual
        .contract
        .as_ref()
        .ok_or_else(|| "actual snapshot contract is missing".to_string())?;
    if actual_contract.abi_id != expected_contract.abi_id {
        return Err(format!(
            "contract abi_id changed from {} to {}",
            expected_contract.abi_id, actual_contract.abi_id,
        ));
    }
    if actual_contract.abi_version != expected_contract.abi_version {
        return Err(format!(
            "contract {} abi_version changed from {} to {}",
            expected_contract.abi_id, expected_contract.abi_version, actual_contract.abi_version,
        ));
    }
    if actual_contract.rust_trait != expected_contract.rust_trait {
        return Err(format!(
            "contract {} rust trait changed from {} to {}",
            expected_contract.abi_id, expected_contract.rust_trait, actual_contract.rust_trait,
        ));
    }

    for (name, expected_ty) in &expected.types {
        let actual_ty = actual
            .types
            .get(name)
            .ok_or_else(|| format!("type {name} was removed"))?;
        if actual_ty.stability != expected_ty.stability {
            return Err(format!(
                "type {name} stability changed from {} to {}",
                expected_ty.stability.as_str(),
                actual_ty.stability.as_str(),
            ));
        }
        if actual_ty.align != expected_ty.align {
            return Err(format!(
                "type {name} alignment changed from {} to {}",
                expected_ty.align, actual_ty.align
            ));
        }
        match expected_ty.stability {
            XabiLayoutStability::Fixed => {
                if actual_ty.size != expected_ty.size {
                    return Err(format!(
                        "fixed type {name} size changed from {} to {}",
                        expected_ty.size, actual_ty.size
                    ));
                }
            }
            XabiLayoutStability::Prefix => {
                if actual_ty.size < expected_ty.size {
                    return Err(format!(
                        "prefix type {name} shrank from {} to {}",
                        expected_ty.size, actual_ty.size
                    ));
                }
            }
        }
        let actual_fields = actual_ty.field_map();
        for expected_field in &expected_ty.fields {
            let field_name = &expected_field.name;
            let actual_field = actual_ty
                .field_by_name(&actual_fields, field_name)
                .ok_or_else(|| format!("type {name} field {field_name} was removed"))?;
            if actual_field.offset != expected_field.offset || actual_field.ty != expected_field.ty
            {
                return Err(format!(
                    "type {name} field {field_name} changed from offset={} type={} to offset={} type={}",
                    expected_field.offset, expected_field.ty, actual_field.offset, actual_field.ty,
                ));
            }
        }
        if expected_ty.stability == XabiLayoutStability::Fixed
            && actual_ty.fields.len() != expected_ty.fields.len()
        {
            return Err(format!("fixed type {name} field set changed"));
        }
        if expected_ty.stability == XabiLayoutStability::Prefix {
            let expected_fields = expected_ty.field_map();
            for field in &actual_ty.fields {
                if !expected_fields.contains_key(field.name.as_str())
                    && field.offset < expected_ty.size
                {
                    return Err(format!(
                        "type {name} appended field {} at offset {} before old size {}",
                        field.name, field.offset, expected_ty.size
                    ));
                }
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
    package: String,
    target: String,
    contract: Option<ContractEntry>,
    types: BTreeMap<String, TypeEntry>,
    vtables: BTreeMap<String, VTableEntry>,
}

#[derive(Clone)]
struct ContractEntry {
    abi_id: String,
    abi_version: u32,
    rust_trait: String,
}

#[derive(Clone)]
struct TypeEntry {
    stability: XabiLayoutStability,
    size: usize,
    align: usize,
    fields: Vec<FieldEntry>,
}

#[derive(Clone)]
struct FieldEntry {
    name: String,
    offset: usize,
    ty: String,
}

impl TypeEntry {
    fn field_map(&self) -> BTreeMap<&str, &FieldEntry> {
        self.fields
            .iter()
            .map(|field| (field.name.as_str(), field))
            .collect()
    }

    fn field_by_name<'a>(
        &'a self,
        fields: &'a BTreeMap<&str, &FieldEntry>,
        name: &str,
    ) -> Option<&'a FieldEntry> {
        fields.get(name).copied()
    }
}

#[derive(Clone, Default)]
struct VTableEntry {
    full_size: usize,
    min_size: usize,
}

enum SnapshotEntry {
    Contract,
    Type(String),
    VTable(String),
}

impl Snapshot {
    fn from_layout(
        package: &str,
        contract: XabiContractLayout,
        target: &str,
        items: Vec<XabiLayoutItem>,
    ) -> Self {
        let mut snapshot = Self {
            format: "xabi-contract-snapshot-v1".to_string(),
            package: package.to_string(),
            contract: Some(ContractEntry {
                abi_id: contract.abi_id.to_string(),
                abi_version: contract.abi_version,
                rust_trait: contract.rust_trait.to_string(),
            }),
            target: target.to_string(),
            ..Self::default()
        };

        for item in items {
            match item {
                XabiLayoutItem::Type(ty) => snapshot.insert_type(ty),
                XabiLayoutItem::VTable(vtable) => snapshot.insert_vtable(vtable),
            }
        }

        snapshot
    }

    fn insert_type(&mut self, ty: XabiTypeLayout) {
        let entry = TypeEntry {
            stability: ty.stability,
            size: ty.size,
            align: ty.align,
            fields: ty
                .fields
                .iter()
                .map(|field| FieldEntry {
                    name: field.name.to_string(),
                    offset: field.offset,
                    ty: field.ty.to_string(),
                })
                .collect(),
        };
        if let Some(existing) = self.types.insert(ty.name.to_string(), entry.clone()) {
            assert_type_equal(ty.name, &existing, &entry);
        }
    }

    fn insert_vtable(&mut self, vtable: XabiVTableLayout) {
        let entry = VTableEntry {
            full_size: vtable.full_size,
            min_size: vtable.min_size,
        };
        if let Some(existing) = self.vtables.insert(vtable.name.to_string(), entry.clone()) {
            assert_vtable_equal(vtable.name, &existing, &entry);
        }
    }

    fn render(&self) -> String {
        let mut out = String::new();
        writeln!(out, "format={}", self.format).unwrap();
        writeln!(out, "package={}", self.package).unwrap();
        writeln!(out, "target={}", self.target).unwrap();
        writeln!(out).unwrap();

        if let Some(contract) = &self.contract {
            writeln!(out, "contract {}", contract.abi_id).unwrap();
            writeln!(out, "  abi_version={}", contract.abi_version).unwrap();
            writeln!(out, "  rust_trait={}", contract.rust_trait).unwrap();
            writeln!(out).unwrap();
        }

        for (name, ty) in &self.types {
            writeln!(out, "type {name}").unwrap();
            writeln!(out, "  stability={}", ty.stability.as_str()).unwrap();
            writeln!(out, "  size={}", ty.size).unwrap();
            writeln!(out, "  align={}", ty.align).unwrap();
            for field in &ty.fields {
                writeln!(
                    out,
                    "  field.{} offset={} type={}",
                    field.name, field.offset, field.ty
                )
                .unwrap();
            }
            writeln!(out).unwrap();
        }

        for (name, vtable) in &self.vtables {
            writeln!(out, "vtable {name}").unwrap();
            writeln!(out, "  full_size={}", vtable.full_size).unwrap();
            writeln!(out, "  min_size={}", vtable.min_size).unwrap();
            writeln!(out).unwrap();
        }

        if out.ends_with("\n\n") {
            out.pop();
        }

        out
    }

    fn parse(input: &str) -> Result<Self, String> {
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
            if let Some(package) = line.strip_prefix("package=") {
                snapshot.package = package.to_string();
                continue;
            }
            if let Some(target) = line.strip_prefix("target=") {
                snapshot.target = target.to_string();
                continue;
            }
            if let Some(abi_id) = line.strip_prefix("contract ") {
                if snapshot.contract.is_some() {
                    return Err("snapshot contains multiple contract entries".to_string());
                }
                snapshot.contract = Some(ContractEntry {
                    abi_id: abi_id.to_string(),
                    abi_version: 0,
                    rust_trait: String::new(),
                });
                entry = Some(SnapshotEntry::Contract);
                continue;
            }
            if let Some(name) = line.strip_prefix("type ") {
                snapshot.types.insert(
                    name.to_string(),
                    TypeEntry {
                        stability: XabiLayoutStability::Prefix,
                        size: 0,
                        align: 0,
                        fields: Vec::new(),
                    },
                );
                entry = Some(SnapshotEntry::Type(name.to_string()));
                continue;
            }
            if let Some(name) = line.strip_prefix("vtable ") {
                snapshot
                    .vtables
                    .insert(name.to_string(), VTableEntry::default());
                entry = Some(SnapshotEntry::VTable(name.to_string()));
                continue;
            }

            let Some(entry) = &entry else {
                return Err(format!("line outside snapshot entry: {line}"));
            };
            let trimmed = line.trim_start();
            match entry {
                SnapshotEntry::Contract => {
                    parse_contract_line(
                        snapshot
                            .contract
                            .as_mut()
                            .expect("contract entry exists while parsing"),
                        trimmed,
                    )?;
                }
                SnapshotEntry::Type(name) => {
                    parse_type_line(
                        snapshot
                            .types
                            .get_mut(name)
                            .expect("type entry exists while parsing"),
                        trimmed,
                    )?;
                }
                SnapshotEntry::VTable(name) => {
                    parse_vtable_line(
                        snapshot
                            .vtables
                            .get_mut(name)
                            .expect("vtable entry exists while parsing"),
                        trimmed,
                    )?;
                }
            }
        }

        if snapshot.format.is_empty() {
            return Err("snapshot format is missing".to_string());
        }
        if snapshot.package.is_empty() {
            return Err("snapshot package is missing".to_string());
        }
        if snapshot.target.is_empty() {
            return Err("snapshot target is missing".to_string());
        }
        let Some(contract) = snapshot.contract.as_ref() else {
            return Err("snapshot contract is missing".to_string());
        };
        if contract.rust_trait.is_empty() {
            return Err("snapshot contract rust_trait is missing".to_string());
        }
        Ok(snapshot)
    }
}

fn parse_contract_line(entry: &mut ContractEntry, line: &str) -> Result<(), String> {
    if let Some(version) = line.strip_prefix("abi_version=") {
        entry.abi_version = parse_u32(version, "contract ABI version")?;
        return Ok(());
    }
    if let Some(rust_trait) = line.strip_prefix("rust_trait=") {
        entry.rust_trait = rust_trait.to_string();
        return Ok(());
    }
    Err(format!("unsupported contract line: {line}"))
}

fn parse_type_line(layout: &mut TypeEntry, line: &str) -> Result<(), String> {
    if let Some(value) = line.strip_prefix("stability=") {
        layout.stability = parse_stability(value)?;
        return Ok(());
    }
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
    layout.fields.push(FieldEntry {
        name: name.to_string(),
        offset: parse_usize(offset, "field offset")?,
        ty: ty.to_string(),
    });
    Ok(())
}

fn parse_vtable_line(layout: &mut VTableEntry, line: &str) -> Result<(), String> {
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

fn parse_stability(value: &str) -> Result<XabiLayoutStability, String> {
    match value {
        "fixed" => Ok(XabiLayoutStability::Fixed),
        "prefix" => Ok(XabiLayoutStability::Prefix),
        other => Err(format!("unsupported type stability: {other}")),
    }
}

fn parse_usize(value: &str, context: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|err| format!("invalid {context} `{value}`: {err}"))
}

fn parse_u32(value: &str, context: &str) -> Result<u32, String> {
    value
        .parse()
        .map_err(|err| format!("invalid {context} `{value}`: {err}"))
}

fn assert_type_equal(name: &str, left: &TypeEntry, right: &TypeEntry) {
    assert!(
        left.stability == right.stability
            && left.size == right.size
            && left.align == right.align
            && left.fields.len() == right.fields.len()
            && left
                .fields
                .iter()
                .zip(&right.fields)
                .all(|(left, right)| left.name == right.name
                    && left.offset == right.offset
                    && left.ty == right.ty),
        "conflicting xabi type layout for {name}",
    );
}

fn assert_vtable_equal(name: &str, left: &VTableEntry, right: &VTableEntry) {
    assert!(
        left.full_size == right.full_size && left.min_size == right.min_size,
        "conflicting xabi vtable layout for {name}",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn append_only_prefix_change_is_compatible() {
        let expected = "\
format=xabi-contract-snapshot-v1
package=demo
target=test-target

contract demo.Contract
  abi_version=1
  rust_trait=demo::Contract

type demo::Wire
  stability=prefix
  size=16
  align=8
  field.size offset=0 type=usize

";
        let actual = "\
format=xabi-contract-snapshot-v1
package=demo
target=test-target

contract demo.Contract
  abi_version=1
  rust_trait=demo::Contract

type demo::Wire
  stability=prefix
  size=24
  align=8
  field.size offset=0 type=usize
  field.tail offset=16 type=u64

";

        compare_compatibility(expected, actual).expect("append-only change is compatible");
    }

    #[test]
    fn snapshot_component_keeps_contract_ids_path_safe() {
        assert_eq!(
            snapshot_component("lance.ScalarIndex/Plugin:v1"),
            "lance.ScalarIndex_Plugin_v1"
        );
    }
}
