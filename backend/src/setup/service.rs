use std::sync::Arc;

use crate::dashboard::{
    diagnostics::{DiagnosticsSink, NoopDiagnostics, RefreshRecord},
    models::ProviderId,
    process::{AllowedProgram, VisibleProcessError, VisibleRunner},
};
use crate::setup::models::{HostPlatform, ProviderInstallKind, ProviderSetupDefinition};

pub struct SetupService {
    runner: Arc<dyn VisibleRunner>,
    diagnostics: Arc<dyn DiagnosticsSink>,
}

impl SetupService {
    pub fn new(runner: Arc<dyn VisibleRunner>) -> Self {
        Self {
            runner,
            diagnostics: Arc::new(NoopDiagnostics),
        }
    }

    pub fn with_diagnostics(mut self, diagnostics: Arc<dyn DiagnosticsSink>) -> Self {
        self.diagnostics = diagnostics;
        self
    }

    fn setup_error(
        &self,
        provider: ProviderId,
        stage: &str,
        started: std::time::Instant,
        error: VisibleProcessError,
    ) -> String {
        // Enum variants and numeric codes only: never record CLI output or paths.
        self.diagnostics.record(&RefreshRecord {
            at: chrono::Utc::now(),
            provider,
            error: Some(format!("setup {stage} failed: {error:?}")),
            duration: started.elapsed(),
        });
        sanitize_setup_error(error)
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
        let started = std::time::Instant::now();
        match self.runner.run_visible(program, args).await {
            Ok(()) => {}
            // WinGet reports this when an already installed package has no
            // update. Still repair and verify it: an absent alias can be fixed.
            Err(VisibleProcessError::NonZero(code))
                if program == AllowedProgram::Winget && code as u32 == 0x8A15002B => {}
            Err(error) => return Err(self.setup_error(provider, "install", started, error)),
        }
        let provider_program = match provider {
            ProviderId::Claude => AllowedProgram::Claude,
            ProviderId::Codex => AllowedProgram::Codex,
            ProviderId::GitHub => AllowedProgram::Gh,
            ProviderId::Grok => AllowedProgram::Grok,
            ProviderId::Cursor => AllowedProgram::CursorAgent,
        };
        self.runner
            .finish_install(provider_program)
            .await
            .map_err(|error| self.setup_error(provider, "command verification", started, error))
    }

    pub async fn login(&self, provider: ProviderId) -> Result<(), String> {
        let started = std::time::Instant::now();
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
            .map_err(|error| self.setup_error(provider, "login", started, error))
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
        VisibleProcessError::Failed
        | VisibleProcessError::Spawn { .. }
        | VisibleProcessError::NonZero(_) => "provider setup process did not complete",
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

    struct FailingFinalization;

    #[async_trait]
    impl VisibleRunner for FailingFinalization {
        async fn run_visible(
            &self,
            _: AllowedProgram,
            _: Vec<String>,
        ) -> Result<(), VisibleProcessError> {
            Ok(())
        }
        async fn finish_install(&self, program: AllowedProgram) -> Result<(), VisibleProcessError> {
            assert_eq!(program, AllowedProgram::Codex);
            Err(VisibleProcessError::NotInstalled)
        }
    }

    #[tokio::test]
    async fn installer_success_is_not_success_when_the_shell_command_is_missing() {
        let service = SetupService::new(Arc::new(FailingFinalization));
        assert_eq!(
            service
                .install_on(ProviderId::Codex, HostPlatform::Windows)
                .await,
            Err("provider tool is not installed".to_owned())
        );
    }

    struct InstallerResult {
        result: Result<(), VisibleProcessError>,
        verification: Result<(), VisibleProcessError>,
        verified: std::sync::Mutex<Vec<AllowedProgram>>,
    }

    #[async_trait]
    impl VisibleRunner for InstallerResult {
        async fn run_visible(
            &self,
            _: AllowedProgram,
            _: Vec<String>,
        ) -> Result<(), VisibleProcessError> {
            self.result
        }
        async fn finish_install(&self, program: AllowedProgram) -> Result<(), VisibleProcessError> {
            self.verified.lock().unwrap().push(program);
            self.verification
        }
    }

    #[derive(Default)]
    struct SetupDiagnostics(std::sync::Mutex<Vec<String>>);
    impl crate::dashboard::diagnostics::DiagnosticsSink for SetupDiagnostics {
        fn record(&self, record: &crate::dashboard::diagnostics::RefreshRecord) {
            self.0
                .lock()
                .unwrap()
                .push(crate::dashboard::diagnostics::format_line(record));
        }
    }

    #[tokio::test]
    async fn already_installed_winget_package_still_repairs_and_verifies_the_command() {
        for verification in [Ok(()), Err(VisibleProcessError::NotInstalled)] {
            let runner = Arc::new(InstallerResult {
                result: Err(VisibleProcessError::NonZero(0x8A15002B_u32 as i32)),
                verification,
                verified: Default::default(),
            });
            let service = SetupService::new(runner.clone());
            let result = service
                .install_on(ProviderId::Codex, HostPlatform::Windows)
                .await;
            assert_eq!(result.is_ok(), verification.is_ok());
            assert_eq!(
                *runner.verified.lock().unwrap(),
                vec![AllowedProgram::Codex]
            );
        }
    }

    #[tokio::test]
    async fn failed_installs_keep_the_stage_and_numeric_code_without_output_or_paths() {
        for failure in [
            VisibleProcessError::NonZero(0x8A150008_u32 as i32),
            VisibleProcessError::Spawn { os_code: Some(5) },
        ] {
            let runner = Arc::new(InstallerResult {
                result: Err(failure),
                verification: Ok(()),
                verified: Default::default(),
            });
            let diagnostics = Arc::new(SetupDiagnostics::default());
            let service = SetupService::new(runner.clone()).with_diagnostics(diagnostics.clone());
            assert_eq!(
                service
                    .install_on(ProviderId::Codex, HostPlatform::Windows)
                    .await,
                Err("provider setup process did not complete".to_owned())
            );
            assert!(runner.verified.lock().unwrap().is_empty());
            let logs = diagnostics.0.lock().unwrap();
            assert_eq!(logs.len(), 1);
            assert!(logs[0].contains(&format!("codex error: setup install failed: {failure:?}")));
        }
    }

    #[tokio::test]
    async fn verification_and_login_failures_are_identified_separately() {
        let diagnostics = Arc::new(SetupDiagnostics::default());
        let service = SetupService::new(Arc::new(InstallerResult {
            result: Ok(()),
            verification: Err(VisibleProcessError::Spawn { os_code: Some(193) }),
            verified: Default::default(),
        }))
        .with_diagnostics(diagnostics.clone());
        assert!(service
            .install_on(ProviderId::Claude, HostPlatform::Windows)
            .await
            .is_err());
        let service = SetupService::new(Arc::new(InstallerResult {
            result: Err(VisibleProcessError::NonZero(1)),
            verification: Ok(()),
            verified: Default::default(),
        }))
        .with_diagnostics(diagnostics.clone());
        assert!(service.login(ProviderId::Claude).await.is_err());
        let logs = diagnostics.0.lock().unwrap();
        assert!(logs[0].contains("setup command verification failed: Spawn { os_code: Some(193) }"));
        assert!(logs[1].contains("setup login failed: NonZero(1)"));
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
