//! Minimal WebDAV client used for remote backup / restore.
//!
//! Only the verbs we need are implemented (MKCOL / PROPFIND / PUT / GET) over the already
//! available `reqwest` blocking client with HTTP Basic auth. We deliberately
//! avoid XML parsing by using a fixed remote layout
//! (`<remote_dir>/skilldo-backup.json`), which keeps the dependency surface
//! tiny while still covering the backup use-case.

use std::time::Duration;

use anyhow::{Context, Result};

use crate::core::app_config::WebDavConfig;
use crate::core::credentials::{
    BwVaultCredentialProvider, CredentialProvider, WEBDAV_PASSWORD_ALIAS,
};

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
        Self::new_with_provider(cfg, &BwVaultCredentialProvider::default())
    }

    fn new_with_provider(
        cfg: &WebDavConfig,
        credential_provider: &dyn CredentialProvider,
    ) -> Result<Self> {
        let base = cfg.url.trim_end_matches('/').to_string();
        if base.is_empty() {
            anyhow::bail!("WebDAV URL 未配置");
        }
        let parsed = reqwest::Url::parse(&base).context("WebDAV URL 格式无效")?;
        let is_loopback = parsed
            .host_str()
            .map(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            })
            .unwrap_or(false);
        if parsed.scheme() != "https" && !(parsed.scheme() == "http" && is_loopback) {
            anyhow::bail!("WebDAV 必须使用 HTTPS；HTTP 会明文发送 Basic Auth 凭据");
        }
        if parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            anyhow::bail!("WebDAV URL 必须包含主机，且不能在 URL 中嵌入凭据");
        }
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            // Never follow a server redirect with an authenticated request;
            // this also prevents a TLS endpoint from redirecting credentials
            // to an insecure or unrelated destination.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .context("创建 HTTP 客户端失败")?;
        let user = cfg.user.trim().to_string();
        let password = if user.is_empty() || !cfg.password.is_empty() {
            cfg.password.clone()
        } else {
            credential_provider
                .get_secret(WEBDAV_PASSWORD_ALIAS)
                .map_err(|_| anyhow::anyhow!("WebDAV 密码凭据不可用"))?
        };
        Ok(Self {
            base,
            user,
            password,
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

    fn request_error_context(operation: &str, is_connect: bool, error_detail: &str) -> String {
        let detail = error_detail.to_ascii_lowercase();
        let tls_validation_failure = is_connect
            && [
                "certificate",
                "cert",
                "tls",
                "ssl",
                "unknownissuer",
                "invalid peer",
            ]
            .iter()
            .any(|marker| detail.contains(marker));

        if tls_validation_failure {
            format!(
                "{operation} 请求失败：TLS 证书验证未通过。请使用证书 SAN 中匹配的 HTTPS 主机名，并确认签发 CA 属于 HTTP 客户端的受信任根证书集合；不要改用远程 HTTP 或关闭证书验证"
            )
        } else {
            format!("{operation} 请求失败")
        }
    }

    fn status_error_context(operation: &str, status: reqwest::StatusCode) -> String {
        let guidance = match status.as_u16() {
            401 => "请核对 WebDAV 用户名、密码或账户状态",
            403 => "请检查 WebDAV 账号对目标共享目录的写入/创建权限，以及反向代理的 PUT/MKCOL 访问策略；服务器允许 HTTP 方法不代表账号有目录写权限",
            _ => "",
        };
        if guidance.is_empty() {
            format!("{operation}失败 (HTTP {status})")
        } else {
            format!("{operation}失败 (HTTP {status})：{guidance}")
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
            .map_err(|error| {
                let context =
                    Self::request_error_context("MKCOL", error.is_connect(), &error.to_string());
                anyhow::anyhow!(context)
            })?;
        let status = resp.status();
        if status.is_success() || status == 405 {
            Ok(())
        } else {
            anyhow::bail!("{}", Self::status_error_context("创建远程目录", status))
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
            .map_err(|error| {
                let context =
                    Self::request_error_context("PUT", error.is_connect(), &error.to_string());
                anyhow::anyhow!(context)
            })?;
        let status = resp.status();
        if status.is_success() || status == 201 || status == 204 {
            Ok(())
        } else {
            anyhow::bail!("{}", Self::status_error_context("上传文件", status))
        }
    }

    /// Download `remote_path` and return its text contents.
    pub fn get(&self, remote_path: &str) -> Result<String> {
        let url = self.full_url(remote_path);
        let resp = self.auth(self.client.get(&url)).send().map_err(|error| {
            let context =
                Self::request_error_context("GET", error.is_connect(), &error.to_string());
            anyhow::anyhow!(context)
        })?;
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
        let resp = self.auth(self.client.get(&url)).send().map_err(|error| {
            let context =
                Self::request_error_context("GET", error.is_connect(), &error.to_string());
            anyhow::anyhow!(context)
        })?;
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

    /// Read-only connection check. It verifies the collection with PROPFIND,
    /// then GETs the profile document; it never creates or uploads data.
    pub fn check_profile(&self, remote_path: &str) -> Result<bool> {
        Ok(self.get_profile_optional(remote_path)?.is_some())
    }

    /// Read the profile only after confirming its parent collection exists.
    pub fn get_profile_optional(&self, remote_path: &str) -> Result<Option<WebDavDocument>> {
        let collection = remote_path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        self.check_collection(collection)?;
        self.get_optional(remote_path)
    }

    /// Verify that the configured remote collection exists before treating a
    /// missing profile document as a normal first-sync state.
    fn check_collection(&self, remote_dir: &str) -> Result<()> {
        let url = format!("{}/", self.full_url(remote_dir).trim_end_matches('/'));
        let method = reqwest::Method::from_bytes(b"PROPFIND").context("非法 HTTP 方法 PROPFIND")?;
        let response = self
            .auth(self.client.request(method, &url).header("Depth", "0"))
            .send()
            .map_err(|error| {
                let context =
                    Self::request_error_context("PROPFIND", error.is_connect(), &error.to_string());
                anyhow::anyhow!(context)
            })?;
        let status = response.status();
        if status.is_success() {
            Ok(())
        } else if status == 404 {
            anyhow::bail!("WebDAV 远程目录不存在 (HTTP 404)：请检查 HTTPS 基础路径和远程目录配置")
        } else if status == 405 || status == 501 {
            anyhow::bail!("WebDAV 目录检查失败 (HTTP {status})：服务器或反向代理未允许 PROPFIND，无法确认远程目录是否存在")
        } else {
            anyhow::bail!(
                "{}",
                Self::status_error_context("检查 WebDAV 远程目录", status)
            )
        }
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
        let resp = request.send().map_err(|error| {
            let context =
                Self::request_error_context("条件 PUT", error.is_connect(), &error.to_string());
            anyhow::Error::new(error).context(context)
        })?;
        let status = resp.status();
        if status == 412 || status == 409 {
            anyhow::bail!("PROFILE_REMOTE_CONFLICT|远端 Profile 已被其他设备更新，请重新同步")
        }
        if !status.is_success() {
            anyhow::bail!("{}", Self::status_error_context("上传文件", status));
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
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use mockito::Matcher;

    use super::*;
    use crate::core::credentials::{CommandOutput, CommandRunError, CommandRunner};

    struct FakeRunner {
        success: bool,
        stdout: Vec<u8>,
        calls: Arc<AtomicUsize>,
    }

    impl CommandRunner for FakeRunner {
        fn run(
            &self,
            executable: &str,
            args: &[&str],
            _stdin: Option<&[u8]>,
        ) -> std::result::Result<CommandOutput, CommandRunError> {
            assert_eq!(executable, "bwvault");
            assert_eq!(
                args,
                [
                    "credential",
                    "get",
                    "--alias",
                    WEBDAV_PASSWORD_ALIAS,
                    "--reveal",
                    "--json"
                ]
            );
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(CommandOutput {
                success: self.success,
                stdout: self.stdout.clone(),
            })
        }
    }

    fn vault_provider(
        success: bool,
        stdout: &[u8],
    ) -> (BwVaultCredentialProvider<FakeRunner>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            BwVaultCredentialProvider::new(FakeRunner {
                success,
                stdout: stdout.to_vec(),
                calls: calls.clone(),
            }),
            calls,
        )
    }

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
    fn rejects_remote_http_before_constructing_authenticated_client() {
        let cfg = WebDavConfig {
            url: "http://dav.example.test".to_string(),
            user: "username".to_string(),
            password: "<redacted>".to_string(),
            remote_dir: "skills".to_string(),
        };
        let error = match WebDavClient::new(&cfg) {
            Ok(_) => panic!("HTTP should be rejected"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("HTTPS"));
    }

    #[test]
    fn rejects_credentials_embedded_in_url() {
        let cfg = config("https://user:pass@dav.example.test".to_string());
        let error = match WebDavClient::new(&cfg) {
            Ok(_) => panic!("URL credentials should be rejected"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("不能在 URL 中嵌入凭据"));
    }

    #[test]
    fn authenticated_empty_config_password_uses_vault_alias() {
        let mut server = mockito::Server::new();
        let collection = server
            .mock("PROPFIND", "/")
            .match_header("authorization", "Basic dXNlcm5hbWU6dmF1bHQtcGFzcw==")
            .match_header("depth", "0")
            .with_status(207)
            .create();
        let request = server
            .mock("GET", "/skilldo-profile.json")
            .match_header("authorization", "Basic dXNlcm5hbWU6dmF1bHQtcGFzcw==")
            .with_status(404)
            .create();
        let cfg = WebDavConfig {
            url: server.url(),
            user: "username".to_string(),
            password: String::new(),
            remote_dir: String::new(),
        };
        let (provider, calls) = vault_provider(true, br#"{"secret":"vault-pass"}"#);
        let client = WebDavClient::new_with_provider(&cfg, &provider).unwrap();

        assert!(!client.check_profile(PROFILE_REMOTE_FILE).unwrap());
        collection.assert();
        request.assert();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn authenticated_in_memory_password_remains_available_for_tests() {
        let mut server = mockito::Server::new();
        let collection = server
            .mock("PROPFIND", "/")
            .match_header("authorization", "Basic dXNlcm5hbWU6bWVtb3J5LXBhc3M=")
            .match_header("depth", "0")
            .with_status(207)
            .create();
        let request = server
            .mock("GET", "/skilldo-profile.json")
            .match_header("authorization", "Basic dXNlcm5hbWU6bWVtb3J5LXBhc3M=")
            .with_status(404)
            .create();
        let cfg = WebDavConfig {
            url: server.url(),
            user: "username".to_string(),
            password: "memory-pass".to_string(),
            remote_dir: String::new(),
        };
        let (provider, calls) = vault_provider(false, b"vault-secret-marker");
        let client = WebDavClient::new_with_provider(&cfg, &provider).unwrap();

        assert!(!client.check_profile(PROFILE_REMOTE_FILE).unwrap());
        collection.assert();
        request.assert();
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn unavailable_vault_password_fails_closed_without_leaking_secret() {
        let cfg = WebDavConfig {
            url: "https://dav.example.test".to_string(),
            user: "username".to_string(),
            password: String::new(),
            remote_dir: String::new(),
        };
        let (provider, calls) = vault_provider(false, b"vault-secret-marker");

        let error = match WebDavClient::new_with_provider(&cfg, &provider) {
            Ok(_) => panic!("missing vault credential should fail closed"),
            Err(error) => error.to_string(),
        };

        assert_eq!(error, "WebDAV 密码凭据不可用");
        assert!(!error.contains("vault-secret-marker"));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn no_auth_profile_does_not_read_vault() {
        let cfg = WebDavConfig {
            url: "https://dav.example.test".to_string(),
            user: String::new(),
            password: String::new(),
            remote_dir: String::new(),
        };
        let (provider, calls) = vault_provider(false, b"vault-secret-marker");

        WebDavClient::new_with_provider(&cfg, &provider).unwrap();

        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn tls_connection_error_suggests_verified_host_and_trusted_ca() {
        let context = WebDavClient::request_error_context(
            "GET",
            true,
            "invalid peer certificate: UnknownIssuer",
        );

        assert!(context.contains("证书 SAN"));
        assert!(context.contains("CA"));
        assert!(context.contains("不要改用远程 HTTP"));
    }

    #[test]
    fn non_tls_request_error_keeps_generic_context() {
        let context = WebDavClient::request_error_context("GET", false, "connection timed out");

        assert_eq!(context, "GET 请求失败");
    }

    #[test]
    fn forbidden_write_error_points_to_acl_or_proxy_without_response_body() {
        let context =
            WebDavClient::status_error_context("上传文件", reqwest::StatusCode::FORBIDDEN);

        assert!(context.contains("HTTP 403"));
        assert!(context.contains("共享目录"));
        assert!(context.contains("反向代理"));
        assert!(!context.contains("password"));
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
    fn check_profile_is_read_only_and_does_not_create_remote_state() {
        let mut server = mockito::Server::new();
        let collection = server
            .mock("PROPFIND", "/")
            .match_header("depth", "0")
            .with_status(207)
            .create();
        let request = server
            .mock("GET", "/skilldo-profile.json")
            .with_status(404)
            .create();
        let client = WebDavClient::new(&config(server.url())).unwrap();

        assert!(!client.check_profile(PROFILE_REMOTE_FILE).unwrap());
        collection.assert();
        request.assert();
    }

    #[test]
    fn check_profile_rejects_missing_collection_instead_of_treating_route_404_as_empty_profile() {
        let mut server = mockito::Server::new();
        let collection = server.mock("PROPFIND", "/").with_status(404).create();
        let profile = server
            .mock("GET", "/skilldo-profile.json")
            .with_status(404)
            .expect(0)
            .create();
        let client = WebDavClient::new(&config(server.url())).unwrap();

        let error = client
            .check_profile(PROFILE_REMOTE_FILE)
            .unwrap_err()
            .to_string();
        assert!(error.contains("远程目录不存在"));
        collection.assert();
        profile.assert();
    }

    #[test]
    fn check_profile_reports_proxy_blocking_propfind() {
        let mut server = mockito::Server::new();
        let collection = server.mock("PROPFIND", "/").with_status(405).create();
        let client = WebDavClient::new(&config(server.url())).unwrap();

        let error = client
            .check_profile(PROFILE_REMOTE_FILE)
            .unwrap_err()
            .to_string();
        assert!(error.contains("未允许 PROPFIND"));
        collection.assert();
    }

    #[test]
    fn check_profile_uses_canonical_nested_collection_url() {
        let mut server = mockito::Server::new();
        let collection = server
            .mock("PROPFIND", "/a/b/")
            .match_header("depth", "0")
            .with_status(207)
            .create();
        let profile = server
            .mock("GET", "/a/b/skilldo-profile.json")
            .with_status(404)
            .create();
        let client = WebDavClient::new(&config(server.url())).unwrap();

        assert!(!client.check_profile("a/b/skilldo-profile.json").unwrap());
        collection.assert();
        profile.assert();
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
