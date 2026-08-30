# Windows MSI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Produce a stable, upgradeable, unsigned Windows x64 MSI that installs only Dashy, bootstraps WebView2 when needed, and launches the app from the installer finish page.

**Architecture:** Use Tauri's WiX MSI target with an explicitly pinned upgrade code and online WebView2 bootstrapper. Keep developer prerequisites in `install.ps1`, outside the end-user installer. Treat local MSI build and a clean Windows smoke test as release gates.

**Tech Stack:** Tauri 2.11, WiX Toolset v3 bundling, Cargo, PowerShell, Windows x64

**Spec:** `docs/WINDOWS_INSTALLER_DESIGN.md`

## Global Constraints

- This plan starts after both provider onboarding plans are complete.
- The first installer is Windows x64 only.
- The first installer is unsigned and intended for private testing.
- The MSI contains Dashy, not Node.js, Rust, Cargo, Tauri CLI, Visual Studio Build Tools, Claude CLI, Codex CLI, or GitHub CLI.
- The WiX upgrade code is permanently fixed to `ea949cb5-35f6-540d-a3ee-0cc7721c122c`.
- Downgrades are blocked.
- WebView2 uses Tauri's `downloadBootstrapper` mode, keeping the MSI small and requiring internet only when WebView2 is absent.
- Tauri's standard checked-by-default Launch App finish-page option is retained.
- Uninstalling Dashy must not remove provider tools, credentials, or settings owned by other products.

## File Structure

- `backend/tauri.conf.json`: MSI-only bundle settings, stable upgrade code, WebView2 mode, and downgrade policy.
- `backend/src/lib.rs`: configuration regression tests.
- `README.md`: separate end-user MSI instructions from contributor prerequisites.
- `docs/WINDOWS_RELEASE_CHECKLIST.md`: repeatable clean-install, upgrade, uninstall, and warning checks.

---

### Task 1: Pin the MSI bundle identity and runtime behavior

**Files:**
- Modify: `backend/src/lib.rs`
- Modify: `backend/tauri.conf.json`

**Interfaces:**
- Produces: one MSI bundle target with stable WiX upgrade identity.
- Preserves: product name `Dashy`, version `0.1.0`, identifier `app.dashy.desktop`.

- [ ] **Step 1: Write the failing configuration regression**

Add this test to the existing `config_tests` module in `backend/src/lib.rs`:

```rust
#[test]
fn windows_bundle_is_an_upgradeable_x64_msi_with_online_webview_bootstrap() {
    let config_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
    let config: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(config_path).unwrap()).unwrap();
    let bundle = &config["bundle"];
    let windows = &bundle["windows"];

    assert_eq!(bundle["targets"], serde_json::json!(["msi"]));
    assert_eq!(windows["allowDowngrades"], false);
    assert_eq!(windows["webviewInstallMode"], serde_json::json!({
        "type": "downloadBootstrapper",
        "silent": true
    }));
    assert_eq!(windows["wix"]["language"], "en-US");
    assert_eq!(
        windows["wix"]["upgradeCode"],
        "ea949cb5-35f6-540d-a3ee-0cc7721c122c"
    );
    assert!(windows["certificateThumbprint"].is_null());
}
```

- [ ] **Step 2: Run the regression and confirm current config fails**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml config_tests::windows_bundle_is_an_upgradeable_x64_msi_with_online_webview_bootstrap
```

Expected: FAIL because the current bundle target is `all` and Windows/WiX settings are absent.

- [ ] **Step 3: Configure only the approved MSI behavior**

Replace the `bundle` object in `backend/tauri.conf.json` with:

```json
"bundle": {
  "active": true,
  "targets": ["msi"],
  "icon": ["icons/icon.ico"],
  "windows": {
    "allowDowngrades": false,
    "webviewInstallMode": {
      "type": "downloadBootstrapper",
      "silent": true
    },
    "wix": {
      "language": "en-US",
      "upgradeCode": "ea949cb5-35f6-540d-a3ee-0cc7721c122c"
    }
  }
}
```

Do not add signing fields with empty strings. An absent certificate configuration is the intentional unsigned state.

- [ ] **Step 4: Validate config and run all config tests**

Run:

```powershell
cargo test --manifest-path backend/Cargo.toml config_tests
Push-Location backend
cargo tauri inspect wix-upgrade-code
Pop-Location
```

Expected upgrade code: `ea949cb5-35f6-540d-a3ee-0cc7721c122c`.

- [ ] **Step 5: Commit MSI identity**

```powershell
git add backend/tauri.conf.json backend/src/lib.rs
git commit -m "build: configure upgradeable Windows MSI"
```

---

### Task 2: Document the end-user path and release smoke test

**Files:**
- Modify: `README.md`
- Create: `docs/WINDOWS_RELEASE_CHECKLIST.md`

**Interfaces:**
- Produces: a no-command end-user install path.
- Preserves: `install.ps1` as contributor/developer setup only.

- [ ] **Step 1: Add a distinct end-user installation section**

Insert `## Install Dashy on Windows` before the current prerequisites section in `README.md`. Use this exact flow:

