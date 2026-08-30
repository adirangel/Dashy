# Dashy

Dashy is a local-first Windows side-notch for three real, CLI-backed signals:

- Claude general usage windows
- Codex general usage windows
- GitHub contribution activity and current streak

Dashy does not use demo metrics. It reads the authenticated command-line tools already
installed on the computer and renders only the bounded status and usage fields needed
by the interface.

## How the side notch works

Dashy normally sits completely outside the selected monitor's usable work area. It
does not reserve desktop space and its hidden window does not block edge clicks,
scrollbars, or other applications.

Move the pointer into the 28 px activation zone on the configured edge and hold it
there briefly to reveal the compact rail. Hover or focus Claude, Codex, or GitHub to
open its detail card. The rail and card form one safe pointer region; when the pointer
leaves it, the Rust desktop controller owns the short dismissal grace period.

Click a provider to pin its card and refresh only that provider. Duplicate refreshes
for the same provider are coalesced. A failed refresh keeps the last verified value
and marks it as stale instead of displaying a fabricated zero.

While a card is pinned, moving or tabbing across another metric does not replace it.
Clicking a different provider switches the pinned card and refreshes that provider in
one action; clicking the already pinned provider unpins it. Reveal and dismissal use
short edge-bound animations, while the Rust controller retains the bounded hide
fallback if the WebView cannot acknowledge an exit animation.

Keyboard behavior:

- Arrow keys move through providers vertically on the right and left placements.
- Left and Right Arrow move through providers on the top placement.
- Enter, Space, or clicking a provider pins it and starts its scoped refresh.
- Tab and Shift+Tab stay inside Dashy while a card is pinned.
- Escape closes a pinned or expanded card to the rail; a second Escape hides the rail.

Right-click the visible notch for its compact native menu.

## Placement, monitors, and fullscreen

Settings supports three physical placements:

- Right edge, with cards opening left
- Left edge, with cards opening right
- Top edge, with cards opening down

Choose the primary monitor or a specific connected monitor. Dashy persists a stable
monitor preference with recovery metadata. If that monitor is missing, Dashy safely
falls back to the primary monitor without deleting the saved preference. Work-area
positioning respects the Windows taskbar, and display or DPI changes trigger
repositioning.

Dashy hides when another application is fullscreen on the selected monitor by
default. Enable **Always show over fullscreen apps** in Settings to override that
behavior.

The system tray remains available while the notch is hidden or fullscreen-suppressed.
Its menu provides Show Dashy, Refresh all providers, placement and monitor choices,
Settings, and Quit Dashy. **Launch at startup** is opt-in and disabled by default.

## Languages

Dashy supports:

- English
- Hebrew
- Arabic
- Spanish
- Russian
- French
- Simplified Chinese
- Japanese

English is the first-run default. Hebrew and Arabic mirror the content layout for RTL
reading, but language direction never changes the user's physical screen placement.

## Provider data

GitHub activity comes from the authenticated GitHub CLI viewer's contribution
calendar. Dashy derives today's activity and the current streak without treating
GitHub as a percentage.

Codex usage comes from the local `codex app-server --stdio` rate-limit response.
Dashy retains the supported short and weekly general windows, summarizes the lower
remaining percentage in the compact ring, and excludes model-specific, preview, and
special-program buckets.

Claude usage comes from the documented Claude Code `/usage` interface. Dashy
retains the current-session and all-models general windows and excludes model-specific
preview limits.

The dashboard primes its local cache on startup and refreshes periodically. Each
provider remains isolated: one unavailable or signed-out CLI does not prevent the
other providers from working.

## Install Dashy on Windows

