use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Internal error wrapper for HTTP handlers.
///
/// This struct is used internally for error handling and is not part of the public API.
#[doc(hidden)]
pub struct Error(eyre::Error);

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: {}", self.0),
        )
            .into_response()
    }
}
