use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::config::ResolvedConfig;

pub struct Client {
    http: reqwest::Client,
    base: String,
    cfg: ResolvedConfig,
}

#[derive(Debug, Clone, Copy)]
pub enum Auth {
    Admin,
    Run,
    ReadOptional,
    None,
}

impl Client {
    pub fn new(cfg: ResolvedConfig) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("dyt/", env!("CARGO_PKG_VERSION")))
            .danger_accept_invalid_certs(cfg.insecure)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .context("building HTTP client")?;
        let base = cfg.api_base();
        Ok(Self { http, base, cfg })
    }

    pub fn root_url(&self, path: &str) -> String {
        format!("{}{}", self.cfg.api_url, path)
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    fn auth_headers(&self, auth: Auth) -> Result<HeaderMap> {
        let mut headers = HeaderMap::new();
        match auth {
            Auth::Admin => {
                let tok = self
                    .cfg
                    .admin_token
                    .as_deref()
                    .ok_or_else(|| anyhow!("admin token required — set DONEYET_ADMIN_TOKEN, pass --admin-token, or run `dyt login`"))?;
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {tok}"))?,
                );
            }
            Auth::Run => {
                let tok = self
                    .cfg
                    .run_token
                    .as_deref()
                    .ok_or_else(|| anyhow!("run token required — pass --run-token or set DONEYET_RUN_TOKEN"))?;
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {tok}"))?,
                );
            }
            Auth::ReadOptional => {
                if let Some(basic) = self.cfg.read_basic.as_deref() {
                    let encoded = B64.encode(basic.as_bytes());
                    headers.insert(
                        AUTHORIZATION,
                        HeaderValue::from_str(&format!("Basic {encoded}"))?,
                    );
                }
                // Allow handoff token override on the read-optional path —
                // some endpoints (start-run with continue_from) need it.
            }
            Auth::None => {}
        }
        Ok(headers)
    }

    pub async fn request_json<B, T>(
        &self,
        method: Method,
        path: &str,
        auth: Auth,
        body: Option<&B>,
        extra: Option<HeaderMap>,
    ) -> Result<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let url = if path.starts_with("/api/") {
            self.root_url(path)
        } else {
            self.url(path)
        };
        let mut req = self.http.request(method, &url);
        let mut headers = self.auth_headers(auth)?;
        if let Some(extra) = extra {
            headers.extend(extra);
        }
        if body.is_some() {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        req = req.headers(headers);
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req.send().await.with_context(|| format!("calling {url}"))?;
        let status = resp.status();
        let bytes = resp.bytes().await.context("reading response body")?;
        if !status.is_success() {
            bail_with_status(status, &bytes)?;
        }
        if bytes.is_empty() {
            // For 204 No Content callers should use request_unit; but a JSON
            // decoder over "null" gives the same effect when T = ()/Value.
            return serde_json::from_slice(b"null")
                .context("decoding empty response as JSON null");
        }
        serde_json::from_slice(&bytes).with_context(|| {
            format!(
                "decoding JSON response from {url}: {}",
                String::from_utf8_lossy(&bytes)
            )
        })
    }

    pub async fn request_unit<B>(
        &self,
        method: Method,
        path: &str,
        auth: Auth,
        body: Option<&B>,
        extra: Option<HeaderMap>,
    ) -> Result<()>
    where
        B: Serialize + ?Sized,
    {
        let url = if path.starts_with("/api/") {
            self.root_url(path)
        } else {
            self.url(path)
        };
        let mut req = self.http.request(method, &url);
        let mut headers = self.auth_headers(auth)?;
        if let Some(extra) = extra {
            headers.extend(extra);
        }
        if body.is_some() {
            headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }
        req = req.headers(headers);
        if let Some(body) = body {
            req = req.json(body);
        }
        let resp = req.send().await.with_context(|| format!("calling {url}"))?;
        let status = resp.status();
        if !status.is_success() {
            let bytes = resp.bytes().await.unwrap_or_default();
            bail_with_status(status, &bytes)?;
        }
        Ok(())
    }

    pub async fn request_text(
        &self,
        method: Method,
        path: &str,
        auth: Auth,
    ) -> Result<String> {
        let url = if path.starts_with("/api/") || path.starts_with("/health") || path.starts_with("/metrics") {
            self.root_url(path)
        } else {
            self.url(path)
        };
        let resp = self
            .http
            .request(method, &url)
            .headers(self.auth_headers(auth)?)
            .send()
            .await
            .with_context(|| format!("calling {url}"))?;
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(anyhow!("{status}: {text}"));
        }
        Ok(text)
    }
}

fn bail_with_status(status: StatusCode, bytes: &[u8]) -> Result<()> {
    let body_str = String::from_utf8_lossy(bytes);
    // The backend returns {"error": "..."} on failures (see error.rs).
    let msg = serde_json::from_slice::<Value>(bytes)
        .ok()
        .and_then(|v| {
            v.get("error")
                .and_then(|e| e.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| body_str.trim().to_string());

    let hint = match status {
        StatusCode::UNAUTHORIZED => " — check your admin/run token",
        StatusCode::NOT_FOUND => " — does the slug / id exist?",
        StatusCode::CONFLICT => " — the resource state forbids that change",
        _ => "",
    };

    bail!("HTTP {status}: {msg}{hint}");
}
