use serde::Serialize;

use crate::dashboard::models::{ProviderId, ProviderStatus};

/// The operating system Dashy is running on, as far as provider setup cares.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostPlatform {
    Windows,
    MacOs,
    Linux,
}

impl HostPlatform {
    pub const fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderInstallKind {
    Winget,
    Homebrew,
    ManualUrl,
}

/// How a provider CLI is installed on one platform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallPackage {
    /// A WinGet package id, installed with the exact-id, interactive command.
    Winget(&'static str),
    /// A Homebrew formula.
    BrewFormula(&'static str),
    /// A Homebrew cask.
    BrewCask(&'static str),
    /// No package manager entry: the official guide is opened instead.
    Manual,
}

impl InstallPackage {
    pub fn package_id(self) -> Option<&'static str> {
        match self {
            Self::Winget(id) | Self::BrewFormula(id) | Self::BrewCask(id) => Some(id),
            Self::Manual => None,
        }
    }

    pub fn install_kind(self) -> ProviderInstallKind {
        match self {
            Self::Winget(_) => ProviderInstallKind::Winget,
            Self::BrewFormula(_) | Self::BrewCask(_) => ProviderInstallKind::Homebrew,
            Self::Manual => ProviderInstallKind::ManualUrl,
        }
    }

    /// The exact argument list Dashy runs in the visible terminal.
    pub fn install_args(self) -> Option<Vec<String>> {
        let args: Vec<&str> = match self {
            Self::Winget(id) => vec![
                "install",
                "--id",
                id,
                "--exact",
                "--source",
                "winget",
                "--interactive",
                "--accept-source-agreements",
                "--accept-package-agreements",
            ],
            Self::BrewFormula(id) => vec!["install", id],
            Self::BrewCask(id) => vec!["install", "--cask", id],
            Self::Manual => return None,
        };
        Some(args.into_iter().map(str::to_owned).collect())
    }

    fn command_name(self) -> Option<&'static str> {
        match self {
            Self::Winget(_) => Some("winget"),
            Self::BrewFormula(_) | Self::BrewCask(_) => Some("brew"),
            Self::Manual => None,
        }
    }

    /// The command shown on the consent card, exactly as it will run.
    pub fn install_command(self) -> Option<String> {
        let name = self.command_name()?;
        let args = self.install_args()?;
        Some(
            std::iter::once(name.to_owned())
                .chain(args)
                .collect::<Vec<_>>()
                .join(" "),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupDefinition {
    pub provider: ProviderId,
    pub publisher: &'static str,
    pub package_id: Option<&'static str>,
    pub install_kind: ProviderInstallKind,
    pub install_command: Option<String>,
    pub install_url: &'static str,
    pub login_command: &'static str,
    #[serde(skip)]
    pub package: InstallPackage,
}

impl ProviderSetupDefinition {
    pub fn for_provider(provider: ProviderId) -> Self {
        Self::for_provider_on(provider, HostPlatform::current())
    }

    pub fn for_provider_on(provider: ProviderId, platform: HostPlatform) -> Self {
        let (publisher, install_url, login_command) = match provider {
            ProviderId::Claude => (
                "Anthropic",
                "https://code.claude.com/docs/en/setup",
                "claude auth login --claudeai",
            ),
            ProviderId::Codex => (
                "OpenAI",
                "https://learn.chatgpt.com/docs/codex/cli",
                "codex login",
            ),
            ProviderId::GitHub => ("GitHub", "https://cli.github.com/", "gh auth login --web"),
            ProviderId::Grok => ("xAI", "https://docs.x.ai/build/overview", "grok login"),
            ProviderId::Cursor => (
                "Anysphere",
                "https://cursor.com/docs/cli/installation",
                "cursor-agent login",
            ),
        };
        let package = install_package(provider, platform);
        Self {
            provider,
            publisher,
            package_id: package.package_id(),
            install_kind: package.install_kind(),
            // Manual-URL providers install through the official guide, so their
            // consent card shows no command at all.
            install_command: package.install_command(),
            install_url,
            login_command,
            package,
        }
    }
}

/// Only official, exact package identifiers. Linux distributions package these
/// CLIs in too many different ways, so every Linux install opens the official
/// guide; Cursor's CLI has no package on any platform.
fn install_package(provider: ProviderId, platform: HostPlatform) -> InstallPackage {
    match (provider, platform) {
        (ProviderId::Claude, HostPlatform::Windows) => {
            InstallPackage::Winget("Anthropic.ClaudeCode")
        }
        (ProviderId::Claude, HostPlatform::MacOs) => InstallPackage::BrewCask("claude-code"),
        (ProviderId::Codex, HostPlatform::Windows) => InstallPackage::Winget("OpenAI.Codex"),
        (ProviderId::Codex, HostPlatform::MacOs) => InstallPackage::BrewCask("codex"),
        (ProviderId::GitHub, HostPlatform::Windows) => InstallPackage::Winget("GitHub.cli"),
        (ProviderId::GitHub, HostPlatform::MacOs) => InstallPackage::BrewFormula("gh"),
        (ProviderId::Grok, HostPlatform::Windows) => InstallPackage::Winget("xAI.GrokBuild"),
        (ProviderId::Grok, HostPlatform::MacOs)
        | (ProviderId::Cursor, _)
        | (_, HostPlatform::Linux) => InstallPackage::Manual,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_installs_through_exact_winget_packages() {
        let codex =
            ProviderSetupDefinition::for_provider_on(ProviderId::Codex, HostPlatform::Windows);
        assert_eq!(codex.install_kind, ProviderInstallKind::Winget);
        assert_eq!(codex.package_id, Some("OpenAI.Codex"));
        assert_eq!(
            codex.install_command.as_deref(),
            Some("winget install --id OpenAI.Codex --exact --source winget --interactive --accept-source-agreements --accept-package-agreements")
        );
        assert_eq!(
            ProviderSetupDefinition::for_provider_on(ProviderId::Grok, HostPlatform::Windows)
                .package_id,
            Some("xAI.GrokBuild")
        );
    }

    #[test]
    fn macos_installs_through_homebrew_where_an_official_package_exists() {
        let claude =
            ProviderSetupDefinition::for_provider_on(ProviderId::Claude, HostPlatform::MacOs);
        assert_eq!(claude.install_kind, ProviderInstallKind::Homebrew);
        assert_eq!(claude.package_id, Some("claude-code"));
        assert_eq!(
            claude.install_command.as_deref(),
            Some("brew install --cask claude-code")
        );
        let gh = ProviderSetupDefinition::for_provider_on(ProviderId::GitHub, HostPlatform::MacOs);
        assert_eq!(gh.install_command.as_deref(), Some("brew install gh"));

        let grok = ProviderSetupDefinition::for_provider_on(ProviderId::Grok, HostPlatform::MacOs);
        assert_eq!(grok.install_kind, ProviderInstallKind::ManualUrl);
        assert_eq!(grok.package_id, None);
        assert_eq!(grok.install_command, None);
    }

    #[test]
    fn linux_and_cursor_always_open_the_official_guide() {
        for provider in ProviderId::ALL {
            let definition =
                ProviderSetupDefinition::for_provider_on(provider, HostPlatform::Linux);
            assert_eq!(
                definition.install_kind,
                ProviderInstallKind::ManualUrl,
                "{provider:?}"
            );
            assert_eq!(definition.package_id, None);
            assert_eq!(definition.install_command, None);
            assert_eq!(definition.package.install_args(), None);
        }
        for platform in [HostPlatform::Windows, HostPlatform::MacOs] {
            let cursor = ProviderSetupDefinition::for_provider_on(ProviderId::Cursor, platform);
            assert_eq!(cursor.install_kind, ProviderInstallKind::ManualUrl);
            assert_eq!(
                cursor.install_url,
                "https://cursor.com/docs/cli/installation"
            );
        }
    }

    #[test]
    fn login_commands_and_guide_urls_do_not_depend_on_the_platform() {
        for provider in ProviderId::ALL {
            let windows = ProviderSetupDefinition::for_provider_on(provider, HostPlatform::Windows);
            let macos = ProviderSetupDefinition::for_provider_on(provider, HostPlatform::MacOs);
            let linux = ProviderSetupDefinition::for_provider_on(provider, HostPlatform::Linux);
            assert_eq!(windows.login_command, macos.login_command);
            assert_eq!(windows.login_command, linux.login_command);
            assert_eq!(windows.install_url, macos.install_url);
            assert_eq!(windows.install_url, linux.install_url);
            assert_eq!(windows.publisher, linux.publisher);
        }
    }

    #[test]
    fn the_serialized_definition_hides_the_internal_package_variant() {
        let value = serde_json::to_value(ProviderSetupDefinition::for_provider_on(
            ProviderId::GitHub,
            HostPlatform::MacOs,
        ))
        .unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "installCommand",
                "installKind",
                "installUrl",
                "loginCommand",
                "packageId",
                "provider",
                "publisher",
            ]
        );
        assert_eq!(value["installKind"], "homebrew");
    }
}
