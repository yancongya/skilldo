//! Secret access backed by the persistent `bwvault` CLI.
//!
//! This module deliberately keeps command output and parser diagnostics out of
//! logs and returned errors. The secret exists only in the captured stdout and
//! the returned value in memory.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::Value;

/// Stable alias used by SkillDo for its GitHub publishing credential.
pub const GITHUB_TOKEN_ALIAS: &str = "skilldo.github.token";
pub const WEBDAV_PASSWORD_ALIAS: &str = "skilldo.webdav.password";

const BWVAULT_COMMAND: &str = "bwvault";
const SAFE_ERROR: &str = "credential provider could not retrieve the requested credential";

/// Resolves a stable credential alias to its secret value.
pub trait CredentialProvider: Send + Sync {
    fn get_secret(&self, alias: &str) -> Result<String, CredentialError>;

    fn alias_exists(&self, alias: &str) -> Result<bool, CredentialError>;

    /// Stores a secret under a stable alias. The secret is passed only through
    /// the command's stdin and must never be included in arguments or logs.
    fn set_secret(
        &self,
        alias: &str,
        secret: &str,
        username: Option<&str>,
    ) -> Result<(), CredentialError>;
}

/// A deliberately detail-free error, so command/parser failures cannot leak
/// stdout, stderr, or secret material through an error chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredentialError;

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(SAFE_ERROR)
    }
}

impl std::error::Error for CredentialError {}

/// Small fake provider for consumers' unit tests.
#[derive(Debug, Clone, Default)]
pub struct MockCredentialProvider {
    credentials: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>,
}

impl MockCredentialProvider {
    pub fn new(credentials: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            credentials: std::sync::Arc::new(std::sync::Mutex::new(
                credentials.into_iter().collect(),
            )),
        }
    }
}

impl CredentialProvider for MockCredentialProvider {
    fn get_secret(&self, alias: &str) -> Result<String, CredentialError> {
        self.credentials
            .lock()
            .map_err(|_| CredentialError)?
            .get(alias)
            .cloned()
            .ok_or(CredentialError)
    }

    fn alias_exists(&self, alias: &str) -> Result<bool, CredentialError> {
        Ok(self
            .credentials
            .lock()
            .map_err(|_| CredentialError)?
            .contains_key(alias))
    }

    fn set_secret(
        &self,
        alias: &str,
        secret: &str,
        _username: Option<&str>,
    ) -> Result<(), CredentialError> {
        self.credentials
            .lock()
            .map_err(|_| CredentialError)?
            .insert(alias.to_owned(), secret.to_owned());
        Ok(())
    }
}

/// Result of running the configured CLI. `stderr` is captured by the process
/// API but never inspected or surfaced.
#[derive(Debug)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: Vec<u8>,
}

/// Injectable process boundary, useful for tests without invoking a real vault.
pub trait CommandRunner: Send + Sync {
    fn run(
        &self,
        executable: &str,
        args: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<CommandOutput, CommandRunError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandRunError {
    Start,
    StdinWrite,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(
        &self,
        executable: &str,
        args: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<CommandOutput, CommandRunError> {
        let mut command = Command::new(executable);
        command
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        let mut child = command.spawn().map_err(|_| CommandRunError::Start)?;
        if let Some(input) = stdin {
            let write_result = child
                .stdin
                .take()
                .ok_or(CommandRunError::StdinWrite)?
                .write_all(input);
            if write_result.is_err() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CommandRunError::StdinWrite);
            }
        }
        let output = child
            .wait_with_output()
            .map_err(|_| CommandRunError::Start)?;
        Ok(CommandOutput {
            success: output.status.success(),
            stdout: output.stdout,
        })
    }
}

pub struct BwVaultCredentialProvider<R = SystemCommandRunner> {
    runner: R,
}

impl Default for BwVaultCredentialProvider<SystemCommandRunner> {
    fn default() -> Self {
        Self::new(SystemCommandRunner)
    }
}

impl<R> BwVaultCredentialProvider<R> {
    pub fn new(runner: R) -> Self {
        Self { runner }
    }
}

impl<R: CommandRunner> CredentialProvider for BwVaultCredentialProvider<R> {
    fn get_secret(&self, alias: &str) -> Result<String, CredentialError> {
        let output = self
            .runner
            .run(
                BWVAULT_COMMAND,
                &["credential", "get", "--alias", alias, "--reveal", "--json"],
                None,
            )
            .map_err(|_| CredentialError)?;

        if !output.success {
            return Err(CredentialError);
        }

        let parsed: Value = serde_json::from_slice(&output.stdout).map_err(|_| CredentialError)?;
        let secret = parsed
            .get("secret")
            .and_then(Value::as_str)
            .filter(|secret| !secret.trim().is_empty())
            .ok_or(CredentialError)?;

        Ok(secret.to_owned())
    }

