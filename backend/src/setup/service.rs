use std::sync::Arc;

use crate::dashboard::{
    models::ProviderId,
    process::{AllowedProgram, VisibleProcessError, VisibleRunner},
};
use crate::setup::models::{HostPlatform, ProviderInstallKind, ProviderSetupDefinition};

pub struct SetupService {
    runner: Arc<dyn VisibleRunner>,
}

impl SetupService {
    pub fn new(runner: Arc<dyn VisibleRunner>) -> Self {
        Self { runner }
    }

    pub async fn install(&self, provider: ProviderId) -> Result<(), String> {
        self.install_on(provider, HostPlatform::current()).await
    }

    pub async fn install_on(
        &self,
        provider: ProviderId,
        platform: HostPlatform,
    ) -> Result<(), String> {
        // Manual-URL providers are installed through their official guide, which the
        // frontend opens from its exact-URL allowlist; defense in depth keeps this
        // path from ever spawning a process for them.
        let package = ProviderSetupDefinition::for_provider_on(provider, platform).package;
        let Some(args) = package.install_args() else {
            return Err("provider does not support automated install".to_owned());
        };
        let program = match package.install_kind() {
            ProviderInstallKind::Winget => AllowedProgram::Winget,
            ProviderInstallKind::Homebrew => AllowedProgram::Brew,
            ProviderInstallKind::ManualUrl => {
                return Err("provider does not support automated install".to_owned())
            }
        };
        self.runner
            .run_visible(program, args)
            .await
            .map_err(sanitize_setup_error)
    }

    pub async fn login(&self, provider: ProviderId) -> Result<(), String> {
        let (program, args) = match provider {
            ProviderId::Claude => (AllowedProgram::Claude, vec!["auth", "login", "--claudeai"]),
            ProviderId::Codex => (AllowedProgram::Codex, vec!["login"]),
            ProviderId::GitHub => (AllowedProgram::Gh, vec!["auth", "login", "--web"]),
            ProviderId::Grok => (AllowedProgram::Grok, vec!["login"]),
            ProviderId::Cursor => (AllowedProgram::CursorAgent, vec!["login"]),
        };
        self.runner
            .run_visible(program, args.into_iter().map(str::to_owned).collect())
            .await
            .map_err(sanitize_setup_error)
    }
}

fn sanitize_setup_error(error: VisibleProcessError) -> String {
    match error {
        VisibleProcessError::UnsupportedPlatform => {
            "provider setup is not supported on this platform"
        }
        VisibleProcessError::NotInstalled => "provider tool is not installed",
        VisibleProcessError::NoTerminal => {
            "no terminal application was found to run the provider tool"
        }
        VisibleProcessError::Failed => "provider setup process did not complete",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;

    use super::SetupService;
    use crate::dashboard::{
        models::ProviderId,
        process::{AllowedProgram, VisibleProcessError, VisibleRunner},
    };
    use crate::setup::models::HostPlatform;

    #[derive(Default)]
    struct RecordingRunner(std::sync::Mutex<Vec<(AllowedProgram, Vec<String>)>>);

    impl RecordingRunner {
        fn calls(&self) -> Vec<(AllowedProgram, Vec<String>)> {
            self.0.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl VisibleRunner for RecordingRunner {
        async fn run_visible(
            &self,
            program: AllowedProgram,
            args: Vec<String>,
        ) -> Result<(), VisibleProcessError> {
            self.0.lock().unwrap().push((program, args));
            Ok(())
        }
    }

    #[tokio::test]
    async fn install_uses_only_the_exact_codex_winget_package() {
        let runner = Arc::new(RecordingRunner::default());
        let service = SetupService::new(runner.clone());
        service
            .install_on(ProviderId::Codex, HostPlatform::Windows)
            .await
            .unwrap();
        assert_eq!(
            runner.calls(),
            vec![(
                AllowedProgram::Winget,
                vec![
                    "install",
                    "--id",
                    "OpenAI.Codex",
                    "--exact",
                    "--source",
                    "winget",
                    "--interactive",
                    "--accept-source-agreements",
                    "--accept-package-agreements",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect()
            )]
        );
    }

    #[tokio::test]
    async fn install_uses_only_the_exact_grok_winget_package() {
        let runner = Arc::new(RecordingRunner::default());
        let service = SetupService::new(runner.clone());
        service
            .install_on(ProviderId::Grok, HostPlatform::Windows)
            .await
            .unwrap();
        let calls = runner.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, AllowedProgram::Winget);
        assert_eq!(calls[0].1[2], "xAI.GrokBuild");
    }

    #[tokio::test]
    async fn macos_install_uses_only_the_exact_homebrew_package() {
        let runner = Arc::new(RecordingRunner::default());
        let service = SetupService::new(runner.clone());
        service
            .install_on(ProviderId::Claude, HostPlatform::MacOs)
            .await
            .unwrap();
        service
            .install_on(ProviderId::GitHub, HostPlatform::MacOs)
            .await
            .unwrap();
        assert_eq!(
            runner.calls(),
            vec![
                (
                    AllowedProgram::Brew,
                    vec!["install".into(), "--cask".into(), "claude-code".into()]
                ),
                (AllowedProgram::Brew, vec!["install".into(), "gh".into()]),
            ]
        );
    }

    #[tokio::test]
    async fn cursor_grok_on_macos_and_every_linux_install_never_spawn_a_process() {
        let runner = Arc::new(RecordingRunner::default());
        let service = SetupService::new(runner.clone());

        for platform in [
            HostPlatform::Windows,
            HostPlatform::MacOs,
            HostPlatform::Linux,
        ] {
            assert_eq!(
                service.install_on(ProviderId::Cursor, platform).await,
                Err("provider does not support automated install".to_owned())
            );
        }
        assert_eq!(
            service
                .install_on(ProviderId::Grok, HostPlatform::MacOs)
                .await,
            Err("provider does not support automated install".to_owned())
        );
        for provider in ProviderId::ALL {
            assert_eq!(
                service.install_on(provider, HostPlatform::Linux).await,
                Err("provider does not support automated install".to_owned())
            );
        }
        assert!(runner.calls().is_empty());
    }

    #[tokio::test]
    async fn login_uses_the_official_subscription_commands() {
        let runner = Arc::new(RecordingRunner::default());
        let service = SetupService::new(runner.clone());
        service.login(ProviderId::Claude).await.unwrap();
        service.login(ProviderId::Codex).await.unwrap();
        service.login(ProviderId::GitHub).await.unwrap();
        service.login(ProviderId::Grok).await.unwrap();
        service.login(ProviderId::Cursor).await.unwrap();
        assert_eq!(
            runner.calls(),
            vec![
                (
                    AllowedProgram::Claude,
                    vec!["auth".into(), "login".into(), "--claudeai".into()]
                ),
                (AllowedProgram::Codex, vec!["login".into()]),
                (
                    AllowedProgram::Gh,
                    vec!["auth".into(), "login".into(), "--web".into()]
                ),
                (AllowedProgram::Grok, vec!["login".into()]),
                (AllowedProgram::CursorAgent, vec!["login".into()]),
            ]
        );
    }
}
