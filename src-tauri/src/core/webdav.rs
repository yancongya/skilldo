//! Minimal WebDAV client used for remote backup / restore.
//!
//! Only the verbs we need are implemented (MKCOL / PUT / GET) over the already
//! available `reqwest` blocking client with HTTP Basic auth. We deliberately
//! avoid PROPFIND / XML parsing by using a fixed remote layout
//! (`<remote_dir>/skilldo-backup.json`), which keeps the dependency surface
//! tiny while still covering the backup use-case.

use std::time::Duration;

use anyhow::{Context, Result};

use crate::core::app_config::WebDavConfig;

#[derive(Debug, Clone)]
pub struct WebDavDocument {
    pub body: String,
    pub etag: Option<String>,
}

/// A small WebDAV client bound to a single server profile.
pub struct WebDavClient {
    base: String,
    user: String,
    password: String,
    client: reqwest::blocking::Client,
}

impl WebDavClient {
    /// Build a client from a stored [`WebDavConfig`].
    pub fn new(cfg: &WebDavConfig) -> Result<Self> {
        let base = cfg.url.trim_end_matches('/').to_string();
        if base.is_empty() {
            anyhow::bail!("WebDAV URL 未配置");
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("创建 HTTP 客户端失败")?;
        Ok(Self {
            base,
            user: cfg.user.trim().to_string(),
            password: cfg.password.clone(),
            client,
        })
    }

    fn auth(&self, req: reqwest::blocking::RequestBuilder) -> reqwest::blocking::RequestBuilder {
        if self.user.is_empty() {
            req
        } else {
            req.basic_auth(&self.user, Some(&self.password))
        }
    }

    /// Join the server base URL with a remote path (no leading slash).
    fn full_url(&self, path: &str) -> String {
        let path = path.trim_start_matches('/');
        format!("{}/{}", self.base, path)
    }

    /// Ensure a remote directory (WebDAV collection) exists. A `405` means
    /// the collection already exists and is treated as success.
    pub fn mkcol(&self, dir: &str) -> Result<()> {
        let url = self.full_url(dir);
        let method = reqwest::Method::from_bytes(b"MKCOL").context("非法 HTTP 方法 MKCOL")?;
        let resp = self
            .auth(self.client.request(method, &url))
            .send()
            .context("MKCOL 请求失败")?;
        let status = resp.status();
        if status.is_success() || status == 405 {
            Ok(())
        } else {
            anyhow::bail!("创建远程目录失败 (HTTP {})", status)
        }
    }

    /// Upload `body` to `remote_path` (e.g. `backups/skilldo-backup.json`).
    pub fn put(&self, remote_path: &str, body: &str) -> Result<()> {
        let url = self.full_url(remote_path);
        let resp = self
            .auth(self.client.put(&url))
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .context("PUT 请求失败")?;
        let status = resp.status();
        if status.is_success() || status == 201 || status == 204 {
            Ok(())
        } else {
            anyhow::bail!("上传文件失败 (HTTP {})", status)
        }
    }

    /// Download `remote_path` and return its text contents.
    pub fn get(&self, remote_path: &str) -> Result<String> {
        let url = self.full_url(remote_path);
        let resp = self
            .auth(self.client.get(&url))
            .send()
            .context("GET 请求失败")?;
        let status = resp.status();
        if status.is_success() {
            resp.text().context("读取响应内容失败")
        } else if status == 404 {
            anyhow::bail!("远程文件不存在: {}", remote_path)
        } else {
            anyhow::bail!("下载文件失败 (HTTP {})", status)
        }
    }

    /// Download a document together with its ETag. Missing documents return `None`.
    pub fn get_optional(&self, remote_path: &str) -> Result<Option<WebDavDocument>> {
        let url = self.full_url(remote_path);
        let resp = self
            .auth(self.client.get(&url))
            .send()
            .context("GET 请求失败")?;
        let status = resp.status();
        if status.is_success() {
            let etag = resp
                .headers()
                .get(reqwest::header::ETAG)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);
            return Ok(Some(WebDavDocument {
                body: resp.text().context("读取响应内容失败")?,
                etag,
            }));
        }
        if status == 404 {
            return Ok(None);
        }
        anyhow::bail!("下载文件失败 (HTTP {})", status)
    }

    /// Upload with optimistic concurrency. `expected_etag=None` creates only
    /// when the document does not already exist.
    pub fn put_conditional(
        &self,
        remote_path: &str,
        body: &str,
        expected_etag: Option<&str>,
    ) -> Result<Option<String>> {
        let url = self.full_url(remote_path);
        let request = self
            .auth(self.client.put(&url))
            .header("Content-Type", "application/json")
            .body(body.to_string());
        let request = if let Some(etag) = expected_etag {
            request.header(reqwest::header::IF_MATCH, etag)
        } else {
            request.header(reqwest::header::IF_NONE_MATCH, "*")
        };
        let resp = request.send().context("条件 PUT 请求失败")?;
        let status = resp.status();
        if status == 412 || status == 409 {
            anyhow::bail!("PROFILE_REMOTE_CONFLICT|远端 Profile 已被其他设备更新，请重新同步")
        }
        if !status.is_success() {
            anyhow::bail!("上传文件失败 (HTTP {})", status);
        }
        Ok(resp
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string))
    }
}

/// Remote file name used for the combined backup blob.
pub const BACKUP_REMOTE_FILE: &str = "skilldo-backup.json";
pub const PROFILE_REMOTE_FILE: &str = "skilldo-profile.json";

