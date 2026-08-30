# Provider Setup Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the local settings, provider-scoped refresh, allowlisted visible process execution, and typed Tauri commands required by modular Claude, Codex, and GitHub setup.

**Architecture:** Extend the existing Rust-owned settings contract with onboarding and enabled-provider state, while migrating pre-feature settings to all providers enabled. Keep all install and login commands in a static Rust allowlist, run them in a visible Windows console, and expose only provider-enum Tauri commands. Refresh only enabled providers during normal dashboard use; setup commands may explicitly refresh one provider regardless of whether it is enabled.

**Tech Stack:** Rust 2021, Tauri 2, Tokio, Serde, `tauri-plugin-store`, existing provider adapters

**Spec:** `docs/WINDOWS_INSTALLER_DESIGN.md`

## Global Constraints

- The first release target is Windows x64, but new process code must return a recoverable unsupported-platform error on other operating systems.
- The three allowed WinGet IDs are exactly `Anthropic.ClaudeCode`, `OpenAI.Codex`, and `GitHub.cli`.
- Login commands are exactly `claude auth login --claudeai`, `codex login`, and `gh auth login --web`.
- No Tauri command accepts an executable, package ID, shell fragment, or arbitrary argument from the frontend.
- Provider credentials, command output, and OAuth material must never enter Dashy settings, logs, cache, or IPC responses.
- A disabled provider is not refreshed during startup, periodic refresh, or Refresh All.
- Existing settings written before this feature migrate to onboarding complete with all three providers enabled.
- A clean install defaults to onboarding incomplete with no providers enabled.

## File Structure

- `backend/src/dashboard/models.rs`: canonical provider enum and stable provider ordering.
- `backend/src/dashboard/service.rs`: cache freshness and provider-scoped refresh orchestration.
- `backend/src/dashboard/process.rs`: bounded hidden provider processes plus the new visible allowlisted process boundary.
- `backend/src/setup/mod.rs`: module exports and Tauri setup state.
- `backend/src/setup/models.rs`: serializable package metadata and setup status.
- `backend/src/setup/service.rs`: fixed install/login command mapping and visible runner orchestration.
- `backend/src/setup/commands.rs`: typed Tauri commands that join setup execution with provider status refresh.
- `backend/src/desktop/settings.rs`: persisted onboarding and enabled-provider preferences.
- `backend/src/desktop/commands.rs`: settings-change events, onboarding completion, and interaction authorization.
- `backend/src/desktop/controller.rs`: suppress the native edge surface when onboarding is incomplete or no provider is enabled.
- `backend/src/lib.rs`: service construction, command registration, and enabled-provider refresh routing.
- `frontend/src/window.ts`: TypeScript mirror of the expanded settings contract; UI work consumes it in the next plan.

---

### Task 1: Persist onboarding and enabled-provider state safely

**Files:**
- Modify: `backend/src/dashboard/models.rs`
- Modify: `backend/src/desktop/settings.rs`
- Modify: `frontend/src/window.ts`

**Interfaces:**
- Produces: `ProviderId::ALL: [ProviderId; 3]`
- Produces: `AppSettings { onboarding_completed: bool, enabled_providers: Vec<ProviderId>, ... }`
- Produces: `SettingsPatch { onboarding_completed: Option<bool>, enabled_providers: Option<Vec<ProviderId>>, ... }`
- Produces: TypeScript `AppSettings.onboardingCompleted` and `AppSettings.enabledProviders`

- [ ] **Step 1: Write failing Rust migration and validation tests**

Add these tests to `backend/src/desktop/settings.rs`:

```rust
#[test]
fn clean_install_requires_onboarding_and_enables_nothing() {
    let settings = AppSettings::default();
    assert!(!settings.onboarding_completed);
    assert!(settings.enabled_providers.is_empty());
}

#[test]
fn legacy_settings_migrate_to_all_providers_without_reonboarding() {
    let legacy = serde_json::json!({
        "placement": "right",
        "monitor": null,
        "locale": "en",
        "alwaysShowOverFullscreen": false
    });
    let migrated: AppSettings = serde_json::from_value(legacy).unwrap();
    assert!(migrated.onboarding_completed);
    assert_eq!(migrated.enabled_providers, ProviderId::ALL.to_vec());
}

#[test]
fn rejects_duplicate_enabled_providers() {
    let persistence = Arc::new(MemorySettingsPersistence::default());
    let service = SettingsService::load(persistence);
    let error = service.update(SettingsPatch {
        enabled_providers: Some(vec![ProviderId::Claude, ProviderId::Claude]),
        ..Default::default()
    });
    assert_eq!(error.unwrap_err(), "enabled providers must be unique");
}
```

