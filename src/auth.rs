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

fn bearer_matches(headers: &HeaderMap, expected: &str) -> bool {
    let Some(auth) = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()) else {
        return false;
    };
    let prefix = "Bearer ";
    if !auth.starts_with(prefix) {
        return false;
    }
    let token = auth[prefix.len()..].trim();
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