/// Compute the remote path for the backup file given a (possibly empty)
/// `remote_dir` profile setting.
pub fn backup_remote_path(remote_dir: &str) -> String {
    let dir = remote_dir
        .trim()
        .trim_start_matches('/')
        .trim_end_matches('/');
    if dir.is_empty() {
        BACKUP_REMOTE_FILE.to_string()
    } else {
        format!("{}/{}", dir, BACKUP_REMOTE_FILE)
    }
}

fn collection_paths(remote_dir: &str) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    let mut current = String::new();
    for segment in remote_dir
        .trim()
        .trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
    {
        if segment == "." || segment == ".." {
            anyhow::bail!("WebDAV 远程目录不能包含 . 或 ..");
        }
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(segment);
        paths.push(current.clone());
    }
    Ok(paths)
}

/// Convenience: serialize `body` and PUT it to the backup location, creating
/// the remote directory first. Returns the remote path actually written.
pub fn upload_backup(cfg: &WebDavConfig, body: &str) -> Result<String> {
    let client = WebDavClient::new(cfg)?;
    let remote_path = backup_remote_path(&cfg.remote_dir);
    for dir in collection_paths(&cfg.remote_dir)? {
        client.mkcol(&dir)?;
    }
    client.put(&remote_path, body)?;
    Ok(remote_path)
}

/// Convenience: download the combined backup blob from the WebDAV location.
pub fn download_backup(cfg: &WebDavConfig) -> Result<String> {
    let client = WebDavClient::new(cfg)?;
    let remote_path = backup_remote_path(&cfg.remote_dir);
    client.get(&remote_path)
}

pub fn profile_remote_path(remote_dir: &str) -> String {
    let dir = remote_dir
        .trim()
        .trim_start_matches('/')
        .trim_end_matches('/');
    if dir.is_empty() {
        PROFILE_REMOTE_FILE.to_string()
    } else {
        format!("{dir}/{PROFILE_REMOTE_FILE}")
    }
}

pub fn prepare_remote_dir(cfg: &WebDavConfig) -> Result<WebDavClient> {
    let client = WebDavClient::new(cfg)?;
    for dir in collection_paths(&cfg.remote_dir)? {
        client.mkcol(&dir)?;
    }
    Ok(client)
}

#[cfg(test)]
mod tests {
    use mockito::Matcher;

    use super::*;

    fn config(url: String) -> WebDavConfig {
        WebDavConfig {
            url,
            user: String::new(),
            password: String::new(),
            remote_dir: String::new(),
        }
    }

    #[test]
    fn collection_paths_builds_each_parent() {
        assert_eq!(
            collection_paths("/backups/skilldo/").unwrap(),
            vec!["backups", "backups/skilldo"]
        );
    }

    #[test]
    fn collection_paths_rejects_parent_traversal() {
        assert!(collection_paths("backups/../private").is_err());
    }

    #[test]
    fn backup_path_handles_empty_and_nested_directories() {
        assert_eq!(backup_remote_path(""), BACKUP_REMOTE_FILE);
        assert_eq!(
            backup_remote_path("/backups/skilldo/"),
            "backups/skilldo/skilldo-backup.json"
        );
        assert_eq!(
            profile_remote_path("/backups/skilldo/"),
            "backups/skilldo/skilldo-profile.json"
        );
    }

    #[test]
    fn get_optional_returns_body_and_etag() {
        let mut server = mockito::Server::new();
        let request = server
            .mock("GET", "/skilldo-profile.json")
            .with_status(200)
            .with_header("etag", "\"profile-7\"")
            .with_body("{\"profileVersion\":1}")
            .create();

        let client = WebDavClient::new(&config(server.url())).unwrap();
        let document = client.get_optional(PROFILE_REMOTE_FILE).unwrap().unwrap();

        request.assert();
        assert_eq!(document.etag.as_deref(), Some("\"profile-7\""));
        assert_eq!(document.body, "{\"profileVersion\":1}");
    }

    #[test]
    fn conditional_put_uses_etag_for_existing_document() {
        let mut server = mockito::Server::new();
        let request = server
            .mock("PUT", "/skilldo-profile.json")
            .match_header("if-match", "\"profile-7\"")
            .match_header("content-type", Matcher::Regex("application/json.*".into()))
            .with_status(204)
            .with_header("etag", "\"profile-8\"")
            .create();

        let client = WebDavClient::new(&config(server.url())).unwrap();
        let etag = client
            .put_conditional(PROFILE_REMOTE_FILE, "{}", Some("\"profile-7\""))
            .unwrap();

        request.assert();
        assert_eq!(etag.as_deref(), Some("\"profile-8\""));
    }

    #[test]
    fn conditional_put_creates_only_when_missing() {
        let mut server = mockito::Server::new();
        let request = server
            .mock("PUT", "/skilldo-profile.json")
            .match_header("if-none-match", "*")
            .with_status(201)
            .create();

        let client = WebDavClient::new(&config(server.url())).unwrap();
        client
            .put_conditional(PROFILE_REMOTE_FILE, "{}", None)
            .unwrap();

        request.assert();
    }

    #[test]
    fn conditional_put_reports_remote_race() {
        let mut server = mockito::Server::new();
        let request = server
            .mock("PUT", "/skilldo-profile.json")
            .match_header("if-match", "\"stale\"")
            .with_status(412)
            .create();

        let client = WebDavClient::new(&config(server.url())).unwrap();
        let error = client
            .put_conditional(PROFILE_REMOTE_FILE, "{}", Some("\"stale\""))
            .unwrap_err();

        request.assert();
        assert!(error.to_string().starts_with("PROFILE_REMOTE_CONFLICT|"));
    }
}