- [ ] **Step 2: Run the focused tests and confirm they fail**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml desktop::settings::tests::clean_install_requires_onboarding_and_enables_nothing
cargo test --manifest-path backend/Cargo.toml desktop::settings::tests::legacy_settings_migrate_to_all_providers_without_reonboarding
cargo test --manifest-path backend/Cargo.toml desktop::settings::tests::rejects_duplicate_enabled_providers
```

Expected: compilation fails because the new fields and `ProviderId::ALL` do not exist.

- [ ] **Step 3: Add stable provider ordering and the persisted fields**

In `backend/src/dashboard/models.rs`, extend `ProviderId` and add the constant:

```rust
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderId {
    #[serde(rename = "github")]
    GitHub,
    Codex,
    Claude,
}

impl ProviderId {
    pub const ALL: [Self; 3] = [Self::Claude, Self::Codex, Self::GitHub];
}
```

In `backend/src/desktop/settings.rs`, import `ProviderId`, add legacy defaults, extend `AppSettings`, and extend `SettingsPatch`:

```rust
use crate::dashboard::models::ProviderId;

fn legacy_onboarding_completed() -> bool { true }
fn legacy_enabled_providers() -> Vec<ProviderId> { ProviderId::ALL.to_vec() }

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub placement: EdgePlacement,
    pub monitor: Option<MonitorPreference>,
    pub locale: LocaleCode,
    pub always_show_over_fullscreen: bool,
    #[serde(default = "legacy_onboarding_completed")]
    pub onboarding_completed: bool,
    #[serde(default = "legacy_enabled_providers")]
    pub enabled_providers: Vec<ProviderId>,
}
```

Set the two new fields to `false` and `Vec::new()` in `AppSettings::default`. Add these optional fields to `SettingsPatch` and copy them in `SettingsService::update`:

```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub onboarding_completed: Option<bool>,
#[serde(skip_serializing_if = "Option::is_none")]
pub enabled_providers: Option<Vec<ProviderId>>,
```

```rust
if let Some(onboarding_completed) = patch.onboarding_completed {
    next.onboarding_completed = onboarding_completed;
}
if let Some(enabled_providers) = patch.enabled_providers {
    next.enabled_providers = enabled_providers;
}
```

Extend the existing `defaults_are_english_right_primary_and_hide_over_fullscreen` expected struct with `onboarding_completed: false` and `enabled_providers: Vec::new()`.

Validate uniqueness without reordering the user's chosen rail order:

```rust
let unique = settings.enabled_providers.iter().copied().collect::<std::collections::HashSet<_>>();
if unique.len() != settings.enabled_providers.len() {
    return Err("enabled providers must be unique".into());
}
```

- [ ] **Step 4: Mirror the exact wire contract in TypeScript**

In `frontend/src/window.ts`, extend `AppSettings`:

```typescript
export type AppSettings = {
  placement: EdgePlacement;
  monitor: MonitorPreference | null;
  locale: SupportedLocale;
  alwaysShowOverFullscreen: boolean;
  onboardingCompleted: boolean;
  enabledProviders: ProviderId[];
};
```

- [ ] **Step 5: Run the settings and TypeScript checks**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml desktop::settings
npm --prefix frontend run build
```

Expected: Rust settings tests pass and TypeScript reports every old fixture that still needs the two new fields. Update those test fixtures to `onboardingCompleted: true` and `enabledProviders: ["claude", "codex", "github"]`, then rerun until both commands pass.

- [ ] **Step 6: Commit the settings contract**

```powershell
git add backend/src/dashboard/models.rs backend/src/desktop/settings.rs frontend/src/window.ts frontend/src
git commit -m "feat: persist modular provider preferences"
```

---

### Task 2: Refresh only the requested providers

**Files:**
- Modify: `backend/src/dashboard/service.rs`
- Modify: `backend/src/dashboard/commands.rs`
- Modify: `backend/src/lib.rs`

