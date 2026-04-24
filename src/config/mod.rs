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

        /// 去掉首尾空白与 Windows 行尾 `\r`，避免 `.env` 复制粘贴导致 KeePass 与服务端不一致。
        fn get_trimmed(key: &'static str) -> Result<String, ConfigError> {
            let s = get(key)?
                .trim()
                .trim_end_matches('\r')
                .to_string();
            if s.is_empty() {
                return Err(ConfigError::Env(key));
            }
            Ok(s)
        }

        fn opt_trimmed(key: &str) -> Option<String> {
            std::env::var(key).ok().map(|s| {
                s.trim()
                    .trim_end_matches('\r')
                    .to_string()
            }).filter(|s| !s.is_empty())
        }

        /// `.env` 里常见 `KEY="value"`；去掉一层成对 `"` 或 `'`，与 curl/KeePass 输入一致。
        fn strip_optional_wrapping_quotes(s: &str) -> String {
            let t = s.trim().trim_end_matches('\r');
            if t.len() >= 2 {
                let b = t.as_bytes();
                let first = b[0];
                let last = b[t.len() - 1];
                if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
                    return t[1..t.len() - 1]
                        .trim()
                        .trim_end_matches('\r')
                        .to_string();
                }
            }
            t.to_string()
        }

        fn opt_relay(key: &str) -> Option<String> {
            opt_trimmed(key).map(|s| strip_optional_wrapping_quotes(&s)).filter(|s| !s.is_empty())
        }

        let listen_addr = std::env::var("LISTEN_ADDR")
            .unwrap_or_else(|_| "0.0.0.0:8080".into())
            .trim()
            .trim_end_matches('\r')
            .to_string();
        let vault_path = std::env::var("RELAY_VAULT_PATH")
            .unwrap_or_else(|_| "/vault.kdbx".into())
            .trim()
            .trim_end_matches('\r')
            .to_string();
        if !vault_path.starts_with('/') {
            return Err(ConfigError::Env("RELAY_VAULT_PATH (must start with /)"));
        }

        let jgy_webdav_root_str = get_trimmed("JGY_WEBDAV_ROOT")?;
        let jgy_webdav_root = url::Url::parse(jgy_webdav_root_str.trim())
            .map_err(|e| ConfigError::Url("JGY_WEBDAV_ROOT", e.to_string()))?;
        validate_host(&jgy_webdav_root)?;

        let jgy_username = strip_optional_wrapping_quotes(&get_trimmed("JGY_USERNAME")?);
        let jgy_app_password = strip_optional_wrapping_quotes(&get_trimmed("JGY_APP_PASSWORD")?);
        let jgy_remote_path = get_trimmed("JGY_REMOTE_PATH")?;
        if !jgy_remote_path.starts_with('/') {
            return Err(ConfigError::Env("JGY_REMOTE_PATH (must start with /)"));
        }

        let jgy_resource_url =
            build_resource_url(&jgy_webdav_root, &jgy_remote_path).map_err(ConfigError::ResourceUrl)?;

        let relay_auth_user = opt_relay("RELAY_AUTH_USER");
        let relay_auth_password = opt_relay("RELAY_AUTH_PASSWORD");
        let relay_bearer_token = opt_relay("RELAY_BEARER_TOKEN");

        let has_basic = relay_auth_user.is_some() && relay_auth_password.is_some();
        let has_bearer = relay_bearer_token.is_some();
        if !has_basic && !has_bearer {
            return Err(ConfigError::RelayAuth);
        }
        if relay_auth_user.is_some() != relay_auth_password.is_some() {
            return Err(ConfigError::RelayAuth);
        }

        let max_body_bytes = parse_usize(&get_trimmed("MAX_BODY_BYTES")?, "MAX_BODY_BYTES")?;
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

    /// 判断请求 URI 的 path（可能为 `%XX` 编码或 UTF-8 字面量）是否与 `RELAY_VAULT_PATH` 指向同一金库。
    ///
    /// 部分 curl 会对中文发 `%e6...`；本机旧 curl 可能发裸 UTF-8。归一化时不能只用 `Url::path()` 字面量（编码大小写/是否已编码会不一致）。
    pub fn request_is_vault_path(&self, uri_path: &str) -> bool {
        match (
            Self::normalize_path_for_match(uri_path),
            Self::normalize_path_for_match(&self.vault_path),
        ) {
            (Some(a), Some(b)) => a == b,
            _ => uri_path == self.vault_path.as_str(),
        }
    }

    /// 先 `Url::parse` 再对 `path()` 做 `%HH` 解码：同一资源在「裸 UTF-8」与「已编码」下 `path()` 字面量不一致，不能直接比字符串。
    fn normalize_path_for_match(path: &str) -> Option<String> {
        if !path.starts_with('/') {
            return None;
        }
        let synthetic = format!("http://relay.invalid{}", path);
        let u = url::Url::parse(&synthetic).ok()?;
        percent_decode_path(u.path())
    }
}

/// 解码路径中的 `%HH`（十六进制大小写不敏感）；非法序列或解码后非 UTF-8 时返回 `None`。
fn percent_decode_path(input: &str) -> Option<String> {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex(bytes[i + 1])?;
            let lo = hex(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod path_match_tests {
    use super::Config;

    fn cfg_with_vault(vault_path: &str) -> Config {
        Config {
            listen_addr: "0.0.0.0:0".into(),
            vault_path: vault_path.into(),
            jgy_webdav_root: url::Url::parse("https://example.invalid/").unwrap(),
            jgy_username: "u".into(),
            jgy_app_password: "p".into(),
            jgy_remote_path: "/r.kdbx".into(),
            jgy_resource_url: url::Url::parse("https://example.invalid/r.kdbx").unwrap(),
            relay_auth_user: None,
            relay_auth_password: None,
            relay_bearer_token: Some("t".into()),
            max_body_bytes: 1024,
            connect_timeout: std::time::Duration::from_secs(1),
            upstream_timeout: std::time::Duration::from_secs(1),
            require_put_baseline: false,
        }
    }

    #[test]
    fn vault_path_matches_percent_encoded_or_utf8_request() {
        let c = cfg_with_vault("/KeePass/数据库.kdbx");
        assert!(c.request_is_vault_path("/KeePass/%e6%95%b0%e6%8d%ae%e5%ba%93.kdbx"));
        assert!(c.request_is_vault_path("/KeePass/数据库.kdbx"));
        assert!(c.request_is_vault_path("/KeePass/%E6%95%B0%E6%8D%AE%E5%BA%93.kdbx"));
        assert!(!c.request_is_vault_path("/KeePass/other.kdbx"));
    }
}
