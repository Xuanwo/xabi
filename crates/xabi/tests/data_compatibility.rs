use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};

use xabi::{XabiLayoutItem, XabiLayoutStability, XabiOwnedBytes, XabiType};

#[xabi::data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarV1 {
    value: u64,
}

#[xabi::data]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScalarV2 {
    value: u64,
    tail: u64,
}

#[xabi::data]
#[derive(Debug, PartialEq, Eq)]
struct OwnedV1 {
    value: String,
}

#[xabi::data]
#[derive(Debug, PartialEq, Eq)]
struct OwnedV2 {
    value: String,
    tail: String,
}

#[test]
fn generated_data_layouts_are_fixed() {
    let mut items = Vec::new();
    <ScalarV2 as XabiType>::collect_xabi_layout(&mut items);

    let layout = items
        .into_iter()
        .find_map(|item| match item {
            XabiLayoutItem::Type(layout) if layout.name.ends_with("::XabiV1DataScalarV2") => {
                Some(layout)
            }
            _ => None,
        })
        .expect("ScalarV2 layout is collected");

    assert_eq!(layout.stability, XabiLayoutStability::Fixed);
}

#[test]
fn direct_method_argument_decoding_rejects_scalar_tail_changes() {
    let new_wire = ScalarV2::new(7, 11).into_wire();
    let old_view =
        unsafe { &*(&new_wire as *const XabiV1DataScalarV2).cast::<XabiV1DataScalarV1>() };
    assert!(!old_view.field_available("value"));
    let err = unsafe { ScalarV1::from_wire(old_view) }
        .expect_err("old consumer must reject a larger wire");
    assert_exact_size_mismatch(err);

    let old_wire = ScalarV1::new(13).into_wire();
    let mut old_in_new_storage = ScalarV2::new(0, 0).into_wire();
    unsafe {
        std::ptr::copy_nonoverlapping(
            (&old_wire as *const XabiV1DataScalarV1).cast::<u8>(),
            (&mut old_in_new_storage as *mut XabiV1DataScalarV2).cast::<u8>(),
            std::mem::size_of::<XabiV1DataScalarV1>(),
        );
    }
    assert!(!old_in_new_storage.field_available("value"));
    let err = unsafe { ScalarV2::from_wire(&old_in_new_storage) }
        .expect_err("new consumer must reject a shorter wire");
    assert_exact_size_mismatch(err);
}

#[test]
fn direct_method_argument_rejection_does_not_consume_owned_fields() -> xabi::Result<()> {
    let new_wire = OwnedV2::new("known", "unknown tail").into_wire();
    let err = unsafe {
        OwnedV1::from_wire((&new_wire as *const XabiV1DataOwnedV2).cast::<XabiV1DataOwnedV1>())
    }
    .expect_err("old consumer must reject a larger ownership-bearing wire");
    assert_exact_size_mismatch(err);
    assert_eq!(
        unsafe { OwnedV2::from_wire(&new_wire) }?,
        OwnedV2::new("known", "unknown tail")
    );

    let old_wire = OwnedV1::new("known").into_wire();
    let mut old_in_new_storage = OwnedV2::new(String::new(), "cleanup tail").into_wire();
    unsafe {
        std::ptr::copy_nonoverlapping(
            (&old_wire as *const XabiV1DataOwnedV1).cast::<u8>(),
            (&mut old_in_new_storage as *mut XabiV1DataOwnedV2).cast::<u8>(),
            std::mem::size_of::<XabiV1DataOwnedV1>(),
        );
    }
    let err = unsafe { OwnedV2::from_wire(&old_in_new_storage) }
        .expect_err("new consumer must reject a shorter ownership-bearing wire");
    assert_exact_size_mismatch(err);

    old_in_new_storage.size = XabiV1DataOwnedV2::FULL_SIZE;
    assert_eq!(
        unsafe { OwnedV2::from_wire(&old_in_new_storage) }?,
        OwnedV2::new("known", "cleanup tail")
    );
    Ok(())
}

#[test]
fn owned_result_decoding_rejects_scalar_tail_changes() {
    let err = unsafe { ScalarV1::from_payload(ScalarV2::new(17, 19).into_payload()) }
        .expect_err("old consumer must reject a larger payload");
    assert_exact_size_mismatch(err);

    let err = unsafe { ScalarV2::from_payload(ScalarV1::new(23).into_payload()) }
        .expect_err("new consumer must reject a shorter payload");
    assert_exact_size_mismatch(err);
}