**Interfaces:**
- Consumes: `ProviderId::ALL`
- Consumes: `AppSettings.enabled_providers`
- Produces: `DashboardService::get_snapshot_for(force: bool, providers: &[ProviderId]) -> DashboardSnapshot`
- Preserves: `DashboardService::get_snapshot(force)` as an all-provider compatibility wrapper for existing tests and setup discovery

- [ ] **Step 1: Write failing provider-scope tests**

Add these tests to `backend/src/dashboard/service.rs` using the existing `ServiceFixture`:

```rust
#[tokio::test]
async fn scoped_refresh_never_fetches_disabled_providers() {
    let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
    fixture.service.get_snapshot_for(false, &[ProviderId::Claude]).await;
    assert_eq!(fixture.claude.calls(), 1);
    assert_eq!(fixture.codex.calls(), 0);
    assert_eq!(fixture.github.calls(), 0);
}

#[tokio::test]
async fn newly_enabled_provider_refreshes_even_when_another_provider_is_fresh() {
    let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
    fixture.service.get_snapshot_for(false, &[ProviderId::Claude]).await;
    fixture.clock.advance_minutes(1);
    fixture.service.get_snapshot_for(false, &[ProviderId::Claude, ProviderId::Codex]).await;
    assert_eq!(fixture.claude.calls(), 1);
    assert_eq!(fixture.codex.calls(), 1);
    assert_eq!(fixture.github.calls(), 0);
}

#[tokio::test]
async fn empty_provider_scope_returns_without_fetching() {
    let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
    let snapshot = fixture.service.get_snapshot_for(false, &[]).await;
    assert_eq!(snapshot.claude.status, ProviderStatus::Unavailable);
    assert_eq!(fixture.claude.calls() + fixture.codex.calls() + fixture.github.calls(), 0);
}
```

- [ ] **Step 2: Run the focused tests and confirm the missing API**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml dashboard::service::tests::scoped_refresh
cargo test --manifest-path backend/Cargo.toml dashboard::service::tests::newly_enabled_provider
cargo test --manifest-path backend/Cargo.toml dashboard::service::tests::empty_provider_scope
```

Expected: compilation fails because `get_snapshot_for` does not exist.

- [ ] **Step 3: Track per-provider attempt freshness and add scoped refresh**

Replace the global refresh lock/generation fields in `DashboardService` with:

```rust
provider_refreshed_at: RwLock<[Option<DateTime<Utc>>; 3]>,
```

Initialize it with `[None, None, None]`. Add a stable index helper:

```rust
fn provider_index(provider: ProviderId) -> usize {
    match provider {
        ProviderId::Claude => 0,
        ProviderId::Codex => 1,
        ProviderId::GitHub => 2,
    }
}
```

Add the scoped API and keep the old API as a wrapper:

```rust
pub async fn get_snapshot(&self, force: bool) -> DashboardSnapshot {
    self.get_snapshot_for(force, &ProviderId::ALL).await
}

