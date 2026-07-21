#[test]
fn unsupported_shapes_report_xabi_diagnostics() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/unsupported_shapes/*.rs");
}

#[test]
fn borrowed_handles_cannot_escape_their_owner() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/borrowed_handles/*.rs");
}
