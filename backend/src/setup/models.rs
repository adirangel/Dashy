use serde::Serialize;

use crate::dashboard::models::{ProviderId, ProviderStatus};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderInstallKind {
    Winget,
    ManualUrl,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupDefinition {
    pub provider: ProviderId,
    pub publisher: &'static str,
    pub package_id: Option<&'static str>,
    pub install_kind: ProviderInstallKind,
    pub install_command: String,
    pub install_url: &'static str,
    pub login_command: &'static str,
}

impl ProviderSetupDefinition {
    pub fn for_provider(provider: ProviderId) -> Self {
        let (publisher, package_id, install_url, login_command) = match provider {
            ProviderId::Claude => (
                "Anthropic",
                Some("Anthropic.ClaudeCode"),
                "https://code.claude.com/docs/en/setup",
                "claude auth login --claudeai",
            ),
            ProviderId::Codex => (
                "OpenAI",
                Some("OpenAI.Codex"),
                "https://learn.chatgpt.com/docs/codex/cli",
                "codex login",
            ),
            ProviderId::GitHub => (
                "GitHub",
                Some("GitHub.cli"),
                "https://cli.github.com/",
                "gh auth login --web",
            ),
            ProviderId::Grok => (
                "xAI",
                Some("xAI.GrokBuild"),
                "https://docs.x.ai/build/overview",
                "grok login",
            ),
            // Cursor's CLI has no winget package; installation goes through the
            // official guide, which the frontend opens from its exact-URL allowlist.
            ProviderId::Cursor => (
                "Anysphere",
                None,
                "https://cursor.com/docs/cli/installation",
                "cursor-agent login",
            ),
        };
        let install_kind = match package_id {
            Some(_) => ProviderInstallKind::Winget,
            None => ProviderInstallKind::ManualUrl,
        };
        let install_command = match package_id {
            Some(package_id) => format!(
                "winget install --id {package_id} --exact --source winget --interactive --accept-source-agreements --accept-package-agreements"
            ),
            // Display-only reference for the consent card; Dashy never executes it.
            None => "irm 'https://cursor.com/install?win32=true' | iex".to_owned(),
        };
        Self {
            provider,
            publisher,
            package_id,
            install_kind,
            install_command,
            install_url,
            login_command,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderRepairAction {
    Install,
    Login,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupState {
    pub definition: ProviderSetupDefinition,
    pub status: ProviderStatus,
    pub repair_action: Option<ProviderRepairAction>,
}
