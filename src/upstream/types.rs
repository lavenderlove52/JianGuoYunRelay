use bytes::Bytes;
use http::HeaderMap;

/// 上游 GET/HEAD 等返回的元数据与正文。
#[derive(Debug, Clone)]
pub struct UpstreamResponse {
    pub status: http::StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
}

impl UpstreamResponse {
    pub fn etag(&self) -> Option<&str> {
        self.headers
            .get(http::header::ETAG)
            .and_then(|v| v.to_str().ok())
    }

    pub fn last_modified(&self) -> Option<&str> {
        self.headers
            .get(http::header::LAST_MODIFIED)
            .and_then(|v| v.to_str().ok())
    }
}
