// w3b:print

pub fn envelope_arm(
    e: &datum_print::Error,
) -> (&'static str, axum::http::StatusCode, Option<&str>, String) {
    (
        "INTERNAL",
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        None,
        e.to_string(),
    )
}
