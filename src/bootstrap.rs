use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::State;
use axum::http::Request;
use axum::middleware::{from_fn_with_state, Next};
use axum::response::{IntoResponse, Response};
use axum::http::StatusCode;
use axum::routing::{any, get};
use axum::Router;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::auth;
use crate::state::AppState;
use crate::upstream::NutstoreClient;
use crate::webdav::dispatch_vault;
use crate::config::Config;

/// 加载配置、初始化日志、启动 HTTP 服务（支持 Ctrl+C 优雅退出）。
pub async fn run() -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    let config = Config::from_env().map_err(|e| e.to_string())?;
    let nutstore = NutstoreClient::new(&config).map_err(|e| e.to_string())?;
    let state = AppState::new(config, nutstore);

    let vault_path = state.config.vault_path.clone();
    let max_body = state.config.max_body_bytes;

    let app = Router::new()
        .route("/health", get(health))
        .route(&vault_path, any(dispatch_vault))
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
    if !auth::validate_request(&st.config, req.headers()) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
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
