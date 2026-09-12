//! Rebuild when files under `migrations/` change.

fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
