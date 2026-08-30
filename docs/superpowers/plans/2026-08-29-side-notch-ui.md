# Dashy Side-Notch UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Dashy's conventional dashboard window with a Windows-native, proximity-revealed acrylic side-notch for Claude, Codex, and GitHub, with configurable placement, monitor, fullscreen behavior, startup behavior, and eight localized interfaces.

**Architecture:** Rust owns a pure edge-interaction state machine, Windows cursor/monitor/fullscreen probes, Tauri window geometry, settings persistence, tray menus, and provider-scoped refresh coordination. React owns the rail/card rendering, accessible input behavior, localized settings surface, and motion within the dynamically sized transparent window. The existing authenticated provider boundary remains isolated and privacy-safe while Claude and Codex retain both general usage windows instead of discarding one.

**Tech Stack:** Rust 2021, Tauri 2, Tokio, `windows` 0.61, `tauri-plugin-store` 2, `tauri-plugin-autostart` 2, React, TypeScript, Vite, Vitest, Testing Library, `i18next`, `react-i18next`, `simple-icons`, CSS acrylic/backdrop-filter.

**Spec:** `docs/superpowers/specs/2026-08-29-side-notch-ui-design.md`

## Global Constraints

- Work in the existing `D:/dev/Dashy` checkout and preserve all pre-existing user changes.
- Do not commit or push; the user explicitly owns the final commit and push.
- Do not run normal installer mode, change login state, expose raw provider output, or serialize account identity beyond the already-reviewed GitHub login field.
- The idle notch is completely outside the selected monitor work area; no transparent edge window may block clicks or scrollbars.
- Activation zone: 28 px. Reveal dwell: 100 ms. Close grace: 420 ms. Cursor sampling target: 40 ms.
- Rail enter: 220 ms ease-out. Card/provider transition: 180 ms ease-out. Exit: 180 ms ease-in.
- Supported placements in the first release: Right, Left, Top. Bottom is excluded.
- Primary monitor is the default; a user-selected monitor is persisted and missing monitors fall back to primary.
- Fullscreen suppression is enabled by default; `Always show over fullscreen apps` is the opt-out.
- `Launch at startup` exists and is disabled by default.
- English is the default locale. Supported locales are `en`, `he`, `ar`, `es`, `ru`, `fr`, `zh-CN`, and `ja`.
- Hebrew and Arabic are RTL. Placement does not change automatically with language direction.
- Claude and Codex compact rings show the minimum remaining percentage across their valid short and weekly general windows.
- GitHub compact state shows streak days, never a fabricated usage percentage.
- Missing provider data never renders a numeric zero. Stale data retains the last successful value.
- Tasks are absent from the first release; the optional fourth tasks metric remains only in `docs/BACKLOG.md`.
- All new behavior follows TDD: focused red test, minimal implementation, focused green test, then broader regression suite.

## File Structure Map

### Backend domain and providers

- `backend/src/dashboard/models.rs` — serialized provider contracts, dual usage-window model, provider identifier.
- `backend/src/dashboard/providers/claude.rs` — Claude short/weekly general-window extraction.
- `backend/src/dashboard/providers/codex.rs` — Codex primary/secondary general-window extraction.
- `backend/src/dashboard/service.rs` — full refresh plus provider-scoped refresh, cache merge, coalescing.
- `backend/src/dashboard/commands.rs` — dashboard snapshot and selected-provider refresh Tauri commands.

### Native desktop shell

- `backend/src/desktop/mod.rs` — desktop module exports and runtime bootstrap.
- `backend/src/desktop/settings.rs` — validated settings model, persistence abstraction, update notifications.
- `backend/src/desktop/edge.rs` — pure state machine and geometry.
- `backend/src/desktop/platform.rs` — platform probe interface and non-Windows safe fallback.
- `backend/src/desktop/windows.rs` — Win32 cursor, monitor work-area, and fullscreen implementation.
- `backend/src/desktop/controller.rs` — 40 ms runtime loop, Tauri show/hide/resize/reposition, UI events.
- `backend/src/desktop/menu.rs` — native tray and notch context menus.
- `backend/src/desktop/commands.rs` — settings, monitor, interaction, menu, and quit commands.
- `backend/src/lib.rs` — plugin initialization, managed state, desktop runtime, command registration.
- `backend/tauri.conf.json` — hidden notch window and hidden settings window definitions.
- `backend/capabilities/default.json` — least-privilege store/autostart/window/event permissions.
- `backend/Cargo.toml` / `backend/Cargo.lock` — native dependencies and Tauri features.

### Frontend application

- `frontend/src/dashboard.ts` — dual-window/provider-refresh TypeScript contract.
- `frontend/src/i18n.ts` — i18next initialization, locale metadata, fallback and direction helpers.
- `frontend/src/locales/*.ts` — eight locale resources with identical keys.
- `frontend/src/window.ts` — Tauri label routing and typed desktop commands/events.
- `frontend/src/App.tsx` — route the `main` window to notch and `settings` window to settings.
- `frontend/src/notch/NotchApp.tsx` — notch state composition and provider selection.
- `frontend/src/notch/MetricRail.tsx` — orientation-aware compact metrics.
- `frontend/src/notch/ProgressRing.tsx` — accessible summary ring and refresh animation.
- `frontend/src/notch/ProviderGlyph.tsx` — monochrome Claude, Codex, and GitHub glyphs.
- `frontend/src/notch/ProviderCard.tsx` — provider card dispatch and common status shell.
- `frontend/src/notch/UsageProviderCard.tsx` — Claude/Codex dual-window details.
- `frontend/src/notch/GitHubCard.tsx` — streak, today's activity, and 12-week heatmap.
- `frontend/src/settings/SettingsApp.tsx` — placement, monitor, language, fullscreen, startup, and provider status.
- `frontend/src/notch.css` / `frontend/src/settings.css` — visual system and orientation-specific geometry.
- `frontend/src/styles.css` — shared reset, acrylic tokens, typography, focus, and reduced motion.

