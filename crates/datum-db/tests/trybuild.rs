//! Compile-fail: `Tx` is sealed (no `Deref` / `DerefMut`, no public constructor).

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

#[test]
fn tx_has_no_deref() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/tx_has_no_deref.rs");
}

#[test]
fn tx_has_no_public_constructor() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/tx_has_no_public_constructor.rs");
}

#[test]
fn read_pool_has_no_deref() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/read_pool_has_no_deref.rs");
}

#[test]
fn read_pool_has_no_execute() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/read_pool_has_no_execute.rs");
}
