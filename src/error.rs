use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::upstream::UpstreamError;
use crate::version_guard::VersionGuardError;

/// 映射为 HTTP 响应的应用错误（不泄露内部细节到 body）。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("precondition required: missing If-Match / X-Base-ETag")]
    PreconditionRequired,
    #[error("bad request")]
    BadRequest,
    #[error("upstream error")]
    Upstream(#[from] UpstreamError),
    #[error("version guard")]
    VersionGuard(#[from] VersionGuardError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::PreconditionRequired => (StatusCode::PRECONDITION_REQUIRED, "precondition required"),
            AppError::BadRequest => (StatusCode::BAD_REQUEST, "bad request"),
            AppError::Upstream(e) => (e.status_or_internal(), "upstream error"),
            AppError::VersionGuard(VersionGuardError::BaselineMismatch { .. }) => {
                (StatusCode::PRECONDITION_FAILED, "version mismatch")
            }
            AppError::VersionGuard(VersionGuardError::UpstreamEtagMissing) => {
                (StatusCode::BAD_GATEWAY, "upstream etag missing")
            }
            AppError::VersionGuard(VersionGuardError::UpstreamStatus(s)) => (*s, "upstream status"),
            AppError::VersionGuard(VersionGuardError::Upstream(e)) => (e.status_or_internal(), "upstream error"),
        };
        tracing::warn!(error = %self, status = %status, "request error");
        (status, msg).into_response()
    }
}
