use bytes::Bytes;
use http::header::{CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_MATCH, LAST_MODIFIED};
use http::HeaderMap;
use reqwest::Client;
use url::Url;

use crate::config::Config;

use super::types::UpstreamResponse;
use super::UpstreamError;

/// 对坚果云单一资源发起 GET/HEAD/PUT。
#[derive(Clone)]
pub struct NutstoreClient {
    http: Client,
    resource_url: Url,
    username: String,
    password: String,
}

impl NutstoreClient {
    pub fn new(cfg: &Config) -> Result<Self, UpstreamError> {
        let http = Client::builder()
            .connect_timeout(cfg.connect_timeout)
            .timeout(cfg.upstream_timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| UpstreamError::Build(e.to_string()))?;

        Ok(Self {
            http,
            resource_url: cfg.jgy_resource_url.clone(),
            username: cfg.jgy_username.clone(),
            password: cfg.jgy_app_password.clone(),
        })
    }

    pub fn resource_url(&self) -> &Url {
        &self.resource_url
    }

    pub async fn head(&self) -> Result<UpstreamResponse, UpstreamError> {
        self.request_empty_body(reqwest::Method::HEAD).await
    }

    pub async fn get(&self) -> Result<UpstreamResponse, UpstreamError> {
        self.request_empty_body(reqwest::Method::GET).await
    }

    /// 带可选 `If-Match` 的条件 PUT（层 A）。
    pub async fn put(
        &self,
        body: Bytes,
        if_match: Option<&str>,
    ) -> Result<UpstreamResponse, UpstreamError> {
        let mut req = self
            .http
            .put(self.resource_url.clone())
            .basic_auth(&self.username, Some(&self.password))
            .header(
                CONTENT_TYPE,
                http::HeaderValue::from_static("application/octet-stream"),
            )
            .body(body.to_vec());

        if let Some(v) = if_match {
            if !v.trim().is_empty() {
                req = req.header(IF_MATCH, v);
            }
        }

        let resp = req.send().await.map_err(UpstreamError::Transport)?;
        Self::map_response(resp).await
    }

    async fn request_empty_body(&self, method: reqwest::Method) -> Result<UpstreamResponse, UpstreamError> {
        let resp = self
            .http
            .request(method, self.resource_url.clone())
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
            .map_err(UpstreamError::Transport)?;

        Self::map_response(resp).await
    }

    async fn map_response(resp: reqwest::Response) -> Result<UpstreamResponse, UpstreamError> {
        let status = http::StatusCode::from_u16(resp.status().as_u16())
            .unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR);

        let mut headers = HeaderMap::new();
        for (k, v) in resp.headers().iter() {
            if let (Ok(name), Ok(val)) = (
                http::HeaderName::from_bytes(k.as_str().as_bytes()),
                http::HeaderValue::from_bytes(v.as_bytes()),
            ) {
                headers.append(name, val);
            }
        }

        let body = resp.bytes().await.map_err(UpstreamError::Transport)?;

        Ok(UpstreamResponse {
            status,
            headers,
            body,
        })
    }
}

/// 从上游响应挑选可转发给客户端的响应头（不含 hop-by-hop）。
pub fn filter_forward_headers(src: &HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for key in [ETAG, CONTENT_LENGTH, CONTENT_TYPE, LAST_MODIFIED].iter() {
        if let Some(v) = src.get(key) {
            out.insert(key.clone(), v.clone());
        }
    }
    out
}