pub async fn get_snapshot_for(
    &self,
    force: bool,
    providers: &[ProviderId],
) -> DashboardSnapshot {
    if providers.is_empty() {
        return self.cached_snapshot_or_empty().await;
    }
    let now = self.clock.now();
    let freshness = self.provider_refreshed_at.read().await;
    let needs_refresh = |provider| {
        force || freshness[provider_index(provider)]
            .is_none_or(|at| now.signed_duration_since(at) >= CACHE_TTL)
    };
    let github = providers.contains(&ProviderId::GitHub) && needs_refresh(ProviderId::GitHub);
    let codex = providers.contains(&ProviderId::Codex) && needs_refresh(ProviderId::Codex);
    let claude = providers.contains(&ProviderId::Claude) && needs_refresh(ProviderId::Claude);
    drop(freshness);

    let github_generation = self.github_generation.load(Ordering::Acquire);
    let codex_generation = self.codex_generation.load(Ordering::Acquire);
    let claude_generation = self.claude_generation.load(Ordering::Acquire);
    tokio::join!(
        async { if github { self.refresh_github(github_generation).await } },
        async { if codex { self.refresh_codex(codex_generation).await } },
        async { if claude { self.refresh_claude(claude_generation).await } },
    );
    self.cached_snapshot_or_empty().await
}
```

After each provider fetch attempt, record `refreshed_at` at its stable index. Replace the panicking cache accessor with:

```rust
async fn cached_snapshot_or_empty(&self) -> DashboardSnapshot {
    self.cache.read().await.clone().unwrap_or_else(|| empty_snapshot(self.clock.now()))
}
```

Use this helper from both `get_snapshot_for` and `refresh_provider`.

- [ ] **Step 4: Route normal dashboard commands through enabled providers**

In `backend/src/dashboard/commands.rs`, add `DesktopState` to `get_dashboard_snapshot` and reject direct refresh of a disabled provider:

```rust
pub async fn get_dashboard_snapshot(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    desktop: tauri::State<'_, crate::desktop::DesktopState>,
    force: Option<bool>,
) -> Result<DashboardSnapshot, String> {
    let enabled = desktop.settings.current()?.enabled_providers;
    let snapshot = state.dashboard.get_snapshot_for(force.unwrap_or(false), &enabled).await;
    if force.unwrap_or(false) { emit_dashboard_cache_changed(&app)?; }
    Ok(snapshot)
}
```

In `refresh_dashboard_provider`, load settings and return `Err("provider is disabled")` unless the provider is present.

In `backend/src/lib.rs`, change startup prefetch and tray `refresh_all` to call `get_snapshot_for` with the current enabled-provider vector. Clone the vector before entering the async task.

- [ ] **Step 5: Run dashboard concurrency and scope tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml dashboard::service
cargo test --manifest-path backend/Cargo.toml dashboard::commands
```

Expected: all existing cache/concurrency tests and the three new provider-scope tests pass.

- [ ] **Step 6: Commit scoped refresh**

```powershell
git add backend/src/dashboard/service.rs backend/src/dashboard/commands.rs backend/src/lib.rs
git commit -m "feat: refresh only enabled providers"
```

---

### Task 3: Add the static setup command allowlist and visible runner

**Files:**
- Modify: `backend/src/dashboard/process.rs`
- Create: `backend/src/setup/mod.rs`
- Create: `backend/src/setup/models.rs`
- Create: `backend/src/setup/service.rs`
- Modify: `backend/src/lib.rs`

**Interfaces:**
- Produces: `VisibleRunner::run_visible(program: AllowedProgram, args: Vec<String>) -> Result<(), VisibleProcessError>`
- Produces: `ProviderSetupDefinition::for_provider(provider)`
- Produces: `SetupService::install(provider)` and `SetupService::login(provider)`
- Produces: serializable `ProviderSetupState { definition, status }`

- [ ] **Step 1: Write failing allowlist tests**

Create `backend/src/setup/service.rs` with a test-only recording runner and these tests first:

```rust
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
    assert_eq!(runner.calls(), vec![(AllowedProgram::Winget, vec![
        "install", "--id", "OpenAI.Codex", "--exact", "--source", "winget",
        "--interactive", "--accept-source-agreements", "--accept-package-agreements",
    ].into_iter().map(str::to_owned).collect())]);
}

#[tokio::test]
async fn login_uses_the_official_subscription_commands() {
    let runner = Arc::new(RecordingRunner::default());
    let service = SetupService::new(runner.clone());
    service.login(ProviderId::Claude).await.unwrap();
    service.login(ProviderId::Codex).await.unwrap();
    service.login(ProviderId::GitHub).await.unwrap();
    assert_eq!(runner.calls(), vec![
        (AllowedProgram::Claude, vec!["auth".into(), "login".into(), "--claudeai".into()]),
        (AllowedProgram::Codex, vec!["login".into()]),
        (AllowedProgram::Gh, vec!["auth".into(), "login".into(), "--web".into()]),
    ]);
}
```

- [ ] **Step 2: Run the setup tests and confirm they fail**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml setup::service
```

Expected: compilation fails because the setup module, `VisibleRunner`, `VisibleProcessError`, and `AllowedProgram::Winget` do not exist.

- [ ] **Step 3: Add visible execution to the existing safe process boundary**

In `backend/src/dashboard/process.rs`, add `Winget` to `AllowedProgram` and map it to `winget`. Keep visible-launch errors separate from captured provider errors:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibleProcessError {
    NotInstalled,
    UnsupportedPlatform,
    Failed,
}

#[async_trait]
pub trait VisibleRunner: Send + Sync {
    async fn run_visible(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
    ) -> Result<(), VisibleProcessError>;
}
```

