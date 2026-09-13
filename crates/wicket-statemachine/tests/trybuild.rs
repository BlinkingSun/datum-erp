//! Compile-fail: [`wicket_statemachine::SignatureDeclaration`] has no `Default`.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

#[test]
fn signature_declaration_has_no_default() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/signature_declaration_no_default.rs");
}
