use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::State;
use axum::http::header::WWW_AUTHENTICATE;
use axum::http::Request;
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Response};
use axum::http::{Method, StatusCode};
use axum::routing::{any, get};
use axum::Router;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::auth;
use crate::error::AppError;
use crate::state::AppState;
use crate::upstream::NutstoreClient;
use crate::webdav::dispatch_vault;
use crate::config::Config;

/// 加载配置、初始化日志、启动 HTTP 服务（支持 Ctrl+C 优雅退出）。
pub async fn run() -> Result<(), String> {
    // 从项目根目录 `.env` 加载；**覆盖** shell 里已存在的同名变量，避免空 `export RELAY_*` 导致与 `.env` 不一致、长期 401。
    let _ = dotenvy::dotenv_override();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let config = Config::from_env().map_err(|e| e.to_string())?;
    tracing::info!(
        has_basic = config.relay_auth_user.is_some() && config.relay_auth_password.is_some(),
        has_bearer = config.relay_bearer_token.is_some(),
        user_len = config.relay_auth_user.as_deref().map(str::len),
        pass_len = config.relay_auth_password.as_deref().map(str::len),
        vault = %config.vault_path,
        "relay auth loaded (lengths only, no secrets)"
    );
    let nutstore = NutstoreClient::new(&config).map_err(|e| e.to_string())?;
    let state = AppState::new(config, nutstore);

    let max_body = state.config.max_body_bytes;

    let app = Router::new()
        .route("/health", get(health))
        .fallback(any(vault_gateway))
        // Axum：先 `.layer` 的为更外层。请求顺序：Trace -> 鉴权 -> body 限制 -> handler
        .layer(TraceLayer::new_for_http())
        .layer(from_fn_with_state(state.clone(), relay_auth_skip_health))
        .layer(axum::extract::DefaultBodyLimit::max(max_body))
        .with_state(state.clone());

    let addr: SocketAddr = state
        .config
        .listen_addr
        .parse()
        .map_err(|e: std::net::AddrParseError| e.to_string())?;

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("bind {addr}: {e}"))?;

    tracing::info!(%addr, vault = %state.config.vault_path, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!("shutdown complete");
    Ok(())
}

async fn relay_auth_skip_health(
    State(st): State<AppState>,
    req: Request<Body>,
    next: Next,
) -> Response {
    if req.uri().path() == "/health" {
        return next.run(req).await;
    }
    // WinHTTP / WebDAV 客户端（含部分 KeePass 环境）会先发 **不带** Authorization 的 OPTIONS，再带 Basic 发业务请求。
    if *req.method() == Method::OPTIONS {
        return next.run(req).await;
    }
    if !auth::validate_request(&st.config, req.headers()) {
        let mut resp = (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
        if let Some(challenge) = auth::www_authenticate_challenge(&st.config) {
            if let Ok(v) = challenge.parse() {
                resp.headers_mut().insert(WWW_AUTHENTICATE, v);
            }
        }
        return resp;
    }
    next.run(req).await
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received");
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}

async fn vault_gateway(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response, AppError> {
    if !state.config.request_is_vault_path(req.uri().path()) {
        return Ok(StatusCode::NOT_FOUND.into_response());
    }
    dispatch_vault(State(state), req).await
}