    fn alias_exists(&self, alias: &str) -> Result<bool, CredentialError> {
        let output = self
            .runner
            .run(BWVAULT_COMMAND, &["credential", "list", "--json"], None)
            .map_err(|_| CredentialError)?;
        if !output.success {
            return Err(CredentialError);
        }

        let parsed: Value = serde_json::from_slice(&output.stdout).map_err(|_| CredentialError)?;
        let items = parsed
            .get("items")
            .and_then(Value::as_array)
            .ok_or(CredentialError)?;
        let aliases = items
            .iter()
            .map(|item| {
                item.get("alias")
                    .and_then(Value::as_str)
                    .ok_or(CredentialError)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(aliases.into_iter().any(|item_alias| item_alias == alias))
    }

    fn set_secret(
        &self,
        alias: &str,
        secret: &str,
        username: Option<&str>,
    ) -> Result<(), CredentialError> {
        if secret.is_empty() {
            return Err(CredentialError);
        }
        let mut args = vec!["credential", "set", "--alias", alias];
        if let Some(username) = username {
            args.extend(["--username", username]);
        }
        args.push("--apply");

        let output = self
            .runner
            .run(BWVAULT_COMMAND, &args, Some(secret.as_bytes()))
            .map_err(|_| CredentialError)?;
        if !output.success {
            return Err(CredentialError);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeRunner {
        response: Arc<Mutex<Result<CommandOutput, CommandRunError>>>,
        calls: Arc<Mutex<Vec<(String, Vec<String>, Option<Vec<u8>>)>>>,
    }

    impl FakeRunner {
        fn returning(response: Result<CommandOutput, CommandRunError>) -> Self {
            Self {
                response: Arc::new(Mutex::new(response)),
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(
            &self,
            executable: &str,
            args: &[&str],
            stdin: Option<&[u8]>,
        ) -> Result<CommandOutput, CommandRunError> {
            self.calls.lock().unwrap().push((
                executable.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect(),
                stdin.map(ToOwned::to_owned),
            ));
            let mut response = self.response.lock().unwrap();
            match response.as_mut() {
                Ok(output) => Ok(CommandOutput {
                    success: output.success,
                    stdout: output.stdout.clone(),
                }),
                Err(error) => Err(*error),
            }
        }
    }

    fn output(success: bool, stdout: &str) -> Result<CommandOutput, CommandRunError> {
        Ok(CommandOutput {
            success,
            stdout: stdout.as_bytes().to_vec(),
        })
    }

    #[test]
    fn invokes_bwvault_with_expected_arguments_and_extracts_secret() {
        let runner = FakeRunner::returning(output(true, r#"{"secret":"value-123"}"#));
        let provider = BwVaultCredentialProvider::new(runner.clone());

        assert_eq!(provider.get_secret("service.key").unwrap(), "value-123");
        assert_eq!(
            runner.calls.lock().unwrap()[0],
            (
                "bwvault".to_owned(),
                vec![
                    "credential",
                    "get",
                    "--alias",
                    "service.key",
                    "--reveal",
                    "--json"
                ]
                .into_iter()
                .map(str::to_owned)
                .collect(),
                None
            )
        );
    }

    #[test]
    fn rejects_missing_command_nonzero_malformed_and_empty_secret() {
        let cases = [
            Err(CommandRunError::Start),
            output(false, r#"{"secret":"sensitive stderr text"}"#),
            output(true, "not json"),
            output(true, r#"{"secret":""}"#),
            output(true, r#"{"secret":"   "}"#),
            output(true, r#"{"other":"value"}"#),
        ];
        for response in cases {
            let provider = BwVaultCredentialProvider::new(FakeRunner::returning(response));
            let error = provider.get_secret("service.key").unwrap_err();
            assert_eq!(error.to_string(), SAFE_ERROR);
            assert!(!error.to_string().contains("sensitive"));
        }
    }

    #[test]
    fn mock_provider_returns_only_configured_aliases() {
        let provider =
            MockCredentialProvider::new([("test.alias".to_owned(), "secret".to_owned())]);
        assert_eq!(provider.get_secret("test.alias").unwrap(), "secret");
        assert!(provider.get_secret("missing.alias").is_err());
        assert!(provider.alias_exists("test.alias").unwrap());
        assert!(!provider.alias_exists("missing.alias").unwrap());
        provider
            .set_secret("missing.alias", "created-secret", None)
            .unwrap();
        assert!(provider.alias_exists("missing.alias").unwrap());
        assert_eq!(
            provider.get_secret("missing.alias").unwrap(),
            "created-secret"
        );
    }

    #[test]
    fn alias_exists_lists_and_matches_aliases_without_revealing_secrets() {
        let runner = FakeRunner::returning(output(
            true,
            r#"{"items":[{"alias":"first.alias"},{"alias":"target.alias"}]}"#,
        ));
        let provider = BwVaultCredentialProvider::new(runner.clone());

        assert!(provider.alias_exists("target.alias").unwrap());
        assert!(!provider.alias_exists("absent.alias").unwrap());
        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls[0].0, "bwvault");
        assert_eq!(calls[0].1, ["credential", "list", "--json"]);
        assert_eq!(calls[0].2, None);
    }

    #[test]
    fn alias_exists_fails_closed_on_command_and_json_errors() {
        let failures = [
            Err(CommandRunError::Start),
            output(false, r#"{"items":[{"alias":"private.alias"}]}"#),
            output(true, "not json"),
            output(true, r#"{"items":null}"#),
            output(true, r#"{"items":[{"name":"private.alias"}]}"#),
        ];
        for response in failures {
            let provider = BwVaultCredentialProvider::new(FakeRunner::returning(response));
            let error = provider.alias_exists("private.alias").unwrap_err();
            assert_eq!(error.to_string(), SAFE_ERROR);
            assert!(!error.to_string().contains("private.alias"));
        }
    }

    #[test]
    fn set_passes_secret_only_over_stdin_and_supports_optional_username() {
        let runner = FakeRunner::returning(output(true, "saved"));
        let provider = BwVaultCredentialProvider::new(runner.clone());
        let secret = "raw secret\nwith bytes";

        provider
            .set_secret("service.key", secret, Some("service-user"))
            .unwrap();
        provider
            .set_secret("another.key", "another-secret", None)
            .unwrap();

        let calls = runner.calls.lock().unwrap();
        assert_eq!(calls[0].0, "bwvault");
        assert_eq!(
            calls[0].1,
            [
                "credential",
                "set",
                "--alias",
                "service.key",
                "--username",
                "service-user",
                "--apply"
            ]
        );
        assert_eq!(calls[0].2.as_deref(), Some(secret.as_bytes()));
        assert!(!calls[0].1.iter().any(|arg| arg.contains(secret)));
        assert_eq!(
            calls[1].1,
            ["credential", "set", "--alias", "another.key", "--apply"]
        );
        assert_eq!(calls[1].2.as_deref(), Some(b"another-secret".as_slice()));
    }

    #[test]
    fn set_failures_are_sanitized_for_start_write_and_nonzero_exit() {
        let failures = [
            Err(CommandRunError::Start),
            Err(CommandRunError::StdinWrite),
            output(false, "secret echoed by failed command"),
        ];
        for response in failures {
            let provider = BwVaultCredentialProvider::new(FakeRunner::returning(response));
            let error = provider
                .set_secret("service.key", "top-secret-value", None)
                .unwrap_err();
            assert_eq!(error.to_string(), SAFE_ERROR);
            assert!(!error.to_string().contains("top-secret-value"));
            assert!(!error.to_string().contains("echoed"));
        }
    }
}
