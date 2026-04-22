//! 坚果云 WebDAV 上游 HTTP 客户端。

mod client;
mod types;

pub use client::{filter_forward_headers, NutstoreClient};
pub use types::UpstreamResponse;

use axum::http::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum UpstreamError {
    #[error("failed to build HTTP client: {0}")]
    Build(String),
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("upstream returned {0}")]
    Status(http::StatusCode),
}

impl UpstreamError {
    pub fn status_or_internal(&self) -> StatusCode {
        match self {
            UpstreamError::Status(s) => *s,
            UpstreamError::Transport(e) => {
                if e.is_timeout() {
                    StatusCode::GATEWAY_TIMEOUT
                } else {
                    StatusCode::BAD_GATEWAY
                }
            }
            UpstreamError::Build(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}
