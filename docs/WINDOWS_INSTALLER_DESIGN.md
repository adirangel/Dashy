# Windows Installer and Provider Onboarding

## Status

Approved product and architecture design for Dashy's first Windows installer.

## Goals

- Publish a normal Windows x64 MSI as a GitHub Release asset.
- Let a new user install and run Dashy without installing developer tooling.
- Let each user enable any combination of Claude, Codex, and GitHub.
- Install missing provider CLIs only after explicit, provider-specific consent.
- Complete provider authentication through each provider's official login flow.
- Keep credentials outside Dashy and local to the provider tools that own them.

## Non-goals for the first release

- macOS or Linux packages.
- Windows ARM64 packages.
- Code signing or SmartScreen reputation.
- In-app automatic updates.
- Direct provider OAuth implemented or proxied by Dashy.
- Silent installation of third-party tools.
- Bundling Node.js, Rust, Tauri, Cargo, or Visual Studio Build Tools for end users.

## Installer experience

The MSI installs Dashy only. Build dependencies belong to developer machines and
the GitHub Actions runner, not to an end user's machine. Tauri's Windows bundler
owns the app packaging and required WebView2 bootstrap behavior.

The first release targets Windows x64. It is intentionally unsigned for private
testing, so the release notes must warn that Windows can show an Unknown Publisher
or SmartScreen prompt.

At the end of installation, the installer offers a checked-by-default `Launch
Dashy` option. When Dashy starts for the first time, it opens provider onboarding
instead of assuming that all three providers are required.

The WiX configuration must keep a stable upgrade code across releases so Windows
treats future MSIs as upgrades instead of installing duplicate applications.

Installing a newer MSI upgrades Dashy while preserving user settings. Uninstalling
Dashy must not uninstall provider CLIs, remove their credentials, or disconnect
their accounts.

## First-run onboarding

Onboarding presents independent cards for Claude, Codex, and GitHub. A user may
configure one provider, any pair, all three, or none. Each card has one of these
states:

- `Not installed`
- `Installed, sign-in required`
- `Connected`
- `Installing`
- `Connecting`
- `Needs attention`
- `Skipped`

The initial detection pass should recognize existing CLI installations and
authentication. Already connected providers require no redundant setup. A user
can skip any card and finish onboarding at any time.

Only enabled providers appear in the compact rail. Disabled or skipped providers
must not produce visible errors, background refreshes, or empty placeholders.
Settings exposes the same provider manager later, so onboarding is never the only
opportunity to add, remove, repair, or reconnect a provider.

The onboarding and provider-management UI must preserve all currently supported
locales: English, Hebrew, Arabic, Spanish, Russian, French, Simplified Chinese,
and Japanese. Layout direction follows the selected locale.

## CLI installation and consent

Dashy's current provider adapters depend on the official CLIs. The approved
WinGet package allowlist is:

| Provider | Executable | WinGet package ID |
| --- | --- | --- |
| Claude | `claude` | `Anthropic.ClaudeCode` |
| Codex | `codex` | `OpenAI.Codex` |
| GitHub | `gh` | `GitHub.cli` |

For a missing CLI, the provider card shows the product name, publisher, exact
package ID, and the command Dashy proposes to run. The user must approve that
provider before anything starts.

After approval, Dashy opens a visible PowerShell or Windows Terminal process and
runs the allowlisted `winget install` command. The child process remains visible
for UAC, license, and package-manager interaction. Dashy never constructs this
command from arbitrary UI text: the backend maps a provider enum to a fixed
package specification.

After the process exits, Dashy refreshes executable discovery and provider state.
If WinGet is unavailable, the package cannot be resolved exactly, the user
cancels, or installation fails, the card moves to `Needs attention` and presents
the official installation link plus manual instructions. Failure for one provider
does not block any other provider or the completion of onboarding.

## Provider authentication

Authentication is a separate consent step after CLI installation. Dashy launches
the official interactive command in a visible terminal:

| Provider | Login command | Status command |
| --- | --- | --- |
| Claude | `claude auth login --claudeai` | `claude auth status` |
| Codex | `codex login` | `codex login status` |
| GitHub | `gh auth login --web` | `gh auth status --hostname github.com` |

Claude and Codex use their official browser login to connect an eligible Claude
or ChatGPT subscription. GitHub uses the GitHub CLI browser flow. Dashy does not
display fields for passwords, API keys, OAuth codes, access tokens, or refresh
tokens.

Provider CLIs remain the credential owners. Dashy must not read, copy, transform,
log, transmit, or delete their stored credentials. It only interprets the minimum
status output and exit code needed to decide whether a provider is ready.

When an authenticated CLI later expires, is removed, or becomes unavailable, the
provider moves to `Needs attention`. The repair action runs only that provider's
install or login flow.

## Local state

Dashy stores only product preferences needed for onboarding and rail composition:

- whether first-run onboarding has been completed;
- the set of enabled providers;
- whether a provider was explicitly skipped;
- existing appearance, locale, placement, monitor, and startup preferences.

Provider connection state is discovered rather than persisted as a source of
truth. No credential material belongs in Dashy's settings, logs, cache, telemetry,
or frontend state.

## GitHub Release pipeline

A Windows GitHub Actions workflow runs for a semantic version tag such as
`v0.2.0`. It performs the following steps:

1. Verify that the tag and application version agree.
2. Install pinned Node.js and Rust toolchains on a Windows runner.
3. Restore dependencies from lockfiles.
4. Run the frontend and Rust verification suites.
5. Build Dashy with Tauri's MSI bundle target for Windows x64.
6. Generate a SHA-256 checksum for the MSI.
7. Create a draft GitHub Release.
8. Upload the MSI and checksum as release assets.

The workflow receives only the GitHub `contents: write` permission required to
create the release. Reusable actions are pinned to an immutable commit or explicit
working release instead of an unresolved moving alias. Publishing the draft is a
manual decision after installing and smoke-testing the artifact on Windows.

The first release notes state that the build is unsigned and document the expected
Windows warning. A future signed release must sign both the executable and MSI in
CI without committing certificate material to the repository.

## Acceptance criteria

- A clean Windows x64 user can install Dashy from a GitHub Release MSI.
- The machine does not need Node.js, Rust, Cargo, Tauri, or Visual Studio Build
  Tools to run the installed app.
- First launch opens localized provider onboarding.
- Every combination of enabled and skipped providers is supported.
- No provider installation or login begins without an explicit action from the
  user.
- Install and login processes are visible and limited to a static backend
  allowlist.
- Existing connected CLIs are detected without reinstalling or reconnecting them.
- Skipped providers do not appear in the rail or create provider refresh errors.
- Failed setup for one provider does not block the others.
- Dashy never stores provider credentials.
- Uninstalling Dashy leaves provider tools and accounts intact.
- A version tag produces a draft GitHub Release containing an x64 MSI and SHA-256
  checksum after all verification passes.

## Official references

- [Tauri Windows Installer](https://v2.tauri.app/distribute/windows-installer/)
- [Tauri GitHub pipeline](https://v2.tauri.app/distribute/pipelines/github/)
- [Claude Code authentication](https://code.claude.com/docs/en/authentication)
- [Claude Code CLI reference](https://code.claude.com/docs/en/cli-usage)
- [OpenAI authentication](https://learn.chatgpt.com/docs/auth)
- [GitHub CLI authentication](https://cli.github.com/manual/gh_auth_login)
- [GitHub Actions workflow syntax](https://docs.github.com/en/actions/reference/workflows-and-actions/workflow-syntax)