#[test]
fn owned_result_rejection_does_not_consume_nested_owners() -> xabi::Result<()> {
    let payload = OwnedV2::new("known", "unknown tail").into_payload();
    // Incompatible payloads are gated before transfer in production. Retain the
    // producer wire here so this negative decoder test can reclaim its owners.
    let retained_wire = unsafe { copy_payload_wire::<XabiV1DataOwnedV2>(&payload) }?;
    let err = unsafe { OwnedV1::from_payload(payload) }
        .expect_err("old consumer must reject a larger ownership-bearing payload");
    assert_exact_size_mismatch(err);
    assert_eq!(
        unsafe { OwnedV2::from_wire(&retained_wire) }?,
        OwnedV2::new("known", "unknown tail")
    );

    let payload = OwnedV1::new("known").into_payload();
    let retained_wire = unsafe { copy_payload_wire::<XabiV1DataOwnedV1>(&payload) }?;
    let err = unsafe { OwnedV2::from_payload(payload) }
        .expect_err("new consumer must reject a shorter ownership-bearing payload");
    assert_exact_size_mismatch(err);
    assert_eq!(
        unsafe { OwnedV1::from_wire(&retained_wire) }?,
        OwnedV1::new("known")
    );
    Ok(())
}

static V1_CALLS: AtomicUsize = AtomicUsize::new(0);
static V2_CALLS: AtomicUsize = AtomicUsize::new(0);

#[test]
fn contract_version_gate_precedes_cross_version_argument_and_result_transfer() {
    V1_CALLS.store(0, Ordering::SeqCst);
    V2_CALLS.store(0, Ordering::SeqCst);

    let v2 = contract_v2::XabiV1OwnedTraitService::new(contract_v2::Implementation);
    let err = unsafe {
        contract_v1::XabiV1BorrowedTraitService::xabi_from_vtable(
            v2.xabi_as_ptr()
                .cast::<contract_v1::XabiV1VtableTraitService>(),
        )
    }
    .expect_err("v1 consumer must reject a v2 producer before calling a method");
    assert!(
        err.to_string()
            .contains("abi_version 2 does not match expected 1")
    );

    let v1 = contract_v1::XabiV1OwnedTraitService::new(contract_v1::Implementation);
    let err = unsafe {
        contract_v2::XabiV1BorrowedTraitService::xabi_from_vtable(
            v1.xabi_as_ptr()
                .cast::<contract_v2::XabiV1VtableTraitService>(),
        )
    }
    .expect_err("v2 consumer must reject a v1 producer before calling a method");
    assert!(
        err.to_string()
            .contains("abi_version 1 does not match expected 2")
    );

    assert_eq!(V1_CALLS.load(Ordering::SeqCst), 0);
    assert_eq!(V2_CALLS.load(Ordering::SeqCst), 0);
}

fn assert_exact_size_mismatch(err: xabi::Error) {
    assert!(
        err.to_string().contains("does not match expected"),
        "unexpected error: {err}"
    );
}

unsafe fn copy_payload_wire<W: Copy>(payload: &XabiOwnedBytes) -> xabi::Result<W> {
    let bytes = unsafe { payload.to_vec() }?;
    assert_eq!(bytes.len(), std::mem::size_of::<W>());
    let mut wire = MaybeUninit::<W>::uninit();
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), wire.as_mut_ptr().cast::<u8>(), bytes.len());
        Ok(wire.assume_init())
    }
}

mod contract_v1 {
    use super::{Ordering, V1_CALLS};

    #[xabi::data]
    pub struct Scalar {
        pub value: u64,
    }

    #[xabi::data]
    pub struct Owned {
        pub value: String,
    }

    #[xabi::xabi(id = "xabi.test.DataCompatibility", version = 1)]
    pub trait Service {
        fn scalar(&self, input: Scalar) -> xabi::Result<Scalar>;
        fn owned(&self, input: Owned) -> xabi::Result<Owned>;
    }

    pub(super) struct Implementation;

    impl Service for Implementation {
        fn scalar(&self, input: Scalar) -> xabi::Result<Scalar> {
            V1_CALLS.fetch_add(1, Ordering::SeqCst);
            Ok(input)
        }

        fn owned(&self, input: Owned) -> xabi::Result<Owned> {
            V1_CALLS.fetch_add(1, Ordering::SeqCst);
            Ok(input)
        }
    }
}

mod contract_v2 {
    use super::{Ordering, V2_CALLS};

    #[xabi::data]
    pub struct Scalar {
        pub value: u64,
        pub tail: u64,
    }

    #[xabi::data]
    pub struct Owned {
        pub value: String,
        pub tail: String,
    }

    #[xabi::xabi(id = "xabi.test.DataCompatibility", version = 2)]
    pub trait Service {
        fn scalar(&self, input: Scalar) -> xabi::Result<Scalar>;
        fn owned(&self, input: Owned) -> xabi::Result<Owned>;
    }

    pub(super) struct Implementation;

    impl Service for Implementation {
        fn scalar(&self, input: Scalar) -> xabi::Result<Scalar> {
            V2_CALLS.fetch_add(1, Ordering::SeqCst);
            Ok(input)
        }

        fn owned(&self, input: Owned) -> xabi::Result<Owned> {
            V2_CALLS.fetch_add(1, Ordering::SeqCst);
            Ok(input)
        }
    }
}
