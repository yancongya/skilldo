//! One-time migration of historical SkillDo SQLite secrets to BWVault.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;

use super::credentials::{CredentialProvider, GITHUB_TOKEN_ALIAS, WEBDAV_PASSWORD_ALIAS};
use super::skill_store::SkillStore;

const GITHUB_TOKEN_KEY: &str = "github_token";
const WEBDAV_CONFIG_KEY: &str = "webdav_config";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationState {
    Migrated,
    AlreadyInVault,
    NotConfigured,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CredentialMigrationReport {
    pub github_token: MigrationState,
    pub webdav_password: MigrationState,
    pub database_compacted: bool,
}

fn ensure_alias_matches_or_write(
    provider: &dyn CredentialProvider,
    alias: &str,
    secret: &str,
    username: Option<&str>,
) -> Result<MigrationState> {
    let exists = provider
        .alias_exists(alias)
        .map_err(|_| anyhow::anyhow!("无法安全检查 BWVault 凭据别名"))?;
    if exists {
        let stored = provider
            .get_secret(alias)
            .map_err(|_| anyhow::anyhow!("BWVault 凭据不可读取，旧数据已保留"))?;
        if stored != secret {
            anyhow::bail!("BWVault 中已有不同的同名凭据；旧数据已保留，请先人工核对");
        }
        return Ok(MigrationState::AlreadyInVault);
    }

    provider
        .set_secret(alias, secret, username)
        .map_err(|_| anyhow::anyhow!("BWVault 写入失败，旧数据已保留"))?;
    let stored = provider
        .get_secret(alias)
        .map_err(|_| anyhow::anyhow!("BWVault 写入后无法核验，旧数据已保留"))?;
    if stored != secret {
        anyhow::bail!("BWVault 写入内容核验不匹配，旧数据已保留");
    }
    Ok(MigrationState::Migrated)
}

/// Migrate both legacy values as one logical operation. SQLite is scrubbed only
/// after every present secret is confirmed in BWVault. Re-running is safe: an
/// existing alias must contain the exact same value or migration stops.
pub fn migrate_legacy_credentials(
    store: &SkillStore,
    provider: &dyn CredentialProvider,
) -> Result<CredentialMigrationReport> {
    let github = store.get_setting(GITHUB_TOKEN_KEY)?.unwrap_or_default();
    let webdav_raw = store.get_setting(WEBDAV_CONFIG_KEY)?;
    let mut sanitized_webdav = None;
    let mut webdav_secret = None;

    if let Some(raw) = webdav_raw.as_deref() {
        let mut config: Value =
            serde_json::from_str(raw).context("WebDAV 配置无法解析；未迁移或清除任何凭据")?;
        let object = config
            .as_object_mut()
            .context("WebDAV 配置格式无效；未迁移或清除任何凭据")?;
        let password = object
            .get("password")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if !password.is_empty() {
            let username = object
                .get("user")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            object.insert("password".to_owned(), Value::String(String::new()));
            sanitized_webdav = Some(
                serde_json::to_string(&config).context("WebDAV 配置脱敏失败；未清除任何凭据")?,
            );
            webdav_secret = Some((password, username));
        }
    }

    let github_state = if github.is_empty() {
        MigrationState::NotConfigured
    } else {
        ensure_alias_matches_or_write(provider, GITHUB_TOKEN_ALIAS, &github, None)?
    };
    let webdav_state = if let Some((password, username)) = webdav_secret.as_ref() {
        ensure_alias_matches_or_write(
            provider,
            WEBDAV_PASSWORD_ALIAS,
            password,
            username.as_deref(),
        )?
    } else {
        MigrationState::NotConfigured
    };

    let database_compacted = !github.is_empty() || sanitized_webdav.is_some();
    if database_compacted {
        store.scrub_legacy_auth_settings(sanitized_webdav.as_deref())?;
    }

    Ok(CredentialMigrationReport {
        github_token: github_state,
        webdav_password: webdav_state,
        database_compacted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::credentials::MockCredentialProvider;

    fn test_store() -> (tempfile::TempDir, SkillStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(dir.path().join("skilldo.db"));
        store.ensure_schema().unwrap();
        (dir, store)
    }

    #[test]
    fn migrates_then_scrubs_legacy_values_and_keeps_webdav_settings() {
        let (_dir, store) = test_store();
        store
            .set_setting(GITHUB_TOKEN_KEY, "github-sentinel")
            .unwrap();
        store
            .set_setting(
                WEBDAV_CONFIG_KEY,
                r#"{"url":"https://dav.example","user":"dav-user","password":"dav-sentinel","remoteDir":"skilldo"}"#,
            )
            .unwrap();
        let provider = MockCredentialProvider::default();

        let report = migrate_legacy_credentials(&store, &provider).unwrap();

        assert_eq!(report.github_token, MigrationState::Migrated);
        assert_eq!(report.webdav_password, MigrationState::Migrated);
        assert!(report.database_compacted);
        assert_eq!(
            store.get_setting(GITHUB_TOKEN_KEY).unwrap().as_deref(),
            Some("")
        );
        let webdav: Value =
            serde_json::from_str(&store.get_setting(WEBDAV_CONFIG_KEY).unwrap().unwrap()).unwrap();
        assert_eq!(webdav["password"], "");
        assert_eq!(webdav["url"], "https://dav.example");
        assert_eq!(
            provider.get_secret(GITHUB_TOKEN_ALIAS).unwrap(),
            "github-sentinel"
        );
        assert_eq!(
            provider.get_secret(WEBDAV_PASSWORD_ALIAS).unwrap(),
            "dav-sentinel"
        );
    }

    #[test]
    fn vault_failure_preserves_all_legacy_settings() {
        let (_dir, store) = test_store();
        store
            .set_setting(GITHUB_TOKEN_KEY, "github-sentinel")
            .unwrap();
        let provider = MockCredentialProvider::default();
        // Force the first set to fail by using an existing conflicting alias.
        provider
            .set_secret(GITHUB_TOKEN_ALIAS, "different", None)
            .unwrap();

        assert!(migrate_legacy_credentials(&store, &provider).is_err());
        assert_eq!(
            store.get_setting(GITHUB_TOKEN_KEY).unwrap().as_deref(),
            Some("github-sentinel")
        );
    }

    #[test]
    fn malformed_webdav_prevents_partial_migration() {
        let (_dir, store) = test_store();
        store
            .set_setting(GITHUB_TOKEN_KEY, "github-sentinel")
            .unwrap();
        store.set_setting(WEBDAV_CONFIG_KEY, "{bad-json").unwrap();
        let provider = MockCredentialProvider::default();

        assert!(migrate_legacy_credentials(&store, &provider).is_err());
        assert_eq!(
            store.get_setting(GITHUB_TOKEN_KEY).unwrap().as_deref(),
            Some("github-sentinel")
        );
        assert!(!provider.alias_exists(GITHUB_TOKEN_ALIAS).unwrap());
    }

    use crate::core::skill_store::SkillStore;
}