Implement it for `SystemProcessRunner`. On Windows, reuse `program_launch`, inherit standard streams, set `CREATE_NEW_CONSOLE` (`0x00000010`), wait for exit, and convert a non-zero exit to `VisibleProcessError::Failed`. On non-Windows, return `VisibleProcessError::UnsupportedPlatform` without spawning anything:

```rust
#[async_trait]
impl VisibleRunner for SystemProcessRunner {
    async fn run_visible(&self, program: AllowedProgram, args: Vec<String>) -> Result<(), VisibleProcessError> {
        #[cfg(windows)]
        {
            const CREATE_NEW_CONSOLE: u32 = 0x00000010;
            let launch = program_launch(program);
            let status = Command::new(&launch.executable)
                .args(&launch.prefix_args)
                .args(args)
                .stdin(std::process::Stdio::inherit())
                .stdout(std::process::Stdio::inherit())
                .stderr(std::process::Stdio::inherit())
                .creation_flags(CREATE_NEW_CONSOLE)
                .status().await.map_err(|error| {
                    if error.kind() == std::io::ErrorKind::NotFound {
                        VisibleProcessError::NotInstalled
                    } else {
                        VisibleProcessError::Failed
                    }
                })?;
            return status.success().then_some(()).ok_or(VisibleProcessError::Failed);
        }
        #[cfg(not(windows))]
        { let _ = (program, args); Err(VisibleProcessError::UnsupportedPlatform) }
    }
}
```

- [ ] **Step 4: Implement package metadata and setup service**

Create `backend/src/setup/models.rs` with the exact metadata contract:

```rust
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
                "Anthropic", "Anthropic.ClaudeCode",
                "https://code.claude.com/docs/en/setup", "claude auth login --claudeai",
            ),
            ProviderId::Codex => (
                "OpenAI", "OpenAI.Codex",
                "https://learn.chatgpt.com/docs/codex/cli", "codex login",
            ),
            ProviderId::GitHub => (
                "GitHub", "GitHub.cli",
                "https://cli.github.com/", "gh auth login --web",
            ),
        };
        let install_command = format!(
            "winget install --id {package_id} --exact --source winget --interactive --accept-source-agreements --accept-package-agreements"
        );
        Self { provider, publisher, package_id, install_command, install_url, login_command }
    }
}
```

Create `ProviderSetupState`:

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetupState {
    pub definition: ProviderSetupDefinition,
    pub status: ProviderStatus,
}
```

Create `SetupService` in `backend/src/setup/service.rs`. Its arguments come only from the provider enum and definition:

```rust
pub struct SetupService {
    runner: Arc<dyn VisibleRunner>,
}

impl SetupService {
    pub fn new(runner: Arc<dyn VisibleRunner>) -> Self { Self { runner } }

    pub async fn install(&self, provider: ProviderId) -> Result<(), String> {
        let package = ProviderSetupDefinition::for_provider(provider).package_id;
        self.runner.run_visible(AllowedProgram::Winget, vec![
            "install".into(), "--id".into(), package.into(), "--exact".into(),
            "--source".into(), "winget".into(), "--interactive".into(),
            "--accept-source-agreements".into(), "--accept-package-agreements".into(),
        ]).await.map_err(sanitize_setup_error)
    }

    pub async fn login(&self, provider: ProviderId) -> Result<(), String> {
        let (program, args) = match provider {
            ProviderId::Claude => (AllowedProgram::Claude, vec!["auth", "login", "--claudeai"]),
            ProviderId::Codex => (AllowedProgram::Codex, vec!["login"]),
            ProviderId::GitHub => (AllowedProgram::Gh, vec!["auth", "login", "--web"]),
        };
        self.runner.run_visible(program, args.into_iter().map(str::to_owned).collect())
            .await.map_err(sanitize_setup_error)
    }
}

