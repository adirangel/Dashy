use serde::Serialize;

use crate::dashboard::models::{ProviderId, ProviderStatus};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupDefinition {
    pub provider: ProviderId,
    pub publisher: &'static str,
    pub package_id: &'static str,
    pub install_command: String,
    pub install_url: &'static str,
    pub login_command: &'static str,
}

impl ProviderSetupDefinition {
    pub fn for_provider(provider: ProviderId) -> Self {
        let (publisher, package_id, install_url, login_command) = match provider {
            ProviderId::Claude => (
                "Anthropic",
                "Anthropic.ClaudeCode",
                "https://code.claude.com/docs/en/setup",
                "claude auth login --claudeai",
            ),
            ProviderId::Codex => (
                "OpenAI",
                "OpenAI.Codex",
                "https://learn.chatgpt.com/docs/codex/cli",
                "codex login",
            ),
            ProviderId::GitHub => (
                "GitHub",
                "GitHub.cli",
                "https://cli.github.com/",
                "gh auth login --web",
            ),
        };
        let install_command = format!(
            "winget install --id {package_id} --exact --source winget --interactive --accept-source-agreements --accept-package-agreements"
        );
        Self {
            provider,
            publisher,
            package_id,
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
