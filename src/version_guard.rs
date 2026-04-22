//! PUT 前版本预检（设计 §7.3 层 B）：HEAD 取当前 ETag，与客户端基准比对。

use axum::http::{HeaderMap, StatusCode};
use http::header::IF_MATCH;

use crate::upstream::NutstoreClient;
use crate::upstream::UpstreamError;

#[derive(Debug, thiserror::Error)]
pub enum VersionGuardError {
    #[error(transparent)]
    Upstream(#[from] UpstreamError),
    #[error("upstream returned {0}")]
    UpstreamStatus(StatusCode),
    #[error("upstream ETag missing; cannot enforce version guard")]
    UpstreamEtagMissing,
    #[error("version baseline mismatch (current={current:?}, expected one of {expected:?})")]
    BaselineMismatch { current: String, expected: Vec<String> },
}

/// 规范化 ETag：去弱前缀、去引号、trim。
pub fn normalize_etag(raw: &str) -> String {
    let s = raw.trim();
    let s = s.strip_prefix("W/").unwrap_or(s).trim();
    s.trim_matches('"').to_string()
}

/// 从请求头提取基准 ETag 列表：`If-Match`（逗号分隔）优先，否则 `X-Base-ETag`。
/// `If-Match: *` 返回 `None`（本服务不将其视为可比对基准）。
pub fn pick_baselines(headers: &HeaderMap) -> Option<Vec<String>> {
    if let Some(im) = headers.get(IF_MATCH).and_then(|v| v.to_str().ok()) {
        let im = im.trim();
        if im == "*" {
            return None;
        }
        let list: Vec<String> = im
            .split(',')
            .map(|t| normalize_etag(t))
            .filter(|t| !t.is_empty())
            .collect();
        if !list.is_empty() {
            return Some(list);
        }
    }
    if let Some(x) = headers
        .get("X-Base-ETag")
        .and_then(|v| v.to_str().ok())
        .map(|s| normalize_etag(s))
    {
        if !x.is_empty() {
            return Some(vec![x]);
        }
    }
    None
}

/// 层 B：远端当前 ETag 必须在 `baselines` 中，否则失败（不写 PUT）。
pub async fn pre_put_check(
    nutstore: &NutstoreClient,
    baselines: &[String],
) -> Result<(), VersionGuardError> {
    if baselines.is_empty() {
        return Ok(());
    }

    let head = nutstore.head().await?;
    if !head.status.is_success() {
        return Err(VersionGuardError::UpstreamStatus(head.status));
    }
    let etag = head
        .etag()
        .ok_or(VersionGuardError::UpstreamEtagMissing)?;
    let current = normalize_etag(etag);

    if !baselines.iter().any(|b| b == &current) {
        return Err(VersionGuardError::BaselineMismatch {
            current,
            expected: baselines.to_vec(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_weak_and_quotes() {
        assert_eq!(normalize_etag(r#"W/"abc""#), "abc");
        assert_eq!(normalize_etag(r#""x""#), "x");
    }
}
