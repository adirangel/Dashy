# Real Data Integrations Design

**Date:** 2026-08-29  
**Status:** Approved  
**Scope:** Replace the mocked GitHub, Claude, and Codex dashboard values with real local-account data.

## Context

Dashy currently renders hard-coded GitHub contribution, Claude usage, and Codex usage values. The desktop app already follows a local-first architecture: React owns presentation, while the Rust/Tauri backend owns native capabilities. The machine already has GitHub CLI, Claude Code, and Codex CLI installed, and GitHub CLI is authenticated as `adirangel`.

This change introduces a native data integration layer. It does not redesign the interface or modify the task system.

## Goals

- Display the authenticated GitHub account's real contribution calendar and current contribution streak.
- Display the real remaining Claude subscription allowance and its reset time.
- Display the real remaining Codex subscription allowance and its reset time.
- Refresh data when Dashy opens and every five minutes afterward.
- Keep credentials and provider output inside the native backend.
- Degrade independently when one provider is unavailable.
- Never present a fabricated percentage or silently substitute API billing usage for subscription usage.

## Non-goals

- Redesigning the dashboard.
- Changing the task list or adding task synchronization.
- Supporting multiple GitHub, Claude, or Codex accounts in the first version.
- Tracking OpenAI or Anthropic API-key billing.
- Scraping provider web pages, calling private web endpoints, or reading browser cookies.
- Reading or copying provider credential files.
- Persisting provider snapshots across application restarts in the first version.
- Running a separate background service outside the Tauri application.

## Selected Approach

Provider adapters will run inside the Rust/Tauri backend. Each adapter owns one external integration and returns a normalized result. A dashboard service runs the adapters concurrently, applies caching and timeouts, and exposes one Tauri command to the React frontend.

This approach keeps native process execution out of the browser context, isolates version-sensitive parsers, and allows one provider to fail without affecting the others.

## Architecture

The backend will be divided into focused modules:

- `dashboard/models.rs` defines the serialized dashboard snapshot and provider states.
- `dashboard/service.rs` coordinates refreshes, caching, timeouts, and stale-data fallback.
- `dashboard/providers/github.rs` queries GitHub through the authenticated `gh` CLI.
- `dashboard/providers/codex.rs` obtains Codex allowance data through the installed Codex CLI.
- `dashboard/providers/claude.rs` obtains Claude allowance data through the installed Claude CLI.
- `dashboard/providers/process.rs` contains the allowlisted, hidden-process runner used by the CLI providers.
- `dashboard/commands.rs` exposes the Tauri command consumed by React.

The exact file split may be adjusted to match Rust module conventions, but these responsibilities must remain isolated.

The frontend will contain:

- A serialized TypeScript representation of the backend snapshot.
- A dashboard-data hook that performs the initial request and starts a five-minute refresh timer.
- Presentation mapping that replaces the existing hard-coded GitHub, Claude, and Codex values without changing the approved visual layout.

## Data Contract

The backend returns one `DashboardSnapshot`:

```text
DashboardSnapshot
├── github
│   ├── status
│   ├── accountLogin
│   ├── contributionDays[]
│   │   ├── date
│   │   ├── count
│   │   └── level
│   ├── currentStreakDays
│   ├── lastSuccessfulRefresh
│   └── errorKind?
├── codex
│   ├── status
│   ├── remainingPercent?
│   ├── resetsAt?
│   ├── resetLabel?
│   ├── lastSuccessfulRefresh
│   └── errorKind?
├── claude
│   ├── status
│   ├── remainingPercent?
│   ├── resetsAt?
│   ├── resetLabel?
│   ├── lastSuccessfulRefresh
│   └── errorKind?
└── refreshedAt
```

Provider status is one of:

- `connected`: the current refresh succeeded.
- `stale`: the refresh failed but a previous in-memory value is available.
- `notInstalled`: the required executable was not found.
- `notAuthenticated`: the executable is installed but its account session is unavailable.
- `unavailable`: the provider timed out, returned unsupported output, or failed for another recoverable reason.

