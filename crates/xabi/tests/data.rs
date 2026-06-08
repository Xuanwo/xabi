use xabi::XabiType;

#[xabi::data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataPoint {
    pub id: u32,
    pub count: u64,
}

#[test]
fn data_macro_generates_wire_type_and_roundtrips() -> xabi::Result<()> {
    let point = DataPoint::new(7, 42);
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
fn data_macro_rejects_invalid_wire_prefix() {
    let mut wire = DataPoint::new(7, 42).into_wire();
    wire.size = 0;

    let err = unsafe { DataPoint::from_wire(&wire) }.expect_err("invalid size must fail");
    assert!(err.to_string().contains("smaller than expected"));
}