```markdown
## Install Dashy on Windows

1. Open the latest GitHub Release.
2. Download the Windows x64 `.msi` asset.
3. Run the installer and leave **Launch Dashy** selected on the final page.
4. In first-run setup, choose any combination of Claude, Codex, and GitHub.
5. Approve only the provider tools you want Dashy to install or connect.

The current private-test MSI is not code-signed, so Windows may show an
**Unknown publisher** or SmartScreen warning. Code signing is required before a
public release.

Node.js, Rust, Cargo, Tauri, and Visual Studio Build Tools are development
requirements only. End users do not install them.
```

Rename the existing `## Prerequisites` heading to `## Contributor prerequisites` and explicitly call `install.ps1` a contributor bootstrap script.

- [ ] **Step 2: Create the release checklist**

Create `docs/WINDOWS_RELEASE_CHECKLIST.md` with checkboxes for:

```markdown
# Windows Release Checklist

## Automated gates

- [ ] Frontend tests pass.
- [ ] Frontend production build passes.
- [ ] Rust formatting, tests, and Clippy pass.
- [ ] The MSI bundle completes for `x86_64-pc-windows-msvc`.
- [ ] The release includes an MSI and matching SHA-256 checksum.

## Clean-machine smoke test

- [ ] Test on Windows x64 without Node.js, Rust, Cargo, Tauri, or Visual Studio Build Tools.
- [ ] Record the expected unsigned-publisher warning for this private build.
- [ ] Complete MSI installation successfully.
- [ ] Leave **Launch Dashy** selected and confirm Dashy opens to first-run setup.
- [ ] Confirm WebView2 is downloaded only when the runtime is missing.
- [ ] Configure Claude only, Codex only, GitHub only, two-provider combinations, all three, and none.
- [ ] Confirm every install and login action requires its own approval.
- [ ] Confirm the provider login opens in a visible terminal/browser flow.
- [ ] Confirm Dashy never asks for or displays a token, API key, or password.

## Upgrade and uninstall

- [ ] Install the previous MSI, configure provider and display preferences, then install the new MSI.
- [ ] Confirm Windows upgrades the existing Dashy entry instead of creating a duplicate.
- [ ] Confirm Dashy preferences survive the upgrade.
- [ ] Uninstall Dashy from Installed apps.
- [ ] Confirm Dashy is removed.
- [ ] Confirm Claude, Codex, GitHub CLI, and their login sessions remain untouched.
```

- [ ] **Step 3: Check documentation formatting and links**

Run:

```powershell
git diff --check
rg -n "Install Dashy on Windows|Contributor prerequisites|Unknown publisher|Launch Dashy" README.md docs/WINDOWS_RELEASE_CHECKLIST.md
```

Expected: no whitespace errors and all four end-user concepts are present.

- [ ] **Step 4: Commit user and release documentation**

```powershell
git add README.md docs/WINDOWS_RELEASE_CHECKLIST.md
git commit -m "docs: add Windows MSI installation path"
```

---

### Task 3: Build and inspect the MSI locally

**Files:**
- Verify: `backend/target/release/bundle/msi/*.msi`

**Interfaces:**
- Verifies the artifact produced by Tasks 1–2; produces no source API.

- [ ] **Step 1: Run the complete pre-bundle verification**

Run:

```powershell
npm --prefix frontend run test
npm --prefix frontend run build
cargo fmt --manifest-path backend/Cargo.toml --check
cargo test --manifest-path backend/Cargo.toml
cargo clippy --manifest-path backend/Cargo.toml --all-targets -- -D warnings
```

Expected: every command exits 0 with zero test failures and zero Clippy warnings.

- [ ] **Step 2: Build only the x64 MSI**

Run:

```powershell
Set-Location backend
cargo tauri build --bundles msi --target x86_64-pc-windows-msvc
```

Expected: Tauri reports a finished MSI under `backend/target/x86_64-pc-windows-msvc/release/bundle/msi/` or `backend/target/release/bundle/msi/`, depending on the installed CLI's target-directory layout.

- [ ] **Step 3: Verify exactly one MSI and calculate its checksum**

Run from the repository root:

```powershell
$msi = @(Get-ChildItem -Path backend\target -Filter *.msi -Recurse | Where-Object FullName -Match '\\bundle\\msi\\')
if ($msi.Count -ne 1) { throw "Expected exactly one MSI, found $($msi.Count)." }
$msi | Select-Object FullName, Length
Get-FileHash -Algorithm SHA256 -LiteralPath $msi[0].FullName
```

Expected: one non-empty `.msi` and one 64-character SHA-256 hash.

- [ ] **Step 4: Perform the checklist smoke test in Windows Sandbox or a clean VM**

Copy only the MSI into a clean Windows x64 environment and execute every checkbox in `docs/WINDOWS_RELEASE_CHECKLIST.md`. Do not use the developer workstation as proof that build prerequisites are unnecessary.

- [ ] **Step 5: Record verification evidence in the release notes, not the repository**

Keep the repository clean after the build. The generated `backend/target` artifact remains ignored; the eventual GitHub Release workflow owns public artifact retention.
