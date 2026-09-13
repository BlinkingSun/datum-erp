//! Rebuild when files under `migrations/` or `templates/` change.

fn main() {
    println!("cargo:rerun-if-changed=migrations");
    println!("cargo:rerun-if-changed=templates");
}
