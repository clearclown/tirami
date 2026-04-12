/// Errors returned by the Forge SDK.
#[derive(Debug, thiserror::Error)]
pub enum SdkError {
    /// An underlying HTTP transport error (connection refused, timeout, etc.).
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// JSON deserialisation failed.
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// The server returned a 4xx/5xx status code.
    #[error("API error ({status}): {message}")]
    Api { status: u16, message: String },
}
