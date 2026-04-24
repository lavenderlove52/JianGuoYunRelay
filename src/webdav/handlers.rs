use axum::body::Body;
use axum::extract::State;
use axum::http::header::{ALLOW, CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_MATCH, LAST_MODIFIED};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use tracing::Instrument;

use crate::error::AppError;
use crate::state::AppState;
use crate::upstream::{filter_forward_headers, UpstreamResponse};
use crate::version_guard::{pick_baselines, pre_put_check};

use super::propfind::propfind_multistatus;

/// 对 `vault_path` 的统一入口：按 HTTP 方法分发。
pub async fn dispatch_vault(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response, AppError> {
    let (parts, body) = req.into_parts();
    let method = parts.method.clone();
    let headers = parts.headers;

    let span = tracing::info_span!("webdav", %method);
    async move {
        match method {
            Method::OPTIONS => Ok(options_response(&state)),
            Method::HEAD => handle_head(&state).await,
            Method::GET => handle_get(&state).await,
            Method::PUT => handle_put(&state, headers, body).await,
            m if m.as_str().eq_ignore_ascii_case("PROPFIND") => handle_propfind(&state, body).await,
            _ => Ok(StatusCode::METHOD_NOT_ALLOWED.into_response()),
        }
    }
    .instrument(span)
    .await
}

fn options_response(_state: &AppState) -> Response {
    let allow = "OPTIONS, GET, HEAD, PUT, PROPFIND";
    Response::builder()
        .status(StatusCode::OK)
        .header(ALLOW, allow)
        .header("DAV", "1")
        .body(Body::empty())
        .unwrap()
}

async fn handle_head(state: &AppState) -> Result<Response, AppError> {
    let res = load_metadata_response(state).await?;
    if !res.status.is_success() {
        return Ok(Response::builder()
            .status(res.status)
            .body(Body::empty())
            .unwrap());
    }
    Ok(metadata_only_response(StatusCode::OK, &res.headers, Body::from(res.body)))
}

async fn handle_get(state: &AppState) -> Result<Response, AppError> {
    let res = state.nutstore.get().await?;
    if !res.status.is_success() {
        return Ok(Response::builder()
            .status(res.status)
            .body(Body::from(res.body))
            .unwrap());
    }
    let mut rb = Response::builder().status(StatusCode::OK);
    let fwd = filter_forward_headers(&res.headers);
    for name in [ETAG, CONTENT_LENGTH, CONTENT_TYPE, LAST_MODIFIED].iter() {
        if let Some(v) = fwd.get(name) {
            rb = rb.header(name.clone(), v.clone());
        }
    }
    Ok(rb.body(Body::from(res.body)).expect("response build"))
}

async fn handle_put(
    state: &AppState,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, AppError> {
    let baselines = pick_baselines(&headers);

    if baselines.is_none() && state.config.require_put_baseline {
        return Err(AppError::PreconditionRequired);
    }

    if let Some(ref list) = baselines {
        pre_put_check(&state.nutstore, list).await?;
    }

    let bytes = body
        .collect()
        .await
        .map_err(|_| AppError::BadRequest)?
        .to_bytes();

    let if_match = upstream_if_match(&headers);
    let put_res = state.nutstore.put(bytes, if_match.as_deref()).await?;

    if !put_res.status.is_success() {
        return Ok(Response::builder()
            .status(put_res.status)
            .body(Body::from(put_res.body))
            .unwrap());
    }

    let mut rb = Response::builder().status(StatusCode::NO_CONTENT);
    if let Some(etag) = put_res.headers.get(ETAG) {
        rb = rb.header(ETAG, etag.clone());
    }
    if let Some(lm) = put_res.headers.get(LAST_MODIFIED) {
        rb = rb.header(LAST_MODIFIED, lm.clone());
    }
    Ok(rb.body(Body::empty()).unwrap())
}

/// 转发给坚果云的 `If-Match`：优先请求里的 `If-Match`，否则用 `X-Base-ETag`。
fn upstream_if_match(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get(IF_MATCH).and_then(|h| h.to_str().ok()) {
        let v = v.trim();
        if !v.is_empty() && v != "*" {
            return Some(v.to_string());
        }
    }
    headers
        .get("X-Base-ETag")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn handle_propfind(state: &AppState, body: Body) -> Result<Response, AppError> {
    let _ = body
        .collect()
        .await
        .map_err(|_| AppError::BadRequest)?;
    let res = load_metadata_response(state).await?;
    if !res.status.is_success() {
        return Ok(Response::builder()
            .status(res.status)
            .body(Body::empty())
            .unwrap());
    }
    let href = state.config.vault_path.clone();
    let name = href
        .rsplit('/')
        .find(|seg| !seg.is_empty())
        .unwrap_or(href.trim_start_matches('/'))
        .to_string();
    let xml = propfind_multistatus(
        &href,
        &name,
        res.headers.get(CONTENT_LENGTH).and_then(|v| v.to_str().ok()),
        res.headers.get(ETAG).and_then(|v| v.to_str().ok()),
        res.headers.get(LAST_MODIFIED).and_then(|v| v.to_str().ok()),
    );
    Ok(Response::builder()
        .status(StatusCode::MULTI_STATUS)
        .header(CONTENT_TYPE, "application/xml; charset=utf-8")
        .body(Body::from(xml))
        .unwrap())
}

async fn load_metadata_response(state: &AppState) -> Result<UpstreamResponse, AppError> {
    let head = state.nutstore.head().await?;
    if !head.status.is_success() {
        return Ok(head);
    }
    if metadata_complete(&head.headers) {
        return Ok(head);
    }
    let get = state.nutstore.get().await?;
    if get.status.is_success() {
        return Ok(get);
    }
    Ok(head)
}

fn metadata_complete(headers: &HeaderMap) -> bool {
    [CONTENT_LENGTH, CONTENT_TYPE, ETAG, LAST_MODIFIED]
        .iter()
        .all(|name| headers.get(name).is_some())
}

fn metadata_only_response(status: StatusCode, headers: &HeaderMap, body: Body) -> Response {
    let fwd = filter_forward_headers(headers);
    let mut resp = Response::builder()
        .status(status)
        .body(body)
        .unwrap();
    for name in [ETAG, CONTENT_LENGTH, CONTENT_TYPE, LAST_MODIFIED].iter() {
        if let Some(v) = fwd.get(name) {
            resp.headers_mut().insert(name.clone(), v.clone());
        }
    }
    resp
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderValue;

    use super::*;

    #[test]
    fn head_metadata_response_keeps_upstream_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("24439"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
        headers.insert(ETAG, HeaderValue::from_static("\"etag-1\""));
        headers.insert(LAST_MODIFIED, HeaderValue::from_static("Fri, 24 Apr 2026 06:00:50 GMT"));

        let resp = metadata_only_response(StatusCode::OK, &headers, Body::from(vec![0_u8; 24439]));

        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers().get(CONTENT_LENGTH).unwrap(), "24439");
        assert_eq!(resp.headers().get(CONTENT_TYPE).unwrap(), "application/octet-stream");
        assert_eq!(resp.headers().get(ETAG).unwrap(), "\"etag-1\"");
        assert_eq!(
            resp.headers().get(LAST_MODIFIED).unwrap(),
            "Fri, 24 Apr 2026 06:00:50 GMT"
        );
    }

    #[test]
    fn metadata_complete_requires_common_file_headers() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_LENGTH, HeaderValue::from_static("24439"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
        headers.insert(LAST_MODIFIED, HeaderValue::from_static("Fri, 24 Apr 2026 06:00:50 GMT"));
        assert!(!metadata_complete(&headers));

        headers.insert(ETAG, HeaderValue::from_static("\"etag-1\""));
        assert!(metadata_complete(&headers));
    }
}
