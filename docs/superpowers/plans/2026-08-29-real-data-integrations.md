# Real Data Integrations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (\`- [ ]\`) syntax for tracking.

**Goal:** Replace Dashy's mocked GitHub, Claude, and Codex metrics with real data from the authenticated local command-line clients.

**Architecture:** The Rust/Tauri backend owns three isolated provider adapters and a five-minute in-memory cache. GitHub uses \`gh api graphql\`, Codex uses the structured \`codex app-server --stdio\` JSON-RPC method \`account/rateLimits/read\`, and Claude checks \`claude auth status --json\` before reading the documented interactive \`/usage\` view through a hidden PTY. React receives one typed snapshot and only renders normalized data and provider states.

**Tech Stack:** Rust 2021, Tauri 2, Tokio, Serde, Chrono, async-trait, portable-pty, React, strict TypeScript, Vite, Vitest, Testing Library.

**Spec:** \`docs/superpowers/specs/2026-08-29-real-data-integrations-design.md\`

## Global Constraints

- Keep credentials, raw provider output, and process execution inside the Rust backend.
- Execute only the allowlisted programs \`gh\`, \`codex\`, and \`claude\`; never interpolate input into a shell command.
- Do not call private provider endpoints, scrape provider web pages, read browser cookies, or read provider credential files.
- Percentages always mean remaining allowance in the inclusive \`0..100\` range.
- When several general plan windows exist, display the most restrictive window; exclude model-specific promotional limits.
- Refresh on startup and every five minutes, with independent provider timeouts and last-good stale fallback.
- Preserve the existing visual layout, RTL behavior, 320-pixel minimum width, task behavior, and transparent Tauri window.
- Do not log raw CLI output or account details.
- Do not commit or push. The user will review and commit the completed working tree.

## Planned File Structure

### Backend

- \`backend/src/lib.rs\` — construct Tauri state, register commands, and expose \`run()\`.
- \`backend/src/main.rs\` — minimal binary entry point.
- \`backend/src/dashboard/models.rs\` — serialized contract and provider errors.
- \`backend/src/dashboard/process.rs\` — allowlisted capture, JSON-RPC, and hidden PTY runners.
- \`backend/src/dashboard/providers/github.rs\` — GitHub GraphQL adapter and streak calculation.
- \`backend/src/dashboard/providers/codex.rs\` — Codex app-server adapter.
- \`backend/src/dashboard/providers/claude.rs\` — Claude authentication and \`/usage\` adapter.
- \`backend/src/dashboard/service.rs\` — concurrency, cache, single-flight, and stale fallback.
- \`backend/src/dashboard/commands.rs\` — typed Tauri command.
- \`backend/tests/fixtures/*\` — sanitized provider fixtures.

### Frontend and setup

- \`frontend/src/dashboard.ts\` — TypeScript contract and Tauri client.
- \`frontend/src/useDashboardSnapshot.ts\` — startup and five-minute refresh hook.
- \`frontend/src/DashboardMetrics.tsx\` — real GitHub and usage-card presentation.
- \`frontend/src/App.tsx\` — compose live metrics with unchanged task UI.
- \`frontend/src/styles.css\` — provider-state styles only.
- \`frontend/src/*.test.tsx\` — rendering and timer tests.
- \`install.ps1\` — prerequisite installer/checker.
- \`README.md\` — setup, login, privacy, and verification instructions.

---

### Task 1: Establish the typed backend contract

**Files:**
- Modify: \`backend/Cargo.toml\`
- Create: \`backend/src/lib.rs\`
- Modify: \`backend/src/main.rs\`
- Create: \`backend/src/dashboard/mod.rs\`
- Create: \`backend/src/dashboard/models.rs\`

**Interfaces:**
- Produces: \`ProviderStatus\`, \`ProviderErrorKind\`, \`ProviderError\`, \`ContributionDay\`, \`GitHubData\`, \`UsageData\`, \`GitHubSnapshot\`, \`UsageSnapshot\`, and \`DashboardSnapshot\`.
- Produces: \`pub fn run()\`.

- [ ] **Step 1: Add backend dependencies**

~~~toml
async-trait = "0.1"
chrono = { version = "0.4", features = ["serde"] }
portable-pty = "0.9"
strip-ansi-escapes = "0.2"
thiserror = "2"
tokio = { version = "1", features = ["io-util", "macros", "process", "rt-multi-thread", "sync", "time"] }
~~~

- [ ] **Step 2: Write failing model tests**

~~~rust
#[test]
fn serializes_remaining_allowance_in_camel_case() {
    let snapshot = UsageSnapshot::connected(
        UsageData {
            remaining_percent: 59,
            resets_at: Some(Utc.with_ymd_and_hms(2026, 9, 3, 11, 0, 0).unwrap()),
            reset_label: None,
        },
        Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap(),
    );
    let json = serde_json::to_value(snapshot).unwrap();
    assert_eq!(json["status"], "connected");
    assert_eq!(json["remainingPercent"], 59);
    assert!(json.get("usedPercent").is_none());
}

#[test]
fn unavailable_usage_has_no_numeric_value() {
    let snapshot = UsageSnapshot::failed(
        ProviderStatus::Unavailable,
        ProviderErrorKind::UnsupportedOutput,
    );
    assert!(serde_json::to_value(snapshot).unwrap()["remainingPercent"].is_null());
}
~~~

- [ ] **Step 3: Run the focused test and confirm missing-type failures**

~~~powershell
Set-Location D:\dev\Dashy\backend
cargo test dashboard::models::tests --no-fail-fast
~~~

Expected: compilation fails because the snapshot types are not defined.

- [ ] **Step 4: Implement the contract**

Use these exact base types and camelCase serialization:

~~~rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderStatus {
    Connected,
    Stale,
    NotInstalled,
    NotAuthenticated,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderErrorKind {
    MissingExecutable,
    Authentication,
    Network,
    Timeout,
    UnsupportedOutput,
    Process,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProviderError {
    #[error("required executable is not installed")]
    NotInstalled,
    #[error("provider authentication is unavailable")]
    NotAuthenticated,
    #[error("provider request timed out")]
    Timeout,
    #[error("provider output is unsupported")]
    UnsupportedOutput,
    #[error("provider process failed")]
    Process,
    #[error("provider network request failed")]
    Network,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContributionDay {
    pub date: NaiveDate,
    pub count: u32,
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubData {
    pub account_login: String,
    pub contribution_days: Vec<ContributionDay>,
    pub current_streak_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageData {
    pub remaining_percent: u8,
    pub resets_at: Option<DateTime<Utc>>,
    pub reset_label: Option<String>,
}
~~~

Define specific GitHub and usage snapshots with optional fields, \`status\`, \`last_successful_refresh\`, and \`error_kind\`. Constructors \`connected\`, \`failed\`, and \`stale_from\` must prevent numeric data from being paired with a non-data status.

- [ ] **Step 5: Split the binary from the library**

~~~rust
// backend/src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dashy::run();
}
~~~

~~~rust
// backend/src/lib.rs
pub mod dashboard;

pub fn run() {
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("failed to run Dashy");
}
~~~

- [ ] **Step 6: Verify Task 1**

~~~powershell
Set-Location D:\dev\Dashy\backend
cargo fmt --check
cargo test dashboard::models::tests
cargo check
git diff -- Cargo.toml src
~~~

Expected: all checks pass. Leave the changes uncommitted.

---

### Task 2: Build the allowlisted process boundary

**Files:**
- Create: \`backend/src/dashboard/process.rs\`
- Modify: \`backend/src/dashboard/mod.rs\`

**Interfaces:**
- Produces: \`AllowedProgram::{Gh, Codex, Claude}\`.
- Produces: \`CaptureRunner::capture\`, \`JsonRpcRunner::request\`, and \`InteractiveRunner::run_command\`.
- Produces: \`SystemProcessRunner\`.

- [ ] **Step 1: Write failing boundary tests**

~~~rust
#[test]
fn executable_names_are_fixed() {
    assert_eq!(AllowedProgram::Gh.executable(), "gh");
    assert_eq!(AllowedProgram::Codex.executable(), "codex");
    assert_eq!(AllowedProgram::Claude.executable(), "claude");
}

#[test]
fn oversized_output_is_rejected() {
    let bytes = vec![b'x'; MAX_OUTPUT_BYTES + 1];
    assert_eq!(bounded_text(bytes).unwrap_err(), ProcessError::OutputLimit);
}

#[test]
fn not_found_maps_to_not_installed() {
    let error = std::io::Error::new(std::io::ErrorKind::NotFound, "missing");
    assert_eq!(map_spawn_error(error), ProcessError::NotInstalled);
}
~~~

- [ ] **Step 2: Confirm the tests fail**

~~~powershell
Set-Location D:\dev\Dashy\backend
cargo test dashboard::process::tests --no-fail-fast
~~~

- [ ] **Step 3: Define the exact runner interfaces**

~~~rust
pub const MAX_OUTPUT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllowedProgram { Gh, Codex, Claude }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessError { NotInstalled, Timeout, NonZero(i32), OutputLimit, Io }

#[async_trait]
pub trait CaptureRunner: Send + Sync {
    async fn capture(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
        timeout: Duration,
    ) -> Result<CapturedOutput, ProcessError>;
}

#[async_trait]
pub trait JsonRpcRunner: Send + Sync {
    async fn request(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
        requests: Vec<serde_json::Value>,
        response_id: u64,
        timeout: Duration,
    ) -> Result<serde_json::Value, ProcessError>;
}

#[async_trait]
pub trait InteractiveRunner: Send + Sync {
    async fn run_command(
        &self,
        program: AllowedProgram,
        args: Vec<String>,
        input: String,
        completion_markers: Vec<String>,
        timeout: Duration,
    ) -> Result<String, ProcessError>;
}
~~~

- [ ] **Step 4: Implement capture and JSON-RPC execution**

Use \`tokio::process::Command\`, piped streams, newline-delimited JSON, and \`tokio::time::timeout\`. On Windows apply \`CREATE_NO_WINDOW = 0x08000000\`. The JSON-RPC runner writes each request with a newline, ignores notifications, returns only the \`result\` whose \`id\` matches \`response_id\`, then kills and waits for the child.

Only \`AllowedProgram::executable()\` may supply the program name.

- [ ] **Step 5: Implement hidden PTY execution**

Use \`portable_pty::native_pty_system()\` inside \`tokio::task::spawn_blocking\`, an 80x30 terminal, bounded output, ANSI stripping, completion-marker detection, and guaranteed child termination on success or timeout.

- [ ] **Step 6: Verify Task 2**

~~~powershell
Set-Location D:\dev\Dashy\backend
cargo fmt --check
cargo test dashboard::process::tests
cargo check
git diff -- src/dashboard/process.rs src/dashboard/mod.rs Cargo.lock
~~~

Expected: all checks pass and no test child process remains. Leave changes uncommitted.

---

### Task 3: Implement GitHub contributions and streaks

**Files:**
- Create: \`backend/src/dashboard/providers/mod.rs\`
- Create: \`backend/src/dashboard/providers/github.rs\`
- Create: \`backend/tests/fixtures/github-contributions.json\`
- Modify: \`backend/src/dashboard/mod.rs\`

**Interfaces:**
- Produces: \`DataProvider<T>::fetch() -> Result<T, ProviderError>\`.
- Produces: \`GitHubProvider<R: CaptureRunner>\` implementing \`DataProvider<GitHubData>\`.

- [ ] **Step 1: Add a sanitized GraphQL fixture**

~~~json
{
  "data": {
    "viewer": {
      "login": "fixture-user",
      "contributionsCollection": {
        "contributionCalendar": {
          "weeks": [{
            "contributionDays": [
              {"date":"2026-08-26","contributionCount":1,"contributionLevel":"FIRST_QUARTILE"},
              {"date":"2026-08-27","contributionCount":2,"contributionLevel":"SECOND_QUARTILE"},
              {"date":"2026-08-28","contributionCount":3,"contributionLevel":"THIRD_QUARTILE"},
              {"date":"2026-08-29","contributionCount":0,"contributionLevel":"NONE"}
            ]
          }]
        }
      }
    }
  }
}
~~~

- [ ] **Step 2: Write failing parser and streak tests**

~~~rust
#[test]
fn keeps_yesterdays_streak_active_until_today_ends() {
    let days = fixture_days(&[
        ("2026-08-27", 1),
        ("2026-08-28", 2),
        ("2026-08-29", 0),
    ]);
    assert_eq!(calculate_streak(&days, date(2026, 8, 29)), 2);
}

#[test]
fn crosses_year_boundary() {
    let days = fixture_days(&[("2025-12-31", 1), ("2026-01-01", 1)]);
    assert_eq!(calculate_streak(&days, date(2026, 1, 1)), 2);
}

#[test]
fn parses_fixture_levels_and_login() {
    let data = parse_response(include_str!("../../../tests/fixtures/github-contributions.json"), date(2026, 8, 29)).unwrap();
    assert_eq!(data.account_login, "fixture-user");
    assert_eq!(data.current_streak_days, 3);
    assert_eq!(data.contribution_days.iter().map(|day| day.level).collect::<Vec<_>>(), vec![1, 2, 3, 0]);
}
~~~

- [ ] **Step 3: Confirm the tests fail**

~~~powershell
cargo test dashboard::providers::github::tests --no-fail-fast
~~~

- [ ] **Step 4: Implement the provider trait and parser**

~~~rust
#[async_trait]
pub trait DataProvider<T>: Send + Sync {
    async fn fetch(&self) -> Result<T, ProviderError>;
}
~~~

Use this query:

~~~graphql
query DashyContributions {
  viewer {
    login
    contributionsCollection {
      contributionCalendar {
        weeks {
          contributionDays { date contributionCount contributionLevel }
        }
      }
    }
  }
}
~~~

Map \`NONE=0\`, \`FIRST_QUARTILE=1\`, \`SECOND_QUARTILE=2\`, \`THIRD_QUARTILE=3\`, and \`FOURTH_QUARTILE=4\`. Reject unknown levels.

- [ ] **Step 5: Implement the live call**

Call \`gh api graphql -f query=<query>\` through \`CaptureRunner\` with a 20-second timeout. Map missing executable, authentication, timeout, and network/process failures separately. Use \`viewer.login\`; do not hard-code \`adirangel\`.

Add an ignored live test named \`live_github_contributions\` that constructs the real provider, asserts a non-empty contribution calendar, and asserts the authenticated login is \`adirangel\`. The test must not print the returned calendar or account metadata.

- [ ] **Step 6: Verify Task 3**

~~~powershell
Set-Location D:\dev\Dashy\backend
cargo test dashboard::providers::github::tests
cargo check
Set-Location D:\dev\Dashy
gh api user --jq .login
git diff -- backend/src/dashboard/providers backend/tests/fixtures/github-contributions.json
~~~

Expected: tests pass and the live login is \`adirangel\`. Leave changes uncommitted.

---

### Task 4: Implement structured Codex allowance retrieval

**Files:**
- Create: \`backend/src/dashboard/providers/codex.rs\`
- Create: \`backend/tests/fixtures/codex-rate-limits.json\`
- Modify: \`backend/src/dashboard/providers/mod.rs\`

**Interfaces:**
- Produces: \`CodexProvider<R: JsonRpcRunner>\` implementing \`DataProvider<UsageData>\`.

- [ ] **Step 1: Add a sanitized JSON-RPC result fixture**

~~~json
{
  "rateLimits": {
    "limitId": "codex",
    "primary": {"usedPercent":10,"windowDurationMins":10080,"resetsAt":1788532560},
    "secondary": null
  },
  "rateLimitsByLimitId": {
    "codex": {
      "limitId": "codex",
      "limitName": null,
      "primary": {"usedPercent":10,"windowDurationMins":10080,"resetsAt":1788532560},
      "secondary": null
    },
    "codex_preview": {
      "limitId": "codex_preview",
      "limitName": "Preview Model",
      "primary": {"usedPercent":80,"windowDurationMins":300,"resetsAt":1788014876},
      "secondary": null
    }
  }
}
~~~

- [ ] **Step 2: Write failing parser tests**

~~~rust
#[test]
fn reads_general_bucket_and_ignores_preview_limits() {
    let data = parse_rate_limits(include_str!("../../../tests/fixtures/codex-rate-limits.json")).unwrap();
    assert_eq!(data.remaining_percent, 90);
    assert_eq!(data.resets_at.unwrap().timestamp(), 1788532560);
}

#[test]
fn chooses_the_most_restrictive_general_window() {
    let value = fixture_with_general_windows(20, 45);
    assert_eq!(parse_value(value).unwrap().remaining_percent, 55);
}

#[test]
fn rejects_missing_general_windows() {
    assert_eq!(parse_value(serde_json::json!({})), Err(ProviderError::UnsupportedOutput));
}
~~~

- [ ] **Step 3: Confirm the tests fail**

~~~powershell
cargo test dashboard::providers::codex::tests --no-fail-fast
~~~

- [ ] **Step 4: Implement the exact app-server request**

Start \`codex app-server --stdio\` and send:

~~~json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"dashy","version":"0.1.0"},"capabilities":{"experimentalApi":true}}}
{"jsonrpc":"2.0","id":2,"method":"account/rateLimits/read"}
~~~

Request response id \`2\` with a 15-second timeout. Prefer \`rateLimitsByLimitId.codex\`; fall back to \`rateLimits\`. Consider only that bucket's \`primary\` and \`secondary\` windows. Convert each \`usedPercent\` to remaining, select the lowest remaining value, and convert Unix \`resetsAt\` seconds to UTC.

- [ ] **Step 5: Map failures without parsing the TUI**

Map missing \`codex\` to \`NotInstalled\`, an authentication JSON-RPC error to \`NotAuthenticated\`, timeout to \`Timeout\`, and schema drift to \`UnsupportedOutput\`. Never fall back to \`/status\` screen parsing.

- [ ] **Step 6: Verify Task 4**

~~~powershell
Set-Location D:\dev\Dashy\backend
cargo test dashboard::providers::codex::tests
cargo test live_codex_rate_limits -- --ignored --nocapture
cargo check
git diff -- src/dashboard/providers/codex.rs tests/fixtures/codex-rate-limits.json
~~~

Expected: deterministic tests pass; the ignored live test returns only a percentage-range and future-reset assertion, not account details. Leave changes uncommitted.

---

### Task 5: Implement Claude authentication and usage retrieval

**Files:**
- Create: \`backend/src/dashboard/providers/claude.rs\`
- Create: \`backend/tests/fixtures/claude-usage.txt\`
- Modify: \`backend/src/dashboard/providers/mod.rs\`

**Interfaces:**
- Produces: \`ClaudeProvider<C: CaptureRunner, I: InteractiveRunner>\` implementing \`DataProvider<UsageData>\`.

- [ ] **Step 1: Add a sanitized usage fixture**

~~~text
Current session
23% used
Resets in 2 hr 15 min

Current week (all models)
41% used
Resets Sep 3 at 2:00 PM

Current week (model-specific preview)
80% used
Resets Sep 3 at 2:00 PM
~~~

- [ ] **Step 2: Write failing authentication and parser tests**

~~~rust
#[test]
fn unauthenticated_status_is_detected() {
    assert!(!parse_auth_status(r#"{"loggedIn":false,"authMethod":"none"}"#).unwrap());
}

#[test]
fn selects_the_most_restrictive_general_window() {
    let data = parse_usage(include_str!("../../../tests/fixtures/claude-usage.txt")).unwrap();
    assert_eq!(data.remaining_percent, 59);
    assert_eq!(data.reset_label.as_deref(), Some("Sep 3 at 2:00 PM"));
}

#[test]
fn rejects_non_plan_output() {
    assert_eq!(parse_usage("Session cost: $0.00"), Err(ProviderError::UnsupportedOutput));
}
~~~

- [ ] **Step 3: Confirm the tests fail**

~~~powershell
cargo test dashboard::providers::claude::tests --no-fail-fast
~~~

- [ ] **Step 4: Implement the authentication gate**

Run \`claude auth status --json\` through \`CaptureRunner\` with a 10-second timeout and deserialize only \`loggedIn\`. If false, return \`NotAuthenticated\` without starting a PTY.

- [ ] **Step 5: Implement the safe interactive usage call**

Start \`claude --safe-mode --no-chrome\` through \`InteractiveRunner\`. Send \`/usage\r\`, wait for \`Current session\` and \`Current week\`, capture bounded ANSI-stripped output, send \`/exit\r\`, and terminate the process. Use a 15-second timeout.

Parse only \`Current session\` and \`Current week (all models)\`. Exclude headings containing \`model-specific\`, \`Sonnet only\`, \`Opus only\`, or a model name. Select the highest used percentage, return \`100 - used\`, and store the text after \`Resets \` as \`reset_label\`.

- [ ] **Step 6: Complete the provider-owned login checkpoint**

~~~powershell
claude auth status --json
claude auth login
claude auth status --json
~~~

The planning probe returned \`"loggedIn": false\`. Complete the browser-owned sign-in and confirm the final status says \`true\`. Do not copy credentials into Dashy or Git.

- [ ] **Step 7: Verify Task 5**

~~~powershell
Set-Location D:\dev\Dashy\backend
cargo test dashboard::providers::claude::tests
cargo test live_claude_usage -- --ignored --nocapture
cargo check
git diff -- src/dashboard/providers/claude.rs tests/fixtures/claude-usage.txt
~~~

Expected: the live result matches the most restrictive general window in native \`/usage\`, without printing email or raw output. Leave changes uncommitted.

---

### Task 6: Add concurrent refresh, cache, stale fallback, and Tauri command

**Files:**
- Create: \`backend/src/dashboard/service.rs\`
- Create: \`backend/src/dashboard/commands.rs\`
- Modify: \`backend/src/dashboard/mod.rs\`
- Modify: \`backend/src/lib.rs\`

**Interfaces:**
- Produces: \`Clock\`, \`SystemClock\`, \`DashboardService::get_snapshot(force: bool)\`.
- Produces: \`AppState\` and \`get_dashboard_snapshot(force: Option<bool>)\`.

- [ ] **Step 1: Write failing service tests**

Use counting fake providers and a controllable clock:

~~~rust
#[tokio::test]
async fn reuses_snapshot_inside_five_minute_ttl() {
    let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
    fixture.service.get_snapshot(false).await;
    fixture.clock.advance_minutes(4);
    fixture.service.get_snapshot(false).await;
    assert_eq!(fixture.github.calls(), 1);
    assert_eq!(fixture.codex.calls(), 1);
    assert_eq!(fixture.claude.calls(), 1);
}

#[tokio::test]
async fn retains_last_good_value_after_one_timeout() {
    let fixture = ServiceFixture::successful_at("2026-08-29T09:00:00Z");
    fixture.service.get_snapshot(false).await;
    fixture.clock.advance_minutes(6);
    fixture.claude.fail_with(ProviderError::Timeout);
    let snapshot = fixture.service.get_snapshot(false).await;
    assert_eq!(snapshot.claude.status, ProviderStatus::Stale);
    assert_eq!(snapshot.claude.remaining_percent, Some(59));
    assert_eq!(snapshot.github.status, ProviderStatus::Connected);
}
~~~

Also test expiry, simultaneous callers, force refresh, partial first-load failure, and all three providers starting before any completes.

- [ ] **Step 2: Confirm service tests fail**

~~~powershell
cargo test dashboard::service::tests --no-fail-fast
~~~

- [ ] **Step 3: Implement the service**

~~~rust
const CACHE_TTL: chrono::Duration = chrono::Duration::minutes(5);

pub struct DashboardService {
    github: Arc<dyn DataProvider<GitHubData>>,
    codex: Arc<dyn DataProvider<UsageData>>,
    claude: Arc<dyn DataProvider<UsageData>>,
    clock: Arc<dyn Clock>,
    cache: tokio::sync::RwLock<Option<DashboardSnapshot>>,
    refresh_lock: tokio::sync::Mutex<()>,
}
~~~

\`get_snapshot\` checks the cache, acquires \`refresh_lock\`, checks again, runs \`tokio::join!\`, merges results independently, stores the snapshot, and returns it. \`force=true\` bypasses TTL but not single-flight protection.

Use this error mapping:

~~~rust
ProviderError::NotInstalled => (ProviderStatus::NotInstalled, ProviderErrorKind::MissingExecutable)
ProviderError::NotAuthenticated => (ProviderStatus::NotAuthenticated, ProviderErrorKind::Authentication)
ProviderError::Timeout => (ProviderStatus::Unavailable, ProviderErrorKind::Timeout)
ProviderError::UnsupportedOutput => (ProviderStatus::Unavailable, ProviderErrorKind::UnsupportedOutput)
ProviderError::Network => (ProviderStatus::Unavailable, ProviderErrorKind::Network)
ProviderError::Process => (ProviderStatus::Unavailable, ProviderErrorKind::Process)
~~~

Preserve last-good data and mark it stale after a failed refresh.

- [ ] **Step 4: Register the Tauri command**

~~~rust
#[tauri::command]
pub async fn get_dashboard_snapshot(
    state: tauri::State<'_, AppState>,
    force: Option<bool>,
) -> DashboardSnapshot {
    state.dashboard.get_snapshot(force.unwrap_or(false)).await
}
~~~

Create one shared \`SystemProcessRunner\`, construct real providers, manage \`AppState\`, and register \`tauri::generate_handler![get_dashboard_snapshot]\`.

- [ ] **Step 5: Verify Task 6**

~~~powershell
Set-Location D:\dev\Dashy\backend
cargo fmt --check
cargo test
cargo check
git diff -- src/dashboard/service.rs src/dashboard/commands.rs src/lib.rs
~~~

Expected: all deterministic backend tests pass. Leave changes uncommitted.

---

### Task 7: Replace frontend mock metrics with live snapshots

**Files:**
- Modify: \`frontend/package.json\`
- Modify: \`frontend/package-lock.json\`
- Modify: \`frontend/vite.config.ts\`
- Create: \`frontend/src/dashboard.ts\`
- Create: \`frontend/src/useDashboardSnapshot.ts\`
- Create: \`frontend/src/useDashboardSnapshot.test.tsx\`
- Create: \`frontend/src/DashboardMetrics.tsx\`
- Create: \`frontend/src/DashboardMetrics.test.tsx\`
- Modify: \`frontend/src/App.tsx\`
- Modify: \`frontend/src/App.test.tsx\`
- Modify: \`frontend/src/types.ts\`
- Delete: \`frontend/src/activity.ts\`
- Delete: \`frontend/src/activity.test.ts\`
- Modify: \`frontend/src/styles.css\`

**Interfaces:**
- Produces: matching TypeScript snapshot types, \`getDashboardSnapshot(force?)\`, \`useDashboardSnapshot()\`, and \`DashboardMetrics\`.

- [ ] **Step 1: Add React test dependencies**

~~~powershell
Set-Location D:\dev\Dashy
npm install --prefix frontend --save-dev @testing-library/jest-dom @testing-library/react jsdom
~~~

Set Vitest's environment to \`jsdom\`.

- [ ] **Step 2: Write failing contract and rendering tests**

~~~ts
export type ProviderStatus =
  | "connected"
  | "stale"
  | "notInstalled"
  | "notAuthenticated"
  | "unavailable";

export type UsageSnapshot = {
  status: ProviderStatus;
  remainingPercent: number | null;
  resetsAt: string | null;
  resetLabel: string | null;
  lastSuccessfulRefresh: string | null;
  errorKind: string | null;
};
~~~

~~~tsx
it("renders remaining allowance and reset information", () => {
  render(<DashboardMetrics snapshot={connectedSnapshot} refreshing={false} />);
  expect(screen.getByText("59%")).toBeInTheDocument();
  expect(screen.getByText(/Sep 3 at 2:00 PM/)).toBeInTheDocument();
});

it("never invents zero for unavailable usage", () => {
  render(<DashboardMetrics snapshot={unavailableSnapshot} refreshing={false} />);
  expect(screen.getByText("Usage unavailable")).toBeInTheDocument();
  expect(screen.queryByText("0%")).not.toBeInTheDocument();
});

it("renders GitHub contribution cells from backend data", () => {
  render(<DashboardMetrics snapshot={connectedSnapshot} refreshing={false} />);
  expect(screen.getByLabelText("3 contributions on 2026-08-28"))
    .toHaveAttribute("data-level", "3");
});
~~~

- [ ] **Step 3: Write failing five-minute refresh tests**

Mock \`getDashboardSnapshot\`, use fake timers, and assert one call on mount plus one after \`300_000\` ms. Add a rejected-refresh case that keeps the prior snapshot visible.

- [ ] **Step 4: Confirm frontend tests fail**

~~~powershell
Set-Location D:\dev\Dashy
npm test
~~~

Expected: missing dashboard modules and components.

- [ ] **Step 5: Implement the Tauri client and non-numeric browser fallback**

~~~ts
export async function getDashboardSnapshot(force = false): Promise<DashboardSnapshot> {
  if (!("__TAURI_INTERNALS__" in window)) return unavailableDashboardSnapshot();
  return invoke<DashboardSnapshot>("get_dashboard_snapshot", { force });
}
~~~

The browser fallback contains no metric values; all providers are unavailable.

- [ ] **Step 6: Implement the refresh hook**

Call once on mount, install one \`window.setInterval\` at \`300_000\` ms, clean it up on unmount, preserve previous values while refreshing, and keep previous data after an invoke rejection.

- [ ] **Step 7: Implement metric rendering and remove fixtures**

Move only the hero and usage cards into \`DashboardMetrics.tsx\`. Preserve class names, order, layout, and todos. Use \`remainingPercent\` directly. Prefer \`resetLabel\`; otherwise format \`resetsAt\` in local time. Render status copy:

~~~ts
const providerCopy = {
  notInstalled: "Not installed",
  notAuthenticated: "Sign in required",
  unavailable: "Usage unavailable",
  stale: "Last updated",
} as const;
~~~

Remove the hard-coded \`activity\`, \`usage\`, frontend streak function, and obsolete \`Usage\` type.

- [ ] **Step 8: Add state styles without redesigning**

Add \`.provider-state\`, \`.provider-reset\`, \`.usage.stale\`, and \`[aria-busy="true"]\` using existing custom properties. Do not change dimensions, glass effects, typography, or task styling.

- [ ] **Step 9: Verify Task 7**

~~~powershell
Set-Location D:\dev\Dashy
npm test
npm run build
git diff -- frontend package.json
~~~

Expected: all tests and build pass; todo and window-position tests remain unchanged. Leave changes uncommitted.

---

### Task 8: Add new-user installation and authentication documentation

**Files:**
- Create: \`install.ps1\`
- Modify: \`README.md\`

**Interfaces:**
- Produces: \`install.ps1 -CheckOnly\` and an idempotent install path.

- [ ] **Step 1: Implement check-only mode**

~~~powershell
[CmdletBinding()]
param([switch]$CheckOnly)

$ErrorActionPreference = 'Stop'
$requiredCommands = @('git', 'gh', 'node', 'npm', 'rustc', 'cargo', 'cargo-tauri', 'claude', 'codex')

function Test-DashyCommand([string]$Name) {
    [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

if ($CheckOnly) {
    $requiredCommands | ForEach-Object {
        [pscustomobject]@{ Command = $_; Installed = Test-DashyCommand $_ }
    } | Format-Table -AutoSize
    return
}
~~~

- [ ] **Step 2: Add idempotent winget installs**

Use \`winget install --exact --accept-package-agreements --accept-source-agreements\` for:

~~~text
Git.Git
GitHub.cli
OpenJS.NodeJS.LTS
Rustlang.Rustup
Microsoft.VisualStudio.2022.BuildTools
Microsoft.EdgeWebView2Runtime
Anthropic.ClaudeCode
OpenAI.Codex
~~~

For Build Tools, add \`Microsoft.VisualStudio.Workload.VCTools\` with recommended components. Skip installed packages; never remove or downgrade tools.

- [ ] **Step 3: Install project dependencies**

After refreshing the current process PATH from machine and user environment values:

~~~powershell
cargo install tauri-cli --version "^2.0.0" --locked
npm ci --prefix (Join-Path $PSScriptRoot 'frontend')
~~~

If a newly installed executable is not visible, print instructions to open a new PowerShell window and rerun. Do not start account login automatically.

- [ ] **Step 4: Validate the script**

~~~powershell
$scriptText = Get-Content -Raw D:\dev\Dashy\install.ps1
[void][ScriptBlock]::Create($scriptText)
& D:\dev\Dashy\install.ps1 -CheckOnly
~~~

Expected: syntax succeeds and the table matches the current machine.

- [ ] **Step 5: Update README**

Remove claims that provider metrics are mocked. Add:

~~~powershell
gh auth login
claude auth login
codex login
~~~

Document GitHub viewer contributions, Codex app-server limits, Claude's documented \`/usage\`, five-minute refresh, error states, and local-only credentials. Link only to official provider and tool documentation.

- [ ] **Step 6: Review Task 8**

~~~powershell
Set-Location D:\dev\Dashy
git diff -- install.ps1 README.md
~~~

Run every non-login README command and confirm no token instructions or machine-specific secrets appear. Leave changes uncommitted.

---

### Task 9: Complete end-to-end verification

**Files:**
- Modify only files required to correct failures discovered below.

**Interfaces:**
- Verifies the complete provider-to-Rust-to-Tauri-to-React path.

- [ ] **Step 1: Confirm identities and authentication**

~~~powershell
gh api user --jq .login
claude auth status --json
codex doctor
~~~

Expected: GitHub is \`adirangel\`, Claude is logged in, and Codex authentication is healthy. Do not inspect credential files.

- [ ] **Step 2: Run deterministic checks**

~~~powershell
Set-Location D:\dev\Dashy
npm test
npm run build
Set-Location D:\dev\Dashy\backend
cargo fmt --check
cargo test
cargo check
~~~

- [ ] **Step 3: Run ignored live-provider tests**

~~~powershell
Set-Location D:\dev\Dashy\backend
cargo test live_github_contributions -- --ignored --nocapture
cargo test live_codex_rate_limits -- --ignored --nocapture
cargo test live_claude_usage -- --ignored --nocapture
~~~

Expected: GitHub returns \`adirangel\`; Codex and Claude return remaining percentages and reset information without printing account output.

- [ ] **Step 4: Launch and compare the desktop app**

~~~powershell
Set-Location D:\dev\Dashy\backend
cargo tauri dev
~~~

Compare GitHub with the account contribution graph, Codex with the structured rate-limit result or native \`/status\`, and Claude with native \`/usage\`. Keep Dashy open for one five-minute refresh and confirm no duplicate or visible child processes.

- [ ] **Step 5: Verify failure states safely**

Use fake-provider tests for missing executable and timeout. Use supported logout/login flows only when an authentication state needs a live check; never rename executables or edit credential files. Confirm unavailable data never renders as zero.

- [ ] **Step 6: Inspect the final working tree**

~~~powershell
Set-Location D:\dev\Dashy
git diff --check
git status --short --branch
~~~

Expected: no whitespace errors, generated build output is not included, sanitized fixtures contain no account data, and all work remains uncommitted and unpushed for the user.
