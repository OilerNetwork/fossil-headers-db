use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// Custom error type for the application.
/// Wraps the underlying error type and provides HTTP response conversion.
pub struct Error(eyre::Error);

impl IntoResponse for Error {
    /// Converts the error into an HTTP response.
    /// 
    /// # Returns
    /// * `Response` - HTTP response with internal server error status and error message
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: {}", self.0),
        )
            .into_response()
    }
}
