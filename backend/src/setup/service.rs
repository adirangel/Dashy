use std::sync::Arc;

use crate::dashboard::{
    models::ProviderId,
    process::{AllowedProgram, VisibleProcessError, VisibleRunner},
};
use crate::setup::models::ProviderSetupDefinition;

pub struct SetupService {
    runner: Arc<dyn VisibleRunner>,
}

impl SetupService {
    pub fn new(runner: Arc<dyn VisibleRunner>) -> Self {
        Self { runner }
    }

    pub async fn install(&self, provider: ProviderId) -> Result<(), String> {
        let package = ProviderSetupDefinition::for_provider(provider).package_id;
        self.runner
            .run_visible(
                AllowedProgram::Winget,
                vec![
                    "install".into(),
                    "--id".into(),
                    package.into(),
                    "--exact".into(),
                    "--source".into(),
                    "winget".into(),
                    "--interactive".into(),
                    "--accept-source-agreements".into(),
                    "--accept-package-agreements".into(),
                ],
            )
            .await
            .map_err(sanitize_setup_error)
    }

    pub async fn login(&self, provider: ProviderId) -> Result<(), String> {
        let (program, args) = match provider {
            ProviderId::Claude => (AllowedProgram::Claude, vec!["auth", "login", "--claudeai"]),
            ProviderId::Codex => (AllowedProgram::Codex, vec!["login"]),
            ProviderId::GitHub => (AllowedProgram::Gh, vec!["auth", "login", "--web"]),
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
        service.install(ProviderId::Codex).await.unwrap();
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
    async fn login_uses_the_official_subscription_commands() {
        let runner = Arc::new(RecordingRunner::default());
        let service = SetupService::new(runner.clone());
        service.login(ProviderId::Claude).await.unwrap();
        service.login(ProviderId::Codex).await.unwrap();
        service.login(ProviderId::GitHub).await.unwrap();
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
            ]
        );
    }
}