fn sanitize_setup_error(error: VisibleProcessError) -> String {
    match error {
        VisibleProcessError::UnsupportedPlatform => "provider setup is not supported on this platform",
        VisibleProcessError::NotInstalled => "provider tool is not installed",
        VisibleProcessError::Failed => "provider setup process did not complete",
    }.to_string()
}
```

Create `backend/src/setup/mod.rs`:

```rust
pub mod models;
pub mod service;
```

- [ ] **Step 5: Run process and setup tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml dashboard::process
cargo test --manifest-path backend/Cargo.toml setup::service
```

Expected: process resolution tests and exact allowlist tests pass without launching a real installer.

- [ ] **Step 6: Commit the allowlisted runner**

```powershell
git add backend/src/dashboard/process.rs backend/src/setup backend/src/lib.rs
git commit -m "feat: add allowlisted provider setup runner"
```

---

### Task 4: Expose typed setup commands and suppress an unconfigured rail

**Files:**
- Create: `backend/src/setup/commands.rs`
- Modify: `backend/src/setup/mod.rs`
- Modify: `backend/src/desktop/commands.rs`
- Modify: `backend/src/desktop/controller.rs`
- Modify: `backend/src/desktop/mod.rs`
- Modify: `backend/src/lib.rs`
- Test: `backend/tests/desktop_shell.rs`

**Interfaces:**
- Produces Tauri commands: `get_provider_setup_states`, `install_provider`, `login_provider`, `complete_onboarding`
- Produces event: `dashy://settings-changed` carrying the complete `AppSettings`
- Consumes: `SetupService`, `DashboardService`, and `SettingsService`

- [ ] **Step 1: Write failing command-contract and controller tests**

Add strict deserialization tests in `backend/src/setup/commands.rs` showing that setup requests accept only a provider enum and reject executable/package fields:

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProviderSetupRequest { provider: ProviderId }

#[test]
fn setup_request_rejects_command_injection_fields() {
    for value in [
        serde_json::json!({"provider":"codex","executable":"powershell"}),
        serde_json::json!({"provider":"github","packageId":"Other.Package"}),
        serde_json::json!({"provider":"unknown"}),
    ] {
        assert!(serde_json::from_value::<ProviderSetupRequest>(value).is_err());
    }
}
```

Add this controller test to `backend/tests/desktop_shell.rs`, whose `Fixture`, `FakeSettings`, and `WindowAction` already own the native boundary:

```rust
#[test]
fn incomplete_onboarding_keeps_the_native_surface_hidden() {
    let fixture = Fixture::new();
    fixture.settings.set(AppSettings::default());
    fixture.controller.show_explicit();
    assert!(fixture.controller.step(Duration::from_millis(1)).is_empty());
    assert_eq!(fixture.controller.state(), EdgeUiState::Hidden);
    assert!(fixture.window.actions().iter().all(|action| {
        !matches!(action, WindowAction::Apply(layout) if layout.visible)
    }));
}
```

Add one configured legacy-equivalent helper so existing edge tests keep their original meaning:

```rust
fn configured_settings() -> AppSettings {
    AppSettings {
        onboarding_completed: true,
        enabled_providers: ProviderId::ALL.to_vec(),
        ..AppSettings::default()
    }
}
```

Use `configured_settings()` in `Fixture::new`, and replace every pre-existing `..AppSettings::default()` settings update in `backend/tests/desktop_shell.rs` with `..configured_settings()`. Keep `AppSettings::default()` only in the new incomplete-onboarding test.

- [ ] **Step 2: Run focused tests and confirm they fail**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml setup::commands
cargo test --manifest-path backend/Cargo.toml incomplete_onboarding_keeps_the_native_surface_hidden
```

Expected: the command module and controller gate do not exist.

- [ ] **Step 3: Implement setup state discovery and action commands**

Create `SetupState` around `Arc<SetupService>` and implement these commands in `backend/src/setup/commands.rs`:

```rust
#[tauri::command]
pub async fn get_provider_setup_states(
    dashboard: State<'_, AppState>,
) -> Result<Vec<ProviderSetupState>, String>;

#[tauri::command]
pub async fn install_provider(
    app: AppHandle,
    dashboard: State<'_, AppState>,
    setup: State<'_, SetupState>,
    request: ProviderSetupRequest,
) -> Result<ProviderSetupState, String>;

#[tauri::command]
pub async fn login_provider(
    app: AppHandle,
    dashboard: State<'_, AppState>,
    setup: State<'_, SetupState>,
    request: ProviderSetupRequest,
) -> Result<ProviderSetupState, String>;
```

