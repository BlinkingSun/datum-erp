//! Compile-fail cases for D1 §8 rows a, d, e, f and PostingId vs PostingHandle.

#![allow(clippy::unwrap_used, clippy::expect_used, unused_crate_dependencies)]

#[test]
fn converted_cannot_be_read_without_exit() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/converted_cannot_be_read_without_exit.rs");
}

#[test]
fn unit_ref_is_the_only_constructor() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/unit_ref_is_the_only_constructor.rs");
}

#[test]
fn posting_id_is_not_a_handle() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/posting_id_is_not_a_handle.rs");
}

#[test]
fn length_plus_mass_does_not_compile() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/length_plus_mass.rs");
}

#[test]
fn money_plus_quantity_does_not_compile() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile_fail/money_plus_quantity.rs");
}