---

### Task 1: Expand the Usage Contract Without Losing Privacy Guarantees

**Files:**
- Modify: `backend/src/dashboard/models.rs`
- Modify: `frontend/src/dashboard.ts`
- Test: `backend/src/dashboard/models.rs`
- Test: `frontend/src/DashboardMetrics.test.tsx` temporarily updated to compile against the new fixture shape

**Interfaces:**
- Produces Rust `UsageWindowKind`, `UsageWindowData`, expanded `UsageData`, expanded `UsageSnapshot`, and serialized `shortWindow` / `weeklyWindow`.
- Produces TypeScript `UsageWindowSnapshot` and updated `UsageSnapshot`.
- Keeps `remainingPercent` as the compact summary field so existing cache/UI boundaries remain incremental.

- [ ] **Step 1: Write failing Rust serialization and summary tests**

Add tests that construct separate short and weekly windows and require the lower remaining percentage to become the summary:

```rust
#[test]
fn usage_snapshot_keeps_both_windows_and_summarizes_the_lower_remaining_value() {
    let snapshot = UsageSnapshot::connected(
        UsageData {
            short_window: Some(UsageWindowData {
                label_key: UsageWindowKind::Short,
                remaining_percent: 71,
                resets_at: Some(Utc.with_ymd_and_hms(2026, 8, 29, 18, 0, 0).unwrap()),
                reset_label: None,
            }),
            weekly_window: Some(UsageWindowData {
                label_key: UsageWindowKind::Weekly,
                remaining_percent: 42,
                resets_at: Some(Utc.with_ymd_and_hms(2026, 9, 3, 0, 0, 0).unwrap()),
                reset_label: None,
            }),
        },
        Utc.with_ymd_and_hms(2026, 8, 29, 9, 0, 0).unwrap(),
    );

    let json = serde_json::to_value(snapshot).unwrap();
    assert_eq!(json["remainingPercent"], 42);
    assert_eq!(json["shortWindow"]["labelKey"], "short");
    assert_eq!(json["weeklyWindow"]["labelKey"], "weekly");
    assert_eq!(json["shortWindow"]["remainingPercent"], 71);
    assert_eq!(json["weeklyWindow"]["remainingPercent"], 42);
    assert!(json.get("usedPercent").is_none());
}
```

Also test one-window fallback, no-window rejection at provider construction boundaries, 0/100 clamping, failed snapshots with both windows null, and stale snapshots retaining both windows.

- [ ] **Step 2: Run the focused model tests and verify RED**

Run:

```powershell
Set-Location D:\dev\Dashy\backend
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test dashboard::models::tests --locked
```

Expected: compilation fails because `UsageWindowData`, `short_window`, and `weekly_window` do not exist.

- [ ] **Step 3: Implement the minimal Rust model**