Percentages always mean **remaining allowance**, not used allowance. If a provider reports usage consumed, the adapter converts it to remaining allowance and clamps the result to the inclusive `0..100` range. `resetsAt` holds a machine-readable timestamp when the provider supplies one; `resetLabel` preserves a sanitized provider reset description when only display text is available. A missing or unparseable value stays absent; it is never replaced with zero.

## GitHub Provider

The GitHub adapter will execute an argument-safe `gh api graphql` request. It will query `viewer.login` and `viewer.contributionsCollection.contributionCalendar`, including each contribution day's date, count, and GitHub contribution level.

The account is derived from `viewer.login`; no username or credential is embedded in the application. On the current development machine, the live integration test must confirm that the returned login is `adirangel`.

The contribution streak is computed from GitHub's returned calendar dates:

1. Sort contribution days by date.
2. Treat a day as active when its contribution count is greater than zero.
3. Walk backward through consecutive active days.
4. If today has no contribution but yesterday is active, keep the streak active through the end of today.
5. Handle month and year boundaries without resetting the streak.

GitHub's returned dates are authoritative for the calendar. Dashy's local date is used only to choose whether the backward walk begins with today or yesterday.

## Claude and Codex Providers

Dashy will use the installed, authenticated Claude and Codex CLIs as the supported local account boundary. It will not call private provider endpoints or read authentication stores directly.

The Codex adapter will use Codex's structured local app-server protocol rather than parse its terminal UI. It will start `codex app-server --stdio`, complete the JSON-RPC `initialize` handshake, call `account/rateLimits/read`, and read the general `codex` rate-limit bucket. Model-specific buckets remain outside this card.

The Claude adapter will use the documented `/usage` command because the CLI does not currently expose an equivalent non-interactive usage JSON command. It will first call `claude auth status --json`; an unauthenticated or expired session maps directly to `notAuthenticated` without opening the interactive UI.

The adapters will:

1. Resolve the executable from `PATH` without invoking a shell.
2. Start the CLI in a hidden Windows process using an interactive terminal runner where required.
3. Issue the provider's supported usage interface: Codex JSON-RPC or Claude `/usage`.
4. Capture only the bounded output needed to identify remaining allowance and reset time.
5. Normalize the result into the shared usage contract.
6. Close the session immediately after parsing the response.

The implementation must verify the exact output shape against the installed CLI versions. Parsers will accept only recognized fields, labels, and formats. If a CLI version does not expose subscription allowance through its supported command surface, the adapter returns `unavailable`; the implementation must not fall back to private APIs or estimates.

When a provider exposes more than one general plan window, Dashy selects the most restrictive active window: the window with the lowest remaining percentage. Its reset time is displayed with that percentage. Model-specific promotional or preview limits are excluded unless they are the provider's only general allowance.

Provider output is version-sensitive. Parsing logic therefore lives entirely inside the corresponding adapter and is covered by sanitized fixture tests.

## Refresh and Cache Flow

1. React requests a snapshot when the dashboard mounts.
2. The dashboard service returns a valid in-memory snapshot immediately when it is less than five minutes old.
3. When data is missing or expired, the service refreshes GitHub, Claude, and Codex concurrently.
4. Each provider has an independent timeout and result.
5. Successful results replace that provider's cache entry.
6. Failed refreshes retain the provider's last successful in-memory result and mark it `stale`.
7. React keeps the previous visible values while a refresh is in progress, preventing layout flashes.
8. React requests another snapshot every five minutes while the dashboard is mounted.

The service uses a single-flight guard so simultaneous frontend requests do not start duplicate CLI processes. The cache is memory-only and disappears when Dashy exits.

Initial timeout targets are 15 seconds for each local CLI and 20 seconds for GitHub. They may be tightened after observing real startup behavior, but no provider may block the full snapshot beyond its own timeout.

## Error Handling and User States

- Missing executable: show `Not installed` and link to the relevant setup instructions.
- Missing authentication: show `Sign in required`.
- Network failure or timeout with cached data: show the previous value and `Last updated ...`.
- Network failure or timeout without cached data: show `Usage unavailable` or `GitHub unavailable`.
- Unsupported CLI output: show `Usage unavailable` and record a sanitized parse-error category.
- One provider failure: render the other provider results normally.