1. Open the [latest GitHub Release](https://github.com/adirangel/Dashy/releases/latest).
2. Download the Windows x64 `.msi` asset.
3. Run the installer and leave **Launch Dashy** selected on the final page.
4. In first-run setup, choose any combination of Claude, Codex, and GitHub.
5. Approve only the provider tools you want Dashy to install or connect.

The current private-test MSI is not code-signed, so Windows may show an
**Unknown publisher** or SmartScreen warning. Code signing is required before a
public release.

Node.js, Rust, Cargo, Tauri, and Visual Studio Build Tools are development
requirements only. End users do not install them.

## Contributor prerequisites

The requirements in this section apply only when developing or packaging Dashy
from this repository. They are not needed to install the Windows MSI above.

| Tool | Purpose |
|---|---|
| Git | Repository commands |
| GitHub CLI (`gh`) | GitHub contribution data and sign-in |
| Node.js LTS and npm | Frontend dependencies and Vite |
| Rust and Cargo | Rust backend and Tauri compilation |
| Microsoft C++ Build Tools | MSVC compiler and `link.exe` |
| Microsoft Edge WebView2 Runtime | Tauri's Windows webview |
| Tauri CLI 2 | Desktop development and packaging |
| Claude Code and Codex CLI | Claude and Codex metrics |

Official installation references:

- [Node.js downloads](https://nodejs.org/en/download)
- [Rust installation guide](https://doc.rust-lang.org/book/ch01-01-installation.html)
- [rustup](https://rustup.rs/)
- [Tauri v2 Windows prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri CLI documentation](https://v2.tauri.app/reference/cli/)
- [Microsoft C++ Build Tools command-line guidance](https://learn.microsoft.com/en-us/cpp/build/building-on-the-command-line?view=msvc-170)
- [Microsoft WebView2 Runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/)
- [GitHub CLI](https://cli.github.com/)
- [WinGet documentation](https://learn.microsoft.com/en-us/windows/package-manager/winget/)

The included `install.ps1` contributor bootstrap script uses WinGet package IDs
and requests the Visual Studio Build Tools C++ workload with recommended
components. The official installer may request elevation.

## Contributor setup

Open PowerShell in the repository. This contributor bootstrap script first lets you
inspect the development machine without changing it:

```powershell
.\install.ps1 -CheckOnly
```

`-CheckOnly` does not install software, change `PATH`, request administrator
access, or start a provider login.

To install missing prerequisites and the locked frontend dependencies:

```powershell
.\install.ps1
```

The installer skips available commands and packages, never removes or downgrades a
tool, refreshes only the current PowerShell process's `PATH`, installs Tauri CLI 2
only when needed, and runs `npm ci` in `frontend`. If a newly installed command is
still unavailable, open a new PowerShell window and rerun the installer.

## Provider CLI sign-in for contributors

The contributor bootstrap script deliberately never signs in for you. The installed
MSI instead presents provider onboarding on first run; use that flow to select the
providers you want Dashy to display. When developing from this repository, complete
only the provider CLI logins you need:

```powershell
gh auth login
claude auth login
codex login
```

Check the existing CLI sessions with:

```powershell
gh auth status --active
claude auth status --json
codex login status
```

On Windows, Dashy recognizes both a standalone Codex executable and the standard
global npm installation (`codex.cmd`). It launches the native executable bundled
with the npm package, so a successful `codex login status` is enough for Dashy too.
Restart Dashy after installing or updating a provider CLI so it inherits the latest
`PATH`.

Use each provider's own browser or terminal flow. Do not paste a token into Dashy,
the installer, source files, or this README.

Official authentication references:

- [GitHub CLI login](https://cli.github.com/manual/gh_auth_login) and [status](https://cli.github.com/manual/gh_auth_status)
- [Claude Code CLI reference](https://code.claude.com/docs/en/cli-reference) and [commands](https://code.claude.com/docs/en/commands)
- [Codex CLI guide](https://learn.chatgpt.com/docs/codex/cli) and [OpenAI authentication guide](https://learn.chatgpt.com/docs/auth)

## Run Dashy

For a browser-only visual preview:

```powershell
Set-Location frontend
npm run dev
```

Provider calls and native edge behavior are intentionally unavailable in a normal
browser preview.

For the Windows desktop application:

```powershell
Set-Location backend
cargo tauri dev --no-watch
```

The first run can take several minutes while Cargo builds dependencies. Dashy starts
hidden; move the pointer to the configured edge or use **Show Dashy** from the tray.

## Settings

Open Settings from the tray or notch context menu to configure:

- Placement
- Monitor
- Language
- Fullscreen override
- Launch at startup
- Provider status and manual refresh

Settings are stored locally. The startup option uses the operating system's normal
autostart registration and remains off until explicitly enabled.

## Privacy

Provider credentials remain owned by `gh`, `claude`, and `codex`. Dashy does not
request, store, print, or inspect tokens, browser cookies, password-manager data, or
credential files.

The Rust backend invokes the installed CLIs with bounded timeouts, parses only the
reviewed response fields, and sends the React UI sanitized provider states. Native
edge events contain only visibility, placement, and provider identifiers—never cursor
coordinates, monitor metadata, raw CLI output, account identity, or errors.

## Test and build

```powershell
Set-Location frontend
npm test
npm run build

Set-Location ..\backend
cargo fmt --check
cargo test --locked
cargo check --locked
```

To build Windows bundles:

```powershell
Set-Location backend
cargo tauri build -- --locked
```

Bundle output is generated under `backend/target/release/bundle` and is not tracked.

### Create a Windows release

Release tags are immutable. Start from an up-to-date `main` checkout and choose a
new, unused semantic version such as `0.2.0`; never move, delete, or reuse an
existing release tag.

Before starting a release, require an active GitHub tag ruleset that targets the
release-tag namespace (for example `refs/tags/v*`) and restricts both updates and
deletions. Do not give the release workflow a bypass. The workflow resolves the
exact fully qualified `refs/tags/<tag>` reference through GitHub's commits API,
peeling either a lightweight or annotated tag, immediately before any draft
mutation and checking it again after final asset/metadata verification. This avoids
falling back to a same-named branch when the tag is missing, but that fail-closed
check cannot by itself make a movable tag atomic with a Release API call. The
enforced tag ruleset is the external control that closes that race.

1. Change the exact same `MAJOR.MINOR.PATCH` value in all three release manifests:
   - `backend/tauri.conf.json`
   - `backend/Cargo.toml`
   - `frontend/package.json`

   Edit only those three version manifests manually. Immediately after changing
   `frontend/package.json`, update the two tracked root-version fields in the npm
   lockfile without changing dependency versions:

   ```powershell
   npm --prefix frontend install --package-lock-only
   ```

   Review the generated `frontend/package-lock.json` change. The Cargo gate below
   also regenerates the tracked `backend/Cargo.lock` package record. The release
   commit therefore contains exactly five files: the three manually edited
   manifests and those two generated lockfiles.
2. Run one intentional unlocked Cargo command to update the tracked root package
   record in `backend/Cargo.lock`, then run every final Cargo gate with the
   lockfile enforced. Do not run another unlocked Cargo command during release
   verification.

   ```powershell
   cargo check --manifest-path backend/Cargo.toml
   npm --prefix frontend ci
   npm run test:release
   npm --prefix frontend run test
   npm --prefix frontend run build
   cargo fmt --manifest-path backend/Cargo.toml --check
   cargo test --manifest-path backend/Cargo.toml --locked
   cargo clippy --manifest-path backend/Cargo.toml --all-targets --locked -- -D warnings
   ```

3. Validate, review, commit, tag, and push the release in one guarded flow.
   Replace `v0.2.0` only
   with the chosen new version; run this from a current local `main` branch.

   ```powershell
   $releaseTag = "v0.2.0"
   & node infrastructure/release/verify-version.mjs $releaseTag
   if ($LASTEXITCODE -ne 0) { throw "Release versions do not match $releaseTag." }

   $expectedReleaseFiles = @(
     "backend/Cargo.lock"
     "backend/Cargo.toml"
     "backend/tauri.conf.json"
     "frontend/package-lock.json"
     "frontend/package.json"
   ) | Sort-Object

   function Assert-ReleaseTagAvailable([string] $tag) {
     & git show-ref --verify --quiet "refs/tags/$tag"
     $localTagStatus = $LASTEXITCODE
     if ($localTagStatus -eq 0) { throw "Local tag $tag already exists; never move, delete, or reuse it." }
     if ($localTagStatus -ne 1) { throw "Could not determine whether local tag $tag exists." }

     $remoteRef = "refs/tags/$tag"
     $remoteTagLines = @(& git ls-remote --refs origin $remoteRef)
     $remoteTagStatus = $LASTEXITCODE
     if ($remoteTagStatus -ne 0) { throw "Could not query remote tag $tag; do not create it." }
     if ($remoteTagLines.Count -ne 0) { throw "Remote tag $tag already exists; never move, delete, or reuse it." }
   }

   $branchOutput = & git branch --show-current
   $branchStatus = $LASTEXITCODE
   $branch = ([string] $branchOutput).Trim()
   if ($branchStatus -ne 0 -or $branch -cne "main") { throw "Release from a current local main branch." }

   Assert-ReleaseTagAvailable $releaseTag

   & git diff --cached --quiet
   $initialIndexStatus = $LASTEXITCODE
   if ($initialIndexStatus -eq 1) { throw "Staged changes already exist; unstage and inspect them before a release." }
   if ($initialIndexStatus -ne 0) { throw "Could not inspect the staged index." }

   git diff --check
   if ($LASTEXITCODE -ne 0) { throw "Release diff has whitespace errors." }
   git diff --stat -- $expectedReleaseFiles
   if ($LASTEXITCODE -ne 0) { throw "Could not inspect the release diff stat." }
   git diff -- $expectedReleaseFiles
   if ($LASTEXITCODE -ne 0) { throw "Could not inspect the release diff." }

   git add -- $expectedReleaseFiles
   if ($LASTEXITCODE -ne 0) { throw "Could not stage the five release files." }
   git diff --cached --check
   if ($LASTEXITCODE -ne 0) { throw "Staged release diff has whitespace errors." }
   $stagedFileOutput = & git diff --cached --name-only
   $stagedFileStatus = $LASTEXITCODE
   if ($stagedFileStatus -ne 0) { throw "Could not enumerate staged release files." }
   $stagedFiles = @($stagedFileOutput | ForEach-Object { $_.Trim() } | Where-Object { $_ } | Sort-Object)
   $stagedDifference = @(Compare-Object -ReferenceObject $expectedReleaseFiles -DifferenceObject $stagedFiles)
   if ($stagedDifference.Count -ne 0) { throw "Release commit must stage exactly both lockfiles and the three version manifests." }

   & git diff --quiet
   $unstagedStatus = $LASTEXITCODE
   if ($unstagedStatus -eq 1) { throw "Unstaged tracked changes remain outside the release commit." }
   if ($unstagedStatus -ne 0) { throw "Could not inspect unstaged tracked changes." }
   $untrackedFiles = @(& git ls-files --others --exclude-standard)
   if ($LASTEXITCODE -ne 0) { throw "Could not inspect untracked files." }
   if ($untrackedFiles.Count -ne 0) { throw "Untracked files remain; release only from a clean worktree." }

   git diff --cached --stat -- $expectedReleaseFiles
   if ($LASTEXITCODE -ne 0) { throw "Could not inspect the staged release diff stat." }
   git diff --cached -- $expectedReleaseFiles
   if ($LASTEXITCODE -ne 0) { throw "Could not inspect the staged release diff." }

   git commit -m "release: $releaseTag"
   if ($LASTEXITCODE -ne 0) { throw "Could not commit release versions." }

   $worktreeState = @(& git status --porcelain)
   if ($LASTEXITCODE -ne 0 -or $worktreeState.Count -ne 0) { throw "Commit must leave a clean worktree." }

   & node infrastructure/release/verify-version.mjs $releaseTag
   if ($LASTEXITCODE -ne 0) { throw "Release versions no longer match $releaseTag." }
   Assert-ReleaseTagAvailable $releaseTag

   & git tag $releaseTag
   if ($LASTEXITCODE -ne 0) { throw "Could not create tag $releaseTag." }
   $tagCommitOutput = & git rev-list -n 1 $releaseTag
   $tagCommitStatus = $LASTEXITCODE
   if ($tagCommitStatus -ne 0) { throw "Could not resolve tag $releaseTag." }
   $tagCommit = ([string] $tagCommitOutput).Trim()
   $headCommitOutput = & git rev-parse HEAD
   $headCommitStatus = $LASTEXITCODE
   if ($headCommitStatus -ne 0) { throw "Could not resolve HEAD." }
   $headCommit = ([string] $headCommitOutput).Trim()
   if (-not $tagCommit -or $tagCommit -cne $headCommit) { throw "Release tag must point to HEAD." }

   & git push --atomic origin main $releaseTag
   if ($LASTEXITCODE -ne 0) { throw "Atomic main-and-tag push failed." }
   ```

   Never create a tag for an uncommitted tree or retag a different commit. If the
   workflow fails, fix the issue in a new commit and use a new patch version.

4. Wait for the `release-windows.yml` GitHub Actions workflow to succeed. Its
   read-only build job first fetches `origin/main` and rejects any tag whose commit
   is not in that branch's history. It validates the three manifest versions and
   both frontend lockfile version fields, builds and hashes the MSI, then transfers
   only that MSI and its matching checksum through immutable-pinned official
   artifact actions. A separate job with only `contents: write` downloads and
   revalidates those exact files before creating or recovering the draft; it does
   not check out or execute repository code. Before any create/upload operation and
   again after final draft verification, it requires the exact live
   `refs/tags/<tag>` reference's peeled commit to equal the triggering workflow SHA.
   A draft can remain partial or blocked while that workflow is still running or
   has failed; do not publish or manually complete it.
5. Inspect and verify the resulting draft. It must contain exactly one Windows x64
   `.msi` and its matching `.msi.sha256` checksum:

   ```powershell
   $releaseTag = "v0.2.0"
   $releaseDirectory = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "dashy-release-check-$([guid]::NewGuid())")
   & gh release download $releaseTag --dir $releaseDirectory.FullName
   if ($LASTEXITCODE -ne 0) { throw "Could not download draft release $releaseTag." }

   $msi = @(Get-ChildItem -LiteralPath $releaseDirectory.FullName -File -Filter *.msi)
   $checksums = @(Get-ChildItem -LiteralPath $releaseDirectory.FullName -File -Filter *.msi.sha256)
   if ($msi.Count -ne 1 -or $checksums.Count -ne 1) { throw "Expected exactly one MSI and one checksum." }
   if ($checksums[0].Name -cne "$($msi[0].Name).sha256") { throw "Checksum does not belong to the downloaded MSI." }

   $expectedHash = ((Get-Content -Raw -LiteralPath $checksums[0].FullName) -split '\s+')[0]
   $actualHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $msi[0].FullName).Hash
   if (-not [string]::Equals($actualHash, $expectedHash, [StringComparison]::OrdinalIgnoreCase)) {
     throw "Downloaded MSI checksum mismatch."
   }
   ```

6. Complete [the Windows release checklist](docs/WINDOWS_RELEASE_CHECKLIST.md) on
   a clean Windows x64 machine. Publish the draft only when every automated and
   clean-machine gate passes.

Private-test MSI releases are unsigned. Windows may show an **Unknown publisher**
or SmartScreen warning; do not describe an unsigned build as suitable for public
distribution.

## Troubleshooting

- **`winget` is missing:** install or update Microsoft App Installer, reopen
  PowerShell, and run `.\install.ps1 -CheckOnly`.
- **A newly installed command is still missing:** reopen PowerShell and run the
  check-only command again.
- **`link.exe` is missing:** install the Visual Studio Build Tools **Desktop
  development with C++** workload, then open a new shell.
- **A provider says “Not installed”:** install that provider's CLI and restart Dashy.
- **A provider says “Sign in required”:** complete its CLI login flow outside Dashy.
- **A provider is unavailable or stale:** the CLI may have timed out, lost network
  access, or changed its supported output. Dashy keeps the last verified local value.
- **The notch does not reveal:** check the selected placement and monitor, then use
  Show Dashy from the tray. A fullscreen application may be suppressing it.
- **A saved monitor is disconnected:** Dashy falls back to the primary monitor; reconnect
  it or choose a new monitor in Settings.
- **The tray is not visible:** check the Windows hidden-icons area.

## Tasks backlog

Todo and task management are intentionally absent from this release. There is no
partially available task surface or hidden task state. A possible optional fourth
metric is tracked only in [docs/BACKLOG.md](docs/BACKLOG.md).
