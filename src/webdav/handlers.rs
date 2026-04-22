use axum::body::Body;
use axum::extract::State;
use axum::http::header::{ALLOW, CONTENT_TYPE, ETAG, IF_MATCH, LAST_MODIFIED};
use axum::http::{HeaderMap, Method, Request, Response, StatusCode};
use axum::response::IntoResponse;
use http_body_util::BodyExt;
use tracing::Instrument;

use crate::error::AppError;
use crate::state::AppState;
use crate::upstream::filter_forward_headers;
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

fn options_response(state: &AppState) -> Response {
    let allow = "OPTIONS, GET, HEAD, PUT, PROPFIND";
    Response::builder()
        .status(StatusCode::OK)
        .header(ALLOW, allow)
        .header("DAV", "1")
        .body(Body::empty())
        .unwrap()
}

async fn handle_head(state: &AppState) -> Result<Response, AppError> {
    let res = state.nutstore.head().await?;
    if !res.status.is_success() {
        return Ok(Response::builder()
            .status(res.status)
            .body(Body::empty())
            .unwrap());
    }
    let mut rb = Response::builder().status(StatusCode::OK);
    for (k, v) in filter_forward_headers(&res.headers) {
        rb = rb.header(k, v);
    }
    Ok(rb.body(Body::empty()).unwrap())
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
    for (k, v) in filter_forward_headers(&res.headers) {
        rb = rb.header(k, v);
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
    let href = state.config.vault_path.clone();
    let name = href.trim_start_matches('/').to_string();
    let xml = propfind_multistatus(&href, &name);
    Ok(Response::builder()
        .status(StatusCode::MULTI_STATUS)
        .header(CONTENT_TYPE, "application/xml; charset=utf-8")
        .body(Body::from(xml))
        .unwrap())
}
