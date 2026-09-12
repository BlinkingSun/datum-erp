//! Rerun migrations when SQL changes.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
