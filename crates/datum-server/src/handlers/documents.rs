// w3b:documents

pub fn envelope_arm(
    e: &datum_documents::Error,
) -> (&'static str, axum::http::StatusCode, Option<&str>, String) {
    (
        "INTERNAL",
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        None,
        e.to_string(),
    )
}
