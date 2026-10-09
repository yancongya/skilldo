//! Secret access backed by the persistent `bwvault` CLI.
//!
//! This module deliberately keeps command output and parser diagnostics out of
//! logs and returned errors. The secret exists only in the captured stdout and
//! the returned value in memory.

use std::process::Command;

use serde_json::Value;

/// Stable alias used by SkillDo for its GitHub publishing credential.
pub const GITHUB_TOKEN_ALIAS: &str = "skilldo.github.token";

const BWVAULT_COMMAND: &str = "bwvault";
const SAFE_ERROR: &str = "credential provider could not retrieve the requested credential";

/// Resolves a stable credential alias to its secret value.
pub trait CredentialProvider: Send + Sync {
    fn get_secret(&self, alias: &str) -> Result<String, CredentialError>;
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
    credentials: std::collections::HashMap<String, String>,
}

impl MockCredentialProvider {
    pub fn new(credentials: impl IntoIterator<Item = (String, String)>) -> Self {
        Self {
            credentials: credentials.into_iter().collect(),
        }
    }
}

impl CredentialProvider for MockCredentialProvider {
    fn get_secret(&self, alias: &str) -> Result<String, CredentialError> {
        self.credentials.get(alias).cloned().ok_or(CredentialError)
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
    fn run(&self, executable: &str, args: &[&str]) -> Result<CommandOutput, ()>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, executable: &str, args: &[&str]) -> Result<CommandOutput, ()> {
        let output = Command::new(executable)
            .args(args)
            .output()
            .map_err(|_| ())?;
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakeRunner {
        response: Arc<Mutex<Result<CommandOutput, ()>>>,
        calls: Arc<Mutex<Vec<(String, Vec<String>)>>>,
    }

    impl FakeRunner {
        fn returning(response: Result<CommandOutput, ()>) -> Self {
            Self {
                response: Arc::new(Mutex::new(response)),
                calls: Arc::new(Mutex::new(Vec::new())),
            }
        }
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, executable: &str, args: &[&str]) -> Result<CommandOutput, ()> {
            self.calls.lock().unwrap().push((
                executable.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect(),
            ));
            let mut response = self.response.lock().unwrap();
            match response.as_mut() {
                Ok(output) => Ok(CommandOutput {
                    success: output.success,
                    stdout: output.stdout.clone(),
                }),
                Err(()) => Err(()),
            }
        }
    }

    fn output(success: bool, stdout: &str) -> Result<CommandOutput, ()> {
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
                .collect()
            )
        );
    }

    #[test]
    fn rejects_missing_command_nonzero_malformed_and_empty_secret() {
        let cases = [
            Err(()),
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
    }
}