`get_provider_setup_states` calls `dashboard.dashboard.get_snapshot(true)` once and maps `ProviderId::ALL` to definitions plus statuses. Each action runs the allowlisted service, refreshes only the requested provider, emits `dashy://dashboard-cache-changed`, and returns its updated status. Never return captured output or executable paths.

- [ ] **Step 4: Implement atomic onboarding completion and settings events**

In `backend/src/desktop/commands.rs`, add:

```rust
const SETTINGS_CHANGED_EVENT: &str = "dashy://settings-changed";

fn emit_settings_changed(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    app.emit_to("main", SETTINGS_CHANGED_EVENT, settings)
        .map_err(|error| format!("failed to publish settings: {error}"))
}

#[tauri::command]
pub async fn complete_onboarding(
    app: AppHandle,
    state: State<'_, DesktopState>,
    enabled_providers: Vec<ProviderId>,
) -> Result<AppSettings, String> {
    let settings = state.settings.update(SettingsPatch {
        onboarding_completed: Some(true),
        enabled_providers: Some(enabled_providers),
        ..Default::default()
    })?;
    emit_settings_changed(&app, &settings)?;
    state.refresh_tray(&app, &settings)?;
    Ok(settings)
}
```

Call `emit_settings_changed` from `update_settings` after persistence succeeds. Do not emit on a failed save.

Before updating settings, retain the previous enabled-provider vector. If it changes, queue `EdgeInteraction::Dismiss` after persistence so a pinned or open card cannot keep showing a provider that was just disabled. Apply the same dismissal after `complete_onboarding`.

- [ ] **Step 5: Gate controller events when no configured provider exists**

In `DesktopController::step`, compute:

```rust
let surface_enabled = settings.onboarding_completed && !settings.enabled_providers.is_empty();
```

When `surface_enabled` is false, pass `None` as the cursor, discard queued selection/show events, and drive one `EdgeInteraction::Dismiss` if the machine is not hidden. Extend `plan_operations` with a `surface_enabled: bool` argument so the gate is tested independently and applies equally to tray Show and edge hover. The hidden window layout must remain non-visible.

In `set_notch_interaction`, reject `SelectProvider` and `TogglePin` when their provider is not in `state.settings.current()?.enabled_providers`.

- [ ] **Step 6: Wire state and commands into Tauri**

In `backend/src/lib.rs`:

```rust
pub mod setup;
```

Add `pub mod commands;` to `backend/src/setup/mod.rs`. Manage `SetupState::new(Arc::new(SetupService::new(Arc::new(SystemProcessRunner))))` and register all four new commands in `tauri::generate_handler!`. Add `ProviderSetupState` command imports explicitly so the command surface remains reviewable.

- [ ] **Step 7: Run all backend tests**

Run:

```powershell
cargo fmt --manifest-path backend/Cargo.toml --check
cargo test --manifest-path backend/Cargo.toml
cargo clippy --manifest-path backend/Cargo.toml --all-targets -- -D warnings
```

Expected: formatting, every unit/integration test, and Clippy pass with zero warnings.

- [ ] **Step 8: Commit the setup IPC boundary**

```powershell
git add backend/src/setup backend/src/desktop backend/src/lib.rs
git commit -m "feat: expose secure provider setup commands"
```

---

### Task 5: Verify the backend feature as one coherent boundary

**Files:**
- Verify: `backend/src`
- Verify: `backend/tests/desktop_shell.rs`

**Interfaces:**
- Verifies all interfaces produced by Tasks 1–4; produces no new runtime API.

- [ ] **Step 1: Run the full repository verification**

Run:

```powershell
npm --prefix frontend run test
npm --prefix frontend run build
cargo fmt --manifest-path backend/Cargo.toml --check
cargo test --manifest-path backend/Cargo.toml
cargo clippy --manifest-path backend/Cargo.toml --all-targets -- -D warnings
```

Expected: all frontend tests, TypeScript/Vite build, Rust formatting, Rust tests, and Clippy pass.

- [ ] **Step 2: Confirm the worktree contains only the intended feature commits**

```powershell
git status --short
git log --oneline -5
```

Expected: a clean worktree and the four focused commits from Tasks 1–4 at the top of history.
