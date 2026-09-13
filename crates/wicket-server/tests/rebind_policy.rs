//! R-2s-7 t0: server Rust must never rebind `wicket.*` session settings.

#![allow(unused_crate_dependencies)]

#[test]
fn server_rust_never_rebinds_wicket_session_gucs() {
    for path in ["src/handlers/mod.rs", "src/boot.rs", "src/session.rs"] {
        let src =
            std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
                .unwrap_or_else(|e| panic!("read {path}: {e}"));
        assert!(
            !src.contains("rebind_write"),
            "{path} must not rebind wicket.* session settings"
        );
        let guc_set = format!("{}('wicket.", concat!("set", "_config"));
        assert!(
            !src.contains(&guc_set),
            "{path} must not set wicket.* GUCs from Rust"
        );
    }
}
