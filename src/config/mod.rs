//! 从环境变量加载配置（对齐设计说明 §8）。

mod validate;

use std::time::Duration;

pub use validate::validate_host;
use validate::{build_resource_url, parse_duration_secs, parse_usize};

/// 运行时可读配置（已解析、只读）。
#[derive(Debug, Clone)]
pub struct Config {
    pub listen_addr: String,
    /// 对外暴露的「金库」路径（M2：固定单 URL），如 `/vault.kdbx`
    pub vault_path: String,
    pub jgy_webdav_root: url::Url,
    pub jgy_username: String,
    pub jgy_app_password: String,
    pub jgy_remote_path: String,
    /// 拼接后的坚果云资源 URL（唯一受控对象）
    pub jgy_resource_url: url::Url,
    pub relay_auth_user: Option<String>,
    pub relay_auth_password: Option<String>,
    pub relay_bearer_token: Option<String>,
    pub max_body_bytes: usize,
    pub connect_timeout: Duration,
    pub upstream_timeout: Duration,
    /// PUT 必须带 `If-Match` 或 `X-Base-ETag` 之一（防无基准覆盖）
    pub require_put_baseline: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("missing or invalid environment variable: {0}")]
    Env(&'static str),
    #[error("invalid URL for {0}: {1}")]
    Url(&'static str, String),
    #[error("relay auth: set RELAY_AUTH_USER+RELAY_AUTH_PASSWORD and/or RELAY_BEARER_TOKEN")]
    RelayAuth,
    #[error("host {0} is not in allowed list for upstream WebDAV")]
    HostNotAllowed(String),
    #[error("failed to build upstream resource URL: {0}")]
    ResourceUrl(String),
}

impl Config {
    /// 从环境变量加载；非法或缺失时返回错误（fail-fast）。
    pub fn from_env() -> Result<Self, ConfigError> {
        fn get(key: &'static str) -> Result<String, ConfigError> {
            std::env::var(key).map_err(|_| ConfigError::Env(key))
        }

        let listen_addr = std::env::var("LISTEN_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
        let vault_path = std::env::var("RELAY_VAULT_PATH").unwrap_or_else(|_| "/vault.kdbx".into());
        if !vault_path.starts_with('/') {
            return Err(ConfigError::Env("RELAY_VAULT_PATH (must start with /)"));
        }

        let jgy_webdav_root_str = get("JGY_WEBDAV_ROOT")?;
        let jgy_webdav_root = url::Url::parse(jgy_webdav_root_str.trim())
            .map_err(|e| ConfigError::Url("JGY_WEBDAV_ROOT", e.to_string()))?;
        validate_host(&jgy_webdav_root)?;

        let jgy_username = get("JGY_USERNAME")?;
        let jgy_app_password = get("JGY_APP_PASSWORD")?;
        let jgy_remote_path = get("JGY_REMOTE_PATH")?;
        if !jgy_remote_path.starts_with('/') {
            return Err(ConfigError::Env("JGY_REMOTE_PATH (must start with /)"));
        }

        let jgy_resource_url =
            build_resource_url(&jgy_webdav_root, &jgy_remote_path).map_err(ConfigError::ResourceUrl)?;

        let relay_auth_user = std::env::var("RELAY_AUTH_USER").ok().filter(|s| !s.is_empty());
        let relay_auth_password = std::env::var("RELAY_AUTH_PASSWORD").ok().filter(|s| !s.is_empty());
        let relay_bearer_token = std::env::var("RELAY_BEARER_TOKEN").ok().filter(|s| !s.is_empty());

        let has_basic = relay_auth_user.is_some() && relay_auth_password.is_some();
        let has_bearer = relay_bearer_token.is_some();
        if !has_basic && !has_bearer {
            return Err(ConfigError::RelayAuth);
        }
        if relay_auth_user.is_some() != relay_auth_password.is_some() {
            return Err(ConfigError::RelayAuth);
        }

        let max_body_bytes = parse_usize(&get("MAX_BODY_BYTES")?, "MAX_BODY_BYTES")?;
        if max_body_bytes == 0 || max_body_bytes > 256 * 1024 * 1024 {
            return Err(ConfigError::Env("MAX_BODY_BYTES (must be 1..=268435456)"));
        }

        let connect_timeout = parse_duration_secs(
            &std::env::var("CONNECT_TIMEOUT_SECS").unwrap_or_else(|_| "10".into()),
            "CONNECT_TIMEOUT_SECS",
        )?;
        let upstream_timeout = parse_duration_secs(
            &std::env::var("UPSTREAM_TIMEOUT_SECS").unwrap_or_else(|_| "120".into()),
            "UPSTREAM_TIMEOUT_SECS",
        )?;

        let require_put_baseline = std::env::var("RELAY_REQUIRE_PUT_BASELINE")
            .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
            .unwrap_or(true);

        Ok(Self {
            listen_addr,
            vault_path,
            jgy_webdav_root,
            jgy_username,
            jgy_app_password,
            jgy_remote_path,
            jgy_resource_url,
            relay_auth_user,
            relay_auth_password,
            relay_bearer_token,
            max_body_bytes,
            connect_timeout,
            upstream_timeout,
            require_put_baseline,
        })
    }
}
