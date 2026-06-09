use xabi::XabiType;

#[xabi::data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataPoint {
    pub id: u32,
    pub count: u64,
}

#[xabi::data]
#[derive(Debug, PartialEq, Eq)]
pub struct DataEnvelope {
    pub point: DataPoint,
    pub label: String,
    pub payload: Vec<u8>,
    pub enabled: bool,
    pub slots: usize,
}

#[xabi::data]
#[derive(Debug, PartialEq, Eq)]
pub struct DataError {
    pub message: String,
}

#[xabi::data]
#[derive(Debug, PartialEq, Eq)]
pub struct OptionalData {
    pub label: Option<String>,
    pub payload: Option<Vec<u8>>,
}

#[xabi::opaque]
#[derive(Clone, Copy, Debug)]
pub struct OpaqueCounter {
    raw: *mut u32,
}

unsafe impl Send for OpaqueCounter {}

#[test]
fn data_macro_generates_wire_type_and_roundtrips() -> xabi::Result<()> {
    let point = DataPoint::new(7_u32, 42_u64);
    let wire = point.into_wire();

    assert_eq!(wire.size, std::mem::size_of::<XabiV1DataDataPoint>());
    assert_eq!(wire.abi_version, xabi::ABI_VERSION);
    wire.validate()?;

    let decoded = unsafe { DataPoint::from_wire(&wire) }?;
    assert_eq!(decoded, point);

    let payload = point.into_payload();
    let decoded = unsafe { DataPoint::from_payload(payload) }?;
    assert_eq!(decoded, point);

    Ok(())
}

#[test]
fn data_macro_lowers_nested_xabi_fields_and_owned_payloads() -> xabi::Result<()> {
    let envelope = DataEnvelope::new(
        DataPoint::new(9_u32, 11_u64),
        "nested",
        vec![1, 2, 3],
        true,
        4_usize,
    );
    let wire = envelope.into_wire();

    assert_eq!(wire.size, std::mem::size_of::<XabiV1DataDataEnvelope>());
    assert_eq!(wire.abi_version, xabi::ABI_VERSION);
    assert_eq!(wire.point.id, 9);
    assert_eq!(wire.enabled, 1);
    wire.validate()?;

    let decoded = unsafe { DataEnvelope::from_wire(&wire) }?;
    assert_eq!(
        decoded,
        DataEnvelope {
            point: DataPoint { id: 9, count: 11 },
            label: "nested".to_string(),
            payload: vec![1, 2, 3],
            enabled: true,
            slots: 4,
        }
    );

    let payload = DataEnvelope::new(
        DataPoint::new(1_u32, 2_u64),
        "payload",
        vec![8, 13],
        false,
        21_usize,
    )
    .into_payload();
    let decoded = unsafe { DataEnvelope::from_payload(payload) }?;
    assert_eq!(decoded.label, "payload");
    assert_eq!(decoded.payload, vec![8, 13]);
    assert!(!decoded.enabled);

    Ok(())
}

#[test]
fn data_macro_can_back_error_payloads() -> xabi::Result<()> {
    let err = DataError::new("failed");
    let payload = err.into_payload();
    let decoded = unsafe { DataError::from_payload(payload) }?;

    assert_eq!(decoded.message, "failed");
    Ok(())
}

#[test]
fn option_xabi_type_preserves_empty_some_values() -> xabi::Result<()> {
    let value = OptionalData::new(Some(String::new()), Some(Vec::new()));
    let decoded = unsafe { OptionalData::from_payload(value.into_payload()) }?;

    assert_eq!(decoded.label, Some(String::new()));
    assert_eq!(decoded.payload, Some(Vec::new()));

    let value = OptionalData::new(None, None);
    let decoded = unsafe { OptionalData::from_payload(value.into_payload()) }?;

    assert_eq!(decoded.label, None);
    assert_eq!(decoded.payload, None);
    Ok(())
}

#[test]
fn opaque_macro_generates_non_null_pointer_handle() -> xabi::Result<()> {
    let mut counter = 7_u32;
    let handle = unsafe { OpaqueCounter::from_raw(&mut counter) }?;
    let wire = handle.into_wire();

    assert_eq!(wire.size, std::mem::size_of::<XabiV1OpaqueOpaqueCounter>());
    assert_eq!(wire.abi_version, xabi::ABI_VERSION);
    assert_eq!(wire.raw, &mut counter as *mut u32);
    wire.validate()?;

    let decoded = unsafe { OpaqueCounter::from_wire(&wire) }?;
    unsafe {
        *decoded.as_raw() = 11;
    }
    assert_eq!(counter, 11);

    Ok(())
}

#[test]
fn opaque_macro_rejects_null_pointer() {
    let err = unsafe { OpaqueCounter::from_raw(std::ptr::null_mut()) }
        .expect_err("null opaque handle must fail");

    assert!(err.to_string().contains("OpaqueCounter::raw"));
}

#[test]
fn data_macro_rejects_invalid_wire_prefix() {
    let mut wire = DataPoint::new(7_u32, 42_u64).into_wire();
    wire.size = 0;

    let err = unsafe { DataPoint::from_wire(&wire) }.expect_err("invalid size must fail");
    assert!(err.to_string().contains("smaller than expected"));
}
