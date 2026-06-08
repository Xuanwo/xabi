fn main() -> xabi::Result<()> {
    let borrowed = xabi::XabiStr::from_static("xabi");
    let borrowed = unsafe { borrowed.as_str()? };
    assert_eq!(borrowed, "xabi");

    let bytes = xabi::XabiBytes::from_slice(b"payload");
    let bytes = unsafe { bytes.as_slice()? };
    assert_eq!(bytes, b"payload");

    let owned = xabi::XabiOwnedBytes::from_string("owned".to_string());
    let owned = unsafe { owned.to_string_and_free()? };
    assert_eq!(owned, "owned");

    Ok(())
}
