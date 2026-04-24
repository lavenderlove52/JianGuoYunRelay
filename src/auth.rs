//! 公司端访问中继的鉴权（设计 §5.3）：Basic 与/或 Bearer。

use axum::http::{header::AUTHORIZATION, HeaderMap};
use base64::Engine;
use subtle::ConstantTimeEq;

use crate::config::Config;

/// 校验中继请求是否携带合法凭据（任一匹配即通过）。
pub fn validate_request(cfg: &Config, headers: &HeaderMap) -> bool {
    if let Some(token) = cfg.relay_bearer_token.as_deref() {
        if bearer_matches(headers, token) {
            return true;
        }
    }
    if let (Some(u), Some(p)) = (
        cfg.relay_auth_user.as_deref(),
        cfg.relay_auth_password.as_deref(),
    ) {
        if basic_matches(headers, u, p) {
            return true;
        }
    }
    false
}

/// 为 401 响应生成认证挑战，便于桌面/WebDAV 客户端触发 Basic/Bearer 重试。
pub fn www_authenticate_challenge(cfg: &Config) -> Option<&'static str> {
    if cfg.relay_auth_user.is_some() && cfg.relay_auth_password.is_some() {
        return Some(r#"Basic realm="JianGuoYunRelay", charset="UTF-8""#);
    }
    if cfg.relay_bearer_token.is_some() {
        return Some(r#"Bearer realm="JianGuoYunRelay""#);
    }
    None
}

fn bearer_matches(headers: &HeaderMap, expected: &str) -> bool {
    let Some(auth) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let prefix = "Bearer ";
    if !auth.starts_with(prefix) {
        return false;
    }
    let token = auth[prefix.len()..].trim().trim_end_matches('\r');
    let expected = expected.trim().trim_end_matches('\r');
    ct_eq_str(token, expected)
}

fn basic_matches(headers: &HeaderMap, user: &str, pass: &str) -> bool {
    let Some(auth) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let prefix = "Basic ";
    if !auth.starts_with(prefix) {
        return false;
    }
    let decoded = match base64::engine::general_purpose::STANDARD.decode(auth[prefix.len()..].as_bytes()) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let Ok(creds) = String::from_utf8(decoded) else {
        return false;
    };
    let Some((u, p)) = creds.split_once(':') else {
        return false;
    };
    // 解码后去掉首尾空白与 `\r`，兼容部分客户端与 Windows 行尾。
    let u = u.trim().trim_end_matches('\r');
    let p = p.trim().trim_end_matches('\r');
    ct_eq_str(u, user) && ct_eq_str(p, pass)
}

fn ct_eq_str(a: &str, b: &str) -> bool {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    if ab.len() != bb.len() {
        return false;
    }
    ab.ct_eq(bb).into()
}

#[cfg(test)]
mod tests {
    use super::www_authenticate_challenge;
    use crate::config::Config;

    fn base_config() -> Config {
        Config {
            listen_addr: "0.0.0.0:0".into(),
            vault_path: "/vault.kdbx".into(),
            jgy_webdav_root: url::Url::parse("https://example.invalid/").unwrap(),
            jgy_username: "u".into(),
            jgy_app_password: "p".into(),
            jgy_remote_path: "/r.kdbx".into(),
            jgy_resource_url: url::Url::parse("https://example.invalid/r.kdbx").unwrap(),
            relay_auth_user: None,
            relay_auth_password: None,
            relay_bearer_token: None,
            max_body_bytes: 1024,
            connect_timeout: std::time::Duration::from_secs(1),
            upstream_timeout: std::time::Duration::from_secs(1),
            require_put_baseline: false,
        }
    }

    #[test]
    fn prefers_basic_challenge_when_basic_auth_enabled() {
        let mut cfg = base_config();
        cfg.relay_auth_user = Some("relay_user".into());
        cfg.relay_auth_password = Some("123456".into());
        assert_eq!(
            www_authenticate_challenge(&cfg),
            Some(r#"Basic realm="JianGuoYunRelay", charset="UTF-8""#)
        );
    }
}