The UI must not display `0%`, an empty contribution calendar, or a zero-day streak unless that value was successfully obtained or computed from valid provider data.

## Security and Privacy

- Execute only allowlisted programs: `gh`, `claude`, and `codex`.
- Pass arguments directly to the process API; never interpolate provider data into a shell command.
- Keep spawned windows hidden on Windows.
- Apply output-size bounds and process timeouts.
- Terminate child processes after success, failure, or timeout.
- Never send raw CLI output, tokens, cookies, or environment secrets to React.
- Never write raw provider output or account details to logs.
- Log only provider name, normalized error category, duration, and timestamp.
- Keep all snapshots local to the running Dashy process.

## Frontend Behavior

The existing visual components remain in place. Their mocked values are replaced with live snapshot values.

- GitHub renders the real contribution cells and calculated streak.
- Claude and Codex render remaining allowance and reset time.
- Existing values remain visible during background refresh.
- Provider-state labels replace numeric content when no valid value exists.
- The global refresh indicator represents the aggregate request, while each card retains its independent state.

No design changes, icon changes, resizing, or layout work are included in this phase.

## Documentation and Installation

`README.md` and `install.ps1` will be updated for new users. The instructions will cover:

- Installing GitHub CLI, Claude Code, and Codex CLI.
- Running `gh auth login`.
- Signing in through the supported Claude CLI flow.
- Running `codex login` or signing in through the supported Codex flow.
- Verifying each executable is available on `PATH`.
- Explaining that Dashy reads supported CLI status output and never stores provider credentials.

Installation links must point to the providers' official documentation.

## Testing Strategy

### Rust unit tests

- Parse sanitized successful Claude and Codex usage output.
- Reject missing, malformed, localized, and changed output without inventing values.
- Normalize used versus remaining percentages correctly.
- Parse GitHub GraphQL responses and contribution levels.
- Calculate streaks across today, yesterday, empty days, month boundaries, and year boundaries.
- Map process failures to the correct provider status.

### Service tests

- Run fake providers concurrently.
- Reuse a snapshot within the five-minute TTL.
- Refresh after expiration.
- Prevent overlapping refreshes with the single-flight guard.
- Retain last-good data and mark it stale after a failed refresh.
- Ensure one timeout does not discard successful sibling-provider results.

### Frontend tests

- Render connected, stale, not-installed, not-authenticated, and unavailable states.
- Keep previous values visible during refresh.
- Render GitHub cells from backend data rather than fixtures.
- Render the remaining percentage and reset time with the correct meaning.

### Live integration and verification

- Confirm `viewer.login` is `adirangel` on the development machine.
- Compare the rendered contribution calendar and streak with GitHub.
- Compare Claude and Codex values with their native usage/status displays.
- Run frontend tests and production build.
- Run `cargo test` and the applicable Rust checks.
- Launch `cargo tauri dev` and confirm successful startup and one complete refresh cycle.

Live-provider tests must be separate from deterministic unit tests and must not expose account output in committed fixtures or logs.

## Acceptance Criteria

- No GitHub, Claude, or Codex card uses a hard-coded metric.
- GitHub shows contributions for the account returned by authenticated `gh`; on the target machine this is `adirangel`.
- The displayed GitHub streak follows the approved today/yesterday rule.
- Claude and Codex show real remaining subscription allowance and reset time when their supported CLI surfaces provide them.
- The displayed Claude and Codex values match the native CLI display on the target machine.
- Refresh occurs at startup and at five-minute intervals without overlapping process runs.
- A provider failure never fabricates a value or prevents other provider cards from updating.
- Dashy does not store credentials or use private provider endpoints.
- Existing dashboard layout and task behavior remain unchanged.
- All automated checks pass and `cargo tauri dev` completes a successful live refresh.

## Implementation Boundary

This specification is intentionally limited to real data integrations. Visual refinements, window glass effects, widget sizing, and task functionality will be handled separately after the integrations are working and verified.