Use these exact public shapes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UsageWindowKind {
    Short,
    Weekly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageWindowData {
    pub label_key: UsageWindowKind,
    pub remaining_percent: u8,
    pub resets_at: Option<DateTime<Utc>>,
    pub reset_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageData {
    pub short_window: Option<UsageWindowData>,
    pub weekly_window: Option<UsageWindowData>,
}
```

`UsageSnapshot` keeps `remaining_percent: Option<u8>` and adds `short_window` and `weekly_window`. Its connected/stale constructors clamp every value and compute the summary from the minimum present window. Do not add account metadata or raw provider labels.

- [ ] **Step 4: Update the TypeScript contract and unavailable fixture**

```ts
export type UsageWindowSnapshot = {
  labelKey: "short" | "weekly";
  remainingPercent: number;
  resetsAt: string | null;
  resetLabel: string | null;
};

export type UsageSnapshot = {
  status: ProviderStatus;
  remainingPercent: number | null;
  shortWindow: UsageWindowSnapshot | null;
  weeklyWindow: UsageWindowSnapshot | null;
  lastSuccessfulRefresh: string | null;
  errorKind: string | null;
};
```

Update every frontend test fixture explicitly; do not use `as UsageSnapshot` casts to suppress missing fields.

- [ ] **Step 5: Run focused and compilation suites**

Run backend model tests, `cargo check --locked`, and frontend `npm test`. Expected: all pass with no numeric value in unavailable snapshots.

- [ ] **Step 6: Review the diff and leave it uncommitted**

Run `git diff --check`. Confirm no provider identity or output fields were added.

---

### Task 2: Preserve Claude and Codex Short and Weekly General Windows

**Files:**
- Modify: `backend/src/dashboard/providers/claude.rs`
- Modify: `backend/src/dashboard/providers/codex.rs`
- Test: tests embedded in both provider modules

**Interfaces:**
- Consumes `UsageData { short_window, weekly_window }`, `UsageWindowData`, and `UsageWindowKind` from Task 1.
- Produces both general windows for the service while retaining the existing 15-second provider deadlines and strict parsing.

- [ ] **Step 1: Replace the old most-restrictive-only tests with failing dual-window tests**

Claude fixture requirements:

```rust
let usage = parse_usage(
    "Current session\n51% used\nResets 8:40 PM\nAll models\n36% used\nResets Thu 12:00 AM\n",
).unwrap();
assert_eq!(usage.short_window.unwrap().remaining_percent, 49);
assert_eq!(usage.weekly_window.unwrap().remaining_percent, 64);
```

Codex fixture requirements:

```rust
let usage = parse_rate_limits(&fixture_with_general_windows(28, 61)).unwrap();
assert_eq!(usage.short_window.unwrap().remaining_percent, 72);
assert_eq!(usage.weekly_window.unwrap().remaining_percent, 39);
```

Keep tests proving model-specific Claude headings and preview Codex buckets are excluded. Add tests that reject duplicate/missing required general headings rather than guessing.

- [ ] **Step 2: Run both provider test modules and verify RED**

Run:

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test dashboard::providers::claude::tests --locked
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test dashboard::providers::codex::tests --locked
```

Expected: assertions fail because each parser currently returns one selected window.

- [ ] **Step 3: Implement Claude dual-window parsing**

Keep the exact accepted headings `Current session` and `All models`. Convert the existing private `UsageWindow` values into `UsageWindowData`, assign current session to `short_window` with `UsageWindowKind::Short`, assign all models to `weekly_window` with `UsageWindowKind::Weekly`, and return `UnsupportedOutput` if either recognized section is malformed. Do not accept localized headings or model-specific sections.

- [ ] **Step 4: Implement Codex dual-window parsing**

Map the general bucket's `primary` window to `short_window` with `UsageWindowKind::Short` and `secondary` window to `weekly_window` with `UsageWindowKind::Weekly`. Preserve fallback to the reviewed top-level general bucket, strict unknown-field behavior, timestamp validation, and exclusion of preview/special buckets.

- [ ] **Step 5: Run provider tests, all deterministic backend tests, and live read-only tests**

Run focused tests, then:

```powershell
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test --locked
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test --locked -- --ignored
```

Expected: deterministic suite passes; the three existing live tests pass without `--nocapture` and disclose no account details.

- [ ] **Step 6: Review and leave uncommitted**

Confirm the parsers still use one absolute provider deadline and never serialize raw headings or account payloads.

---

### Task 3: Add Provider-Scoped Refresh With Cache Coalescing

**Files:**
- Modify: `backend/src/dashboard/models.rs`
- Modify: `backend/src/dashboard/service.rs`
- Modify: `backend/src/dashboard/commands.rs`
- Modify: `backend/src/lib.rs`
- Modify: `frontend/src/dashboard.ts`
- Modify: `frontend/src/useDashboardSnapshot.ts`
- Test: `backend/src/dashboard/service.rs`
- Test: `frontend/src/useDashboardSnapshot.test.tsx`

**Interfaces:**
- Produces serialized `ProviderId = "github" | "codex" | "claude"`.
- Produces `DashboardService::refresh_provider(provider: ProviderId) -> DashboardSnapshot`.
- Produces Tauri command `refresh_dashboard_provider(provider: ProviderId)` and frontend `refreshDashboardProvider(provider)`.

- [ ] **Step 1: Write failing service tests**

Add tests proving:

```rust
let before = fixture.service.get_snapshot(false).await;
let after = fixture.service.refresh_provider(ProviderId::Claude).await;
assert_eq!(fixture.github.calls(), 1);
assert_eq!(fixture.codex.calls(), 1);
assert_eq!(fixture.claude.calls(), 2);
assert_eq!(after.github, before.github);
assert_eq!(after.codex, before.codex);
```

Also cover duplicate concurrent Claude refreshes coalescing into one fetch, a provider-scoped timeout retaining only Claude's stale value, and a full refresh racing a selected refresh without overwriting a newer provider result.

Add one full-refresh failure-isolation test in which Claude fails while GitHub and Codex succeed; the resulting snapshot must keep Claude stale/unavailable as appropriate while publishing the two successful provider results.

- [ ] **Step 2: Run service tests and verify RED**

Expected: `ProviderId` and `refresh_provider` are missing.

- [ ] **Step 3: Implement the provider identifier and refresh coordination**

Use:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProviderId { GitHub, Codex, Claude }
```

Retain a single mutation lock for cache writes, one generation counter per provider, and one async in-flight gate per provider. A full refresh records all generations, acquires/fans out through the provider gates, fetches the eligible providers concurrently, merges under the cache lock, and increments each completed provider generation. A selected refresh records only its provider generation, waits for only that provider gate, re-checks the generation before fetching, returns the completed cached snapshot when another caller already advanced it, otherwise fetches only that provider, merges only that field, advances `refreshed_at`, and increments only that generation. Never hold a provider gate or cache lock while awaiting a different provider.

- [ ] **Step 4: Add the command and frontend API**

```rust
#[tauri::command]
pub async fn refresh_dashboard_provider(
    state: tauri::State<'_, AppState>,
    provider: ProviderId,
) -> Result<DashboardSnapshot, String>
```

```ts
export type ProviderId = "github" | "codex" | "claude";
export const refreshDashboardProvider = (provider: ProviderId) =>
  invoke<DashboardSnapshot>("refresh_dashboard_provider", { provider });
```

Extend the snapshot hook with `refreshProvider(provider)` and a `refreshingProviders: ReadonlySet<ProviderId>`. Suppress duplicate UI calls for the same provider but allow different providers to refresh concurrently at the React boundary.

- [ ] **Step 5: Run focused and full suites**

Run service tests, hook tests, all backend tests, frontend tests, and both builds/checks. Expected: periodic five-minute full refresh behavior remains unchanged.

- [ ] **Step 6: Review and leave uncommitted**

Confirm selected refresh does not clear or refetch unrelated providers.

---

### Task 4: Build Validated Desktop Settings and Persistence

**Files:**
- Create: `backend/src/desktop/mod.rs`
- Create: `backend/src/desktop/settings.rs`
- Create: `backend/src/desktop/commands.rs`
- Modify: `backend/src/lib.rs`
- Modify: `backend/Cargo.toml`
- Modify: `backend/Cargo.lock`
- Modify: `backend/capabilities/default.json`
- Test: `backend/src/desktop/settings.rs`

**Interfaces:**
- Produces `EdgePlacement`, `LocaleCode`, `MonitorPreference`, `StoredMonitorRect`, `AppSettings`, `SettingsPatch`, `SettingsService`, `get_settings`, and `update_settings`.
- Produces a Tokio watch receiver used by the edge controller in Task 7.

- [ ] **Step 1: Write failing settings tests**

Test exact defaults:

```rust
assert_eq!(AppSettings::default(), AppSettings {
    placement: EdgePlacement::Right,
    monitor: None,
    locale: LocaleCode::En,
    always_show_over_fullscreen: false,
});
```

Also test all eight locale codes, rejection of unknown locales/placements, preserving a stored unavailable monitor preference and its recovery metadata, fallback to defaults for malformed store content, and one change notification per successful update.

- [ ] **Step 2: Run settings tests and verify RED**

Expected: the desktop module and settings types do not exist.

- [ ] **Step 3: Add the settings types**

Use exact serialized values:

```rust
pub enum EdgePlacement { Right, Left, Top }
pub enum LocaleCode { En, He, Ar, Es, Ru, Fr, #[serde(rename = "zh-CN")] ZhCn, Ja }
pub struct StoredMonitorRect { pub x: i32, pub y: i32, pub width: u32, pub height: u32 }
pub struct MonitorPreference { pub id: String, pub name: String, pub last_work_area: StoredMonitorRect }
pub struct AppSettings {
    pub placement: EdgePlacement,
    pub monitor: Option<MonitorPreference>,
    pub locale: LocaleCode,
    pub always_show_over_fullscreen: bool,
}
```

`SettingsPatch` contains the same fields as `Option<T>` (with a nested nullable monitor value so the user can return to Primary). Validate monitor identifiers and names as non-empty bounded strings without path separators or control characters, and reject zero-size or overflow-prone recovery geometry. Match monitors by stable id first; use name/geometry only to recover from platform identifier changes. If no safe match exists, run on Primary without deleting the saved preference.

- [ ] **Step 4: Add persistence through the official Tauri store plugin**

Add `tauri-plugin-store = "2"`, initialize `tauri_plugin_store::Builder::default().build()`, and persist one `AppSettings` object under `settings.json`. Keep a memory persistence adapter for tests. No provider credentials or account fields may enter this store.

- [ ] **Step 5: Add settings commands and least-privilege capability**

```rust
#[tauri::command]
pub async fn get_settings(state: State<'_, DesktopState>) -> Result<AppSettings, String>;

#[tauri::command]
pub async fn update_settings(
    state: State<'_, DesktopState>,
    patch: SettingsPatch,
) -> Result<AppSettings, String>;
```

Grant `store:default` only to the `main` and future `settings` windows. Do not add filesystem or shell permissions.

- [ ] **Step 6: Run tests and verify store isolation**

Run desktop settings tests, backend full tests, `cargo check --locked`, and inspect capability JSON. Expected: settings round-trip and contain only approved fields.

- [ ] **Step 7: Review and leave uncommitted**

Run `git diff --check`; verify no machine-specific monitor name is committed in fixtures.

---

### Task 5: Implement the Pure Edge State Machine and Geometry

**Files:**
- Create: `backend/src/desktop/edge.rs`
- Modify: `backend/src/desktop/mod.rs`
- Test: `backend/src/desktop/edge.rs`

**Interfaces:**
- Consumes `EdgePlacement` and provider `ProviderId`.
- Produces `Point`, `Rect`, `MonitorWorkArea`, `EdgeUiState`, `EdgeInput`, `WindowLayout`, and `EdgeMachine::advance`.

- [ ] **Step 1: Write failing geometry tests for all orientations**

Use a 1920x1040 work area at `(0, 0)` and assert:

```rust
assert!(activation_zone(EdgePlacement::Right, work).contains(Point { x: 1919, y: 520 }));
assert!(activation_zone(EdgePlacement::Left, work).contains(Point { x: 0, y: 520 }));
assert!(activation_zone(EdgePlacement::Top, work).contains(Point { x: 960, y: 0 }));
assert!(!activation_zone(EdgePlacement::Right, work).contains(Point { x: 1890, y: 520 }));
```

Add negative-coordinate monitor cases and assert rail/card rectangles remain inside `rcWork`.

- [ ] **Step 2: Write failing timing/state tests**

Cover Hidden → Rail only after 100 ms continuously inside the 28 px zone; Rail → Card on provider hover; Card provider switching without Hidden; 420 ms close grace; Pinned ignoring pointer exit; outside click unpinning and returning focus to the selected metric; Suppressed on fullscreen; override returning to Hidden; Escape moving Pinned/Card → Rail and a second Escape moving Rail → Hidden; and no action for a cursor near a different monitor.

- [ ] **Step 3: Run focused tests and verify RED**

Expected: edge types and state machine do not exist.

- [ ] **Step 4: Implement pure geometry**

Use integer virtual-screen coordinates, including negative values. `WindowLayout` contains `position`, `size`, `visible`, `always_on_top`, `view_state`, and `placement`. Keep all Tauri and Win32 types outside this module.

- [ ] **Step 5: Implement the deterministic state machine**

`EdgeMachine::advance(now: Duration, input: EdgeInput) -> Vec<EdgeEffect>` must not read the clock itself. Effects are semantic:

```rust
pub enum EdgeEffect {
    ApplyWindow(WindowLayout),
    EmitView(EdgeViewState),
}
```

This permits paused-time tests and prevents platform behavior from leaking into transition logic.

- [ ] **Step 6: Run focused/full backend tests and review**

Expected: every timing boundary has an explicit test at one millisecond before and at the threshold. Leave uncommitted.

---

### Task 6: Add the Windows Cursor, Monitor, and Fullscreen Probe

**Files:**
- Create: `backend/src/desktop/platform.rs`
- Create: `backend/src/desktop/windows.rs`
- Modify: `backend/src/desktop/mod.rs`
- Modify: `backend/Cargo.toml`
- Modify: `backend/Cargo.lock`
- Test: helper tests in `backend/src/desktop/windows.rs`

**Interfaces:**
- Produces `DesktopProbe::cursor_position`, `monitors`, and `foreground_is_fullscreen`.
- Produces `MonitorDescriptor { id, name, monitor_rect, work_rect, primary }` using virtual-screen coordinates.

- [ ] **Step 1: Write failing pure helper tests**

Separate raw Win32 calls from classification helpers. Test monitor deduplication, primary selection, negative coordinates, fullscreen exact coverage, a 2 px tolerance, maximized-to-work-area not counting as fullscreen, and Dashy's own HWND never suppressing itself.

- [ ] **Step 2: Run focused tests and verify RED**

Expected: platform adapter types are missing.

- [ ] **Step 3: Add the Windows dependency and feature set**

```toml
[target.'cfg(windows)'.dependencies]
windows = { version = "0.61", features = [
  "Win32_Foundation",
  "Win32_Graphics_Gdi",
  "Win32_UI_HiDpi",
  "Win32_UI_WindowsAndMessaging"
] }
```

- [ ] **Step 4: Implement monitor and cursor calls**

Use `GetCursorPos` for screen coordinates, `EnumDisplayMonitors` plus `GetMonitorInfoW/MONITORINFOEXW` for stable device names and `rcMonitor`/`rcWork`, and preserve negative coordinates. Convert Win32 failures into a bounded internal desktop error; never panic the runtime loop.

- [ ] **Step 5: Implement fullscreen classification**

Use `GetForegroundWindow`, `GetWindowRect`, and `MonitorFromWindow`. A foreground window is fullscreen only when its rectangle covers the selected monitor rectangle within 2 px on every edge. Ignore Dashy's notch/settings HWNDs and treat missing/failed foreground queries as not fullscreen.

- [ ] **Step 6: Add a non-Windows fallback**

The fallback returns the platform monitor list where available, never reports global proximity/fullscreen, and leaves Dashy operable through tray/manual show. This keeps `cargo check` portable without pretending Windows behavior exists elsewhere.

- [ ] **Step 7: Run tests, format, and check**

Run Windows helper tests, full backend tests, `cargo fmt --check`, and `cargo check --locked`. Leave uncommitted.

Official implementation references: [GetCursorPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getcursorpos), [GetMonitorInfoW and work areas](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmonitorinfow), and [multi-monitor positioning](https://learn.microsoft.com/en-us/windows/win32/gdi/positioning-objects-on-multiple-display-monitors).

---

### Task 7: Integrate the Native Controller, Windows, Tray, and Autostart

**Files:**
- Create: `backend/src/desktop/controller.rs`
- Create: `backend/src/desktop/menu.rs`
- Modify: `backend/src/desktop/commands.rs`
- Modify: `backend/src/desktop/mod.rs`
- Modify: `backend/src/lib.rs`
- Modify: `backend/tauri.conf.json`
- Modify: `backend/capabilities/default.json`
- Modify: `backend/Cargo.toml`
- Modify: `backend/Cargo.lock`
- Test: `backend/tests/desktop_shell.rs`

**Interfaces:**
- Consumes edge effects, desktop probes, settings watch updates, dashboard provider refresh.
- Produces Tauri event `dashy://edge-view`, commands `set_notch_interaction`, `list_monitors`, `show_notch_menu`, `show_settings`, `quit_dashy`, and startup control.

- [ ] **Step 1: Write failing desktop-shell integration tests with fake window/probe ports**

Define an internal `WindowPort` trait so tests assert ordered effects without opening a real window:

```rust
trait WindowPort: Send + Sync {
    fn apply(&self, layout: &WindowLayout) -> Result<(), DesktopError>;
    fn emit_view(&self, state: &EdgeViewState) -> Result<(), DesktopError>;
}
```

Test: reveal does not focus the window; explicit tray show and provider click do focus it; card expansion resizes inward while keeping the rail edge anchored; losing focus after a pinned interaction produces the semantic outside-click input and unpins; hide completes after the CSS exit acknowledgement or a bounded fallback; settings changes reposition immediately; display/work-area/DPI changes reposition safely; fullscreen suppression hides; controller errors do not terminate later ticks.

- [ ] **Step 2: Run focused integration tests and verify RED**

Expected: controller and window port do not exist.

- [ ] **Step 3: Configure two Tauri windows**

Set `main` to transparent, undecorated, initially invisible, skip-taskbar, non-resizable by the user, and sized initially to the compact rail. Add a `settings` window using the same frontend entry point, initially invisible, centered, decorated, 460 x 620, and independently resizable only if content requires it. Keep `acrylic` on the notch; settings may use a standard opaque/acrylic Windows surface.

- [ ] **Step 4: Implement the 40 ms controller loop**

Spawn one cancellation-aware Tokio task in `setup`. Each tick reads current settings, selected monitor, cursor position, and fullscreen state, advances the pure machine, applies effects, and continues after recoverable probe/window errors. It must not hold settings or dashboard locks across Tauri window calls.

- [ ] **Step 5: Implement native tray and shared menu actions**

Enable Tauri's `tray-icon` feature. Build menu ids `show`, `refresh_all`, `placement_right`, `placement_left`, `placement_top`, a dynamic `monitor_<stable-id>` submenu, `settings`, and `quit`. Mark the active placement and resolved monitor, preserve the unavailable saved monitor preference, and fall back to Primary at runtime. Left tray click reveals/focuses Dashy explicitly. Menu actions reuse settings/dashboard/controller services rather than invoking frontend DOM actions. Build a second native menu with the same essential placement, monitor, refresh, settings, and quit controls for `show_notch_menu` and `popup_menu` at the cursor.

- [ ] **Step 6: Add localized tray-label updates**

Expose:

```rust
pub struct TrayLabels {
    pub show: String,
    pub refresh_all: String,
    pub placement: String,
    pub monitor: String,
    pub primary_monitor: String,
    pub settings: String,
    pub quit: String,
}
```

Validate each label as non-empty, no control characters, and at most 80 Unicode scalar values. The frontend sends the active translations after startup/language change; English labels exist as the native startup fallback.

- [ ] **Step 7: Add startup integration**

Add and initialize `tauri-plugin-autostart = "2"`. Expose startup state through the official plugin commands and grant only `autostart:allow-enable`, `autostart:allow-disable`, and `autostart:allow-is-enabled` to the settings window. Do not enable startup during setup; first-run remains disabled.

- [ ] **Step 8: Register commands and close behavior**

Register all dashboard and desktop commands. Closing the settings window hides it. Closing the notch window from an OS event hides it and keeps the tray runtime alive. Only the explicit `quit` menu/command exits the application.

- [ ] **Step 9: Run deterministic suites and a short manual shell smoke test**

Run backend tests/check/format. Then run `cargo tauri dev`, verify the process reaches ready state, tray icon appears, settings opens, and quit removes the exact Dashy process. Do not yet judge final visuals. Leave uncommitted.

Official references: [Tauri system tray](https://v2.tauri.app/learn/system-tray/), [native menus](https://v2.tauri.app/learn/window-menu/), [autostart plugin](https://v2.tauri.app/plugin/autostart/), and [store plugin](https://v2.tauri.app/plugin/store/).

---

### Task 8: Add Eight-Locale Infrastructure and the Settings Window

**Files:**
- Modify: `frontend/package.json`
- Modify: `frontend/package-lock.json`
- Create: `frontend/src/i18n.ts`
- Create: `frontend/src/locales/en.ts`
- Create: `frontend/src/locales/he.ts`
- Create: `frontend/src/locales/ar.ts`
- Create: `frontend/src/locales/es.ts`
- Create: `frontend/src/locales/ru.ts`
- Create: `frontend/src/locales/fr.ts`
- Create: `frontend/src/locales/zh-CN.ts`
- Create: `frontend/src/locales/ja.ts`
- Create: `frontend/src/window.ts`
- Create: `frontend/src/settings/SettingsApp.tsx`
- Create: `frontend/src/settings/SettingsApp.test.tsx`
- Create: `frontend/src/settings.css`
- Modify: `frontend/src/App.tsx`

**Interfaces:**
- Consumes settings/monitor commands and autostart plugin from Task 7.
- Produces `SUPPORTED_LOCALES`, `setLocale`, `directionForLocale`, `SettingsApp`, and typed desktop command wrappers.

- [ ] **Step 1: Install localization, icon, and startup bindings**

Add runtime dependencies `i18next`, `react-i18next`, `simple-icons`, and `@tauri-apps/plugin-autostart`. Do not add a second component library or CSS framework.

- [ ] **Step 2: Write failing locale-parity and direction tests**

Require every locale to have exactly the English leaf-key set. Assert `he` and `ar` return `rtl`; the other six return `ltr`; unknown stored values fall back to `en`; and switching locale updates `document.documentElement.lang` and `.dir`.

Use this exact top-level key structure:

```ts
type Messages = {
  providers: { claude: string; codex: string; github: string };
  usage: { shortWindow: string; weeklyWindow: string; remaining: string; resets: string };
  github: { streakDays: string; today: string; contributions: string; heatmapLabel: string };
  status: { loading: string; notInstalled: string; signInRequired: string; unavailable: string; stale: string; lastUpdated: string };
  guidance: { installClaude: string; installCodex: string; installGitHub: string; signInClaude: string; signInCodex: string; signInGitHub: string; retryLater: string };
  settings: { title: string; placement: string; right: string; left: string; top: string; monitor: string; language: string; fullscreen: string; startup: string; providerStatus: string };
  menu: { show: string; refreshAll: string; placement: string; monitor: string; primaryMonitor: string; settings: string; quit: string };
  actions: { refresh: string; refreshAll: string; openSettings: string; close: string };
};
```

- [ ] **Step 3: Run i18n tests and verify RED**

Expected: i18n module and locale resources are absent.

- [ ] **Step 4: Implement all eight complete locale resources**

Use concise native-language UI translations, not transliterated English. Keep product names Claude, Codex, and GitHub unchanged. English is the fallback and first-run default. Use interpolation for streak-day counts and reset timestamps, and rely on `Intl.NumberFormat` / `Intl.DateTimeFormat` for localized values.

- [ ] **Step 5: Write failing SettingsApp tests**

Mock typed commands and autostart bindings. Test initial values, saving each placement, monitor selection, eight-language selection, fullscreen toggle, startup enable/disable success, startup failure leaving the toggle unchanged, manual refresh-all, and provider status copy with provider-specific setup/sign-in guidance but without fake values.

- [ ] **Step 6: Implement window routing and settings UI**

`App.tsx` obtains the Tauri label through `getCurrentWindow().label`; render `SettingsApp` for `settings`, otherwise render `NotchApp` (introduced as a compile-safe placeholder component in this task and completed in Task 9). In browser tests without Tauri, accept a deterministic injected/default `main` label.

Settings saves Rust-owned fields with `update_settings`. Startup toggling calls `enable`/`disable`, then re-reads `isEnabled` before showing success. Manual refresh invokes the existing `getDashboardSnapshot(true)` / `get_dashboard_snapshot { force: true }` full-refresh boundary and reports isolated provider outcomes. Send translated tray labels after locale initialization and every language change.

- [ ] **Step 7: Run tests and build**

Run all frontend tests and `npm run build`. Test both LTR and RTL DOM direction and ensure no locale key renders as raw dotted text.

- [ ] **Step 8: Review and leave uncommitted**

Confirm no Google Fonts network import remains; use Segoe/system typography for offline startup.

---

### Task 9: Build the Acrylic Rail and Provider Cards

**Files:**
- Create: `frontend/src/notch/NotchApp.tsx`
- Create: `frontend/src/notch/NotchApp.test.tsx`
- Create: `frontend/src/notch/MetricRail.tsx`
- Create: `frontend/src/notch/ProgressRing.tsx`
- Create: `frontend/src/notch/ProviderGlyph.tsx`
- Create: `frontend/src/notch/ProviderCard.tsx`
- Create: `frontend/src/notch/UsageProviderCard.tsx`
- Create: `frontend/src/notch/GitHubCard.tsx`
- Create: `frontend/src/notch/heatmap.ts`
- Create: `frontend/src/notch.css`
- Modify: `frontend/src/styles.css`
- Modify: `frontend/src/App.tsx`
- Remove: active todo state/rendering from `frontend/src/App.tsx`
- Remove: obsolete `frontend/src/DashboardMetrics.tsx` after its tested logic moves into focused notch components
- Update/Test: `frontend/src/DashboardMetrics.test.tsx` migrated into provider-card/heatmap tests

**Interfaces:**
- Consumes localized copy, expanded dashboard snapshots, settings placement, provider-refresh hook, and desktop events.
- Produces accessible `MetricRail`, dual-window usage cards, GitHub card, and final acrylic visual tokens.

- [ ] **Step 1: Write failing compact-rail tests**

Test exact compact semantics:

- Claude/Codex ring value equals `remainingPercent` summary.
- GitHub center text equals translated/localized streak plus `d`-equivalent compact suffix, while ring intensity maps from today's real contribution level rather than a percentage.
- No unavailable provider renders `0`.
- Provider glyph exists only in the compact rail, not inside expanded card headings.
- Right/left use vertical orientation; top uses horizontal orientation.
- Every metric button has a 44 x 44 minimum hit target class and accessible name.

- [ ] **Step 2: Write failing detail-card tests**

Claude and Codex must render both available windows with independent percentages and reset times, using each typed `labelKey` to choose translated copy. GitHub must render current streak, today's count derived from the local ISO date, and the existing weekday-aligned 84-day heatmap. Test connected, stale, not-installed, not-authenticated, unavailable, and loading states in English plus one RTL locale. Not-installed and sign-in-required states must show the provider-specific translated setup guidance; unavailable uses translated retry guidance. No component may expose raw backend error text.

- [ ] **Step 3: Run focused tests and verify RED**

Expected: notch components do not exist.

- [ ] **Step 4: Implement reusable primitives**

`ProgressRing` uses an SVG circle with `stroke-dasharray`/`stroke-dashoffset`, clamps 0-100, exposes `role="progressbar"` only for numeric usage, and uses a non-progress accessible group for GitHub streak. `ProviderGlyph` reads path data from `simple-icons` and renders one monochrome SVG with a fixed viewBox.

- [ ] **Step 5: Implement provider cards**

`UsageProviderCard` receives `provider`, `shortWindow`, `weeklyWindow`, `status`, and `lastSuccessfulRefresh`. `GitHubCard` derives today's record by local ISO date and delegates month labels/weekday placement to pure `heatmap.ts`. Status rendering is shared by `ProviderCard`; never duplicate provider error logic.

- [ ] **Step 6: Implement the visual system**

Define the approved acrylic tokens from the spec exactly as the initial implementation baseline:

```css
:root {
  --glass-deep: rgba(7, 10, 12, 0.82);
  --glass-raised: rgba(13, 18, 21, 0.76);
  --glass-border: rgba(255, 255, 255, 0.12);
  --glass-highlight: rgba(255, 255, 255, 0.08);
  --text-primary: #f4f7f5;
  --text-secondary: #98a39d;
  --track: rgba(255, 255, 255, 0.12);
  --claude: #ff7548;
  --codex: #62e6a7;
  --github: #8deb78;
  --warning: #ffc56e;
  --error: #ff7474;
}
```

Use a 70 px side rail, a 250-280 px by approximately 70 px top rail, provider accent custom properties, tabular numerals, inner highlight, subtle border, and one notch pointer wedge. Right/left/top classes rotate geometry and motion without duplicating full rules. Remove the existing 380 x 470 dashboard/todo layout and Google Fonts import.

- [ ] **Step 7: Implement reduced motion and contrast safeguards**

In `prefers-reduced-motion: reduce`, disable translation and ring rotation while retaining instant state changes. Verify primary text reaches 4.5:1 against the effective raised glass surface and that neutral status never relies only on accent color.

- [ ] **Step 8: Run tests/build and inspect at three static orientations**

Run frontend tests/build. Render deterministic fixture states in browser/dev mode for Right, Left, and Top at 100% and 150% Windows scaling equivalents; capture screenshots over bright and dark backgrounds for implementation review.

- [ ] **Step 9: Review and leave uncommitted**

Confirm tasks are absent, provider cards have no duplicate icons, and GitHub has no percentage.

---

### Task 10: Wire Proximity Events, Hover Safety, Pinning, Refresh Motion, and Final Verification

**Files:**
- Modify: `frontend/src/notch/NotchApp.tsx`
- Modify: `frontend/src/notch/NotchApp.test.tsx`
- Modify: `frontend/src/window.ts`
- Modify: `frontend/src/useDashboardSnapshot.ts`
- Modify: `backend/src/desktop/controller.rs`
- Modify: `backend/src/desktop/commands.rs`
- Modify: `README.md`
- Verify: `install.ps1`
- Verify: `docs/BACKLOG.md`

**Interfaces:**
- Completes the `dashy://edge-view` event contract and semantic interaction commands.
- Produces the finished hidden → rail → card → pinned behavior across all placements.

- [ ] **Step 1: Write failing end-state interaction tests**

Frontend tests must assert:

- backend `RailVisible` state renders the rail without a card;
- provider pointer/focus invokes `set_notch_interaction({ kind: "selectProvider", provider })`;
- leaving the React safe region does not hide immediately; Rust owns the 420 ms grace;
- clicking a provider pins and calls only `refresh_dashboard_provider` for that provider;
- duplicate clicks while the provider is refreshing do not invoke again;
- window focus loss after a pinned click sends outside-click/unpin and restores the selected metric when Dashy remains visible;
- Escape from a pinned/card state closes the card to the rail and restores the selected metric; a second Escape hides the rail;
- arrow keys follow vertical order for side placement and horizontal order for top;
- Tab/Shift+Tab cannot move focus outside the visible Dashy surface while pinned;
- switching provider card uses one mounted card region and updates content without rail teardown.

Backend integration tests assert the matching semantic commands drive the pure state machine and emit one coherent view update.

- [ ] **Step 2: Run focused tests and verify RED**

Expected: event/command wiring is incomplete.

- [ ] **Step 3: Implement the typed event bridge**

Use:

```ts
export type EdgeViewState = {
  visibility: "hidden" | "rail" | "card" | "pinned" | "suppressed";
  placement: "right" | "left" | "top";
  provider: ProviderId | null;
};
```

Listen with `@tauri-apps/api/event`, clean up listeners on unmount, and do not create polling in React. Rust emits only bounded enum/numeric state, never cursor coordinates or monitor metadata.

- [ ] **Step 4: Complete semantic input handling**

Send `enterSafeRegion`, `leaveSafeRegion`, `selectProvider`, `clearProvider`, `togglePin`, `outsideClick`, and `escape` commands. Translate the notch window's native `Focused(false)` event into `outsideClick` only while pinned, so clicking elsewhere unpins without a global mouse hook. Rust remains authoritative for dismissal timers and window visibility. Explicit tray opening selects the rail and requests focus; proximity opening does not focus.

- [ ] **Step 5: Complete refresh feedback**

Animate only the selected provider's ring while its provider-scoped request is in flight. On success update the snapshot without closing the card. On failure retain stale data and announce the translated stale status through `aria-live="polite"`. Reduced motion changes rotation to a static busy indicator.

- [ ] **Step 6: Update user documentation**

Update the English README with side-notch behavior, placements, monitor selection, tray controls, fullscreen default, startup opt-in, supported languages, provider prerequisites, privacy statement, and troubleshooting. Keep the existing official Node/Rust/Tauri/CLI setup links. Document that tasks are intentionally absent and linked in the backlog, not partially available.

- [ ] **Step 7: Run the complete deterministic verification**

```powershell
Set-Location D:\dev\Dashy\frontend
npm test
npm run build

Set-Location D:\dev\Dashy\backend
& "$env:USERPROFILE\.cargo\bin\cargo.exe" fmt --check
& "$env:USERPROFILE\.cargo\bin\cargo.exe" test --locked
& "$env:USERPROFILE\.cargo\bin\cargo.exe" check --locked

Set-Location D:\dev\Dashy
[void][ScriptBlock]::Create((Get-Content -LiteralPath .\install.ps1 -Raw))
& .\.superpowers\sdd\2026-08-29-real-data-integrations\task-8-helper-tests.ps1
$beforePath = $env:PATH
& .\install.ps1 -CheckOnly
if ($env:PATH -cne $beforePath) { throw 'CheckOnly changed PATH' }
git diff --check
git status --short --branch
```

Expected: all tests/build/checks pass; CheckOnly is non-mutating; changes remain uncommitted and unpushed.

- [ ] **Step 8: Run live read-only provider verification**

Run the three ignored provider tests without `--nocapture`. Expected: GitHub, Codex, and Claude pass while output contains only test names/status, never usage values or account details.

- [ ] **Step 9: Run Windows end-to-end visual and lifecycle verification**

Start one exact `cargo tauri dev` session and verify:

- hidden state blocks no edge clicks;
- entering the 28 px zone reveals after the dwell;
- all three provider cards remain open across rail-to-card pointer travel;
- right, left, and top placements anchor correctly;
- selected monitor changes and missing-monitor fallback work;
- fullscreen suppression and override work;
- English, Hebrew, Arabic, Spanish, Russian, French, Chinese, and Japanese switch without raw keys or clipping;
- Hebrew and Arabic mirror content while preserving placement;
- tray and context-menu placement/monitor actions, settings, startup toggle, manual refresh, pin/unpin including outside click, and quit work;
- the exact dev process and provider children are gone after quit.

Do not alter real login state or run the installer in normal mode.

- [ ] **Step 10: Final privacy, generated-output, and source-control audit**

Search pending files for tokens, email addresses, organization/account fixture keys, raw CLI output, generated `dist`, and Cargo `target` output. Confirm branch divergence remains unchanged and no commit/push occurred. Leave the working tree for the user.

---

## Execution Order and Dependency Notes

1. Task 1 establishes the contract used by every later provider/UI task.
2. Task 2 must finish before provider cards can display dual windows.
3. Task 3 establishes provider-scoped refresh before click/pin behavior.
4. Task 4 establishes persisted settings consumed by native and frontend work.
5. Task 5 is pure and must be accepted before any Win32/Tauri runtime code depends on it.
6. Task 6 implements the platform probe consumed by Task 7.
7. Task 7 produces the native runtime and commands needed by Tasks 8-10.
8. Task 8 establishes window routing, localization, and settings before final visuals.
9. Task 9 builds the complete visual surface against deterministic state.
10. Task 10 wires interaction and performs full end-to-end verification.

Tasks must execute sequentially. Do not run simultaneous implementers because Tasks 1-10 share contracts and working-tree files.
