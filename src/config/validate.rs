use std::time::Duration;

use url::Url;

use super::ConfigError;

/// 允许连接的上游主机（防 SSRF / 误配）。可通过 `JGY_ALLOWED_HOSTS` 覆盖，逗号分隔。
pub fn validate_host(root: &Url) -> Result<(), ConfigError> {
    let host = root
        .host_str()
        .ok_or_else(|| ConfigError::Url("JGY_WEBDAV_ROOT", "missing host".into()))?;

    let allowed: Vec<String> = match std::env::var("JGY_ALLOWED_HOSTS") {
        Ok(s) if !s.trim().is_empty() => s.split(',').map(|h| h.trim().to_ascii_lowercase()).collect(),
        _ => vec!["dav.jianguoyun.com".to_string()],
    };

    if allowed.iter().any(|h| h == &host.to_ascii_lowercase()) {
        Ok(())
    } else {
        Err(ConfigError::HostNotAllowed(host.to_string()))
    }
}

pub fn build_resource_url(root: &Url, remote_path: &str) -> Result<Url, String> {
    let path = remote_path.trim();
    if !path.starts_with('/') {
        return Err("JGY_REMOTE_PATH must start with '/'".into());
    }
    let mut base_str = root.as_str().to_string();
    if !base_str.ends_with('/') {
        base_str.push('/');
    }
    let base = Url::parse(&base_str).map_err(|e| e.to_string())?;
    let rel = path.trim_start_matches('/');
    base.join(rel).map_err(|e| e.to_string())
}

pub fn parse_usize(s: &str, key: &'static str) -> Result<usize, ConfigError> {
    s.parse()
        .map_err(|_| ConfigError::Env(key))
}

pub fn parse_duration_secs(s: &str, key: &'static str) -> Result<Duration, ConfigError> {
    let secs: u64 = s.parse().map_err(|_| ConfigError::Env(key))?;
    if secs == 0 || secs > 86400 {
        return Err(ConfigError::Env(key));
    }
    Ok(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_resource_joins_paths() {
        let root = Url::parse("https://dav.jianguoyun.com/dav/").unwrap();
        let u = build_resource_url(&root, "/a/b.kdbx").unwrap();
        assert_eq!(u.as_str(), "https://dav.jianguoyun.com/dav/a/b.kdbx");
    }
}
