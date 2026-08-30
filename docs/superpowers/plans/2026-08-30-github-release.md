# GitHub Windows Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn a validated semantic-version tag into a tested Windows x64 MSI, SHA-256 checksum, and manually publishable draft GitHub Release.

**Architecture:** A small dependency-free Node script verifies that the tag, Tauri config, Cargo package, and frontend package share one version. A least-privilege Windows GitHub Actions job runs the full verification suite, builds through the official Tauri action, creates a draft release, and uploads a checksum beside the MSI.

**Tech Stack:** GitHub Actions, Windows runner, Node.js 22.14.0, Rust 1.98.0, Tauri CLI 2.11.4, official `tauri-apps/tauri-action`, GitHub CLI

**Spec:** `docs/WINDOWS_INSTALLER_DESIGN.md`

## Global Constraints

- This plan starts after `2026-08-30-windows-msi.md` is complete and its local MSI smoke test passes.
- Releases are triggered only by tags matching `vMAJOR.MINOR.PATCH`.
- The tag version must exactly match `backend/tauri.conf.json`, `backend/Cargo.toml`, and `frontend/package.json`.
- The workflow runs on Windows and targets `x86_64-pc-windows-msvc` only.
- The GitHub token receives only `contents: write` in the release job.
- Every reusable action is pinned to the reviewed immutable commit listed in this plan.
- The release remains a draft until a human completes the clean-machine checklist.
- The release body states that this private-test build is unsigned.
- No signing secret, provider credential, or developer token is added to the repository.
- The release contains one MSI and one matching `.sha256` file; no NSIS installer or plain executable is uploaded.

## File Structure

- `infrastructure/release/verify-version.mjs`: pure version validation plus a CLI entry point.
- `infrastructure/release/verify-version.test.mjs`: Node test-runner coverage for malformed and mismatched releases.
- `.github/workflows/release-windows.yml`: least-privilege Windows test/build/draft-release workflow.
- `README.md`: maintainer release commands and draft publication process.
- `docs/WINDOWS_RELEASE_CHECKLIST.md`: final release gates created by the MSI plan.

---

### Task 1: Add a dependency-free release-version gate

**Files:**
- Create: `infrastructure/release/verify-version.mjs`
- Create: `infrastructure/release/verify-version.test.mjs`
- Modify: `package.json`

**Interfaces:**
- Produces: `validateReleaseVersions(tag, versions)`
- Produces CLI: `node infrastructure/release/verify-version.mjs v0.1.0`
- Produces npm script: `npm run test:release`

- [ ] **Step 1: Write failing pure validation tests**

Create `infrastructure/release/verify-version.test.mjs`:

```javascript
import test from "node:test";
import assert from "node:assert/strict";
import { validateReleaseVersions } from "./verify-version.mjs";

const matching = { tauri: "0.1.0", cargo: "0.1.0", frontend: "0.1.0" };

test("accepts one exact semantic version across tag and manifests", () => {
  assert.doesNotThrow(() => validateReleaseVersions("v0.1.0", matching));
});

test("rejects malformed and prerelease tags", () => {
  for (const tag of ["0.1.0", "release-0.1.0", "v0.1", "v0.1.0-beta.1"]) {
    assert.throws(() => validateReleaseVersions(tag, matching), /vMAJOR\.MINOR\.PATCH/);
  }
});

test("reports every manifest that disagrees with the tag", () => {
  assert.throws(
    () => validateReleaseVersions("v0.2.0", { tauri: "0.2.0", cargo: "0.1.0", frontend: "0.3.0" }),
    /backend\/Cargo\.toml=0\.1\.0.*frontend\/package\.json=0\.3\.0/,
  );
});
```

- [ ] **Step 2: Run the test and confirm the module is missing**

Run:

```powershell
node --test infrastructure/release/verify-version.test.mjs
```

Expected: FAIL because `verify-version.mjs` does not exist.

- [ ] **Step 3: Implement the validator and manifest readers**

Create `infrastructure/release/verify-version.mjs`:

```javascript
import fs from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

const TAG_PATTERN = /^v(\d+)\.(\d+)\.(\d+)$/;

export function validateReleaseVersions(tag, versions) {
  const match = TAG_PATTERN.exec(tag);
  if (!match) throw new Error("Release tag must use vMAJOR.MINOR.PATCH.");
  const expected = match.slice(1).join(".");
  const labels = {
    tauri: "backend/tauri.conf.json",
    cargo: "backend/Cargo.toml",
    frontend: "frontend/package.json",
  };
  const mismatches = Object.entries(versions)
    .filter(([, version]) => version !== expected)
    .map(([name, version]) => `${labels[name]}=${version}`);
  if (mismatches.length) {
    throw new Error(`Tag ${tag} does not match ${mismatches.join(", ")}.`);
  }
}

function cargoPackageVersion(source) {
  const packageStart = source.indexOf("[package]");
  if (packageStart < 0) throw new Error("backend/Cargo.toml has no [package] section.");
  const nextSection = source.indexOf("\n[", packageStart + 1);
  const section = source.slice(packageStart, nextSection < 0 ? undefined : nextSection);
  const match = /^version\s*=\s*"([^"]+)"\s*$/m.exec(section);
  if (!match) throw new Error("backend/Cargo.toml has no package version.");
  return match[1];
}

export function readReleaseVersions(root = process.cwd()) {
  const tauri = JSON.parse(fs.readFileSync(path.join(root, "backend/tauri.conf.json"), "utf8"));
  const frontend = JSON.parse(fs.readFileSync(path.join(root, "frontend/package.json"), "utf8"));
  const cargo = fs.readFileSync(path.join(root, "backend/Cargo.toml"), "utf8");
  return { tauri: tauri.version, cargo: cargoPackageVersion(cargo), frontend: frontend.version };
}

const entry = process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url;
if (entry) {
  try {
    validateReleaseVersions(process.argv[2] ?? "", readReleaseVersions());
    process.stdout.write(`Release version ${process.argv[2]} is consistent.\n`);
  } catch (error) {
    process.stderr.write(`${error instanceof Error ? error.message : "Release version validation failed."}\n`);
    process.exitCode = 1;
  }
}
```

- [ ] **Step 4: Add the root test script**

Add this script to root `package.json`:

```json
"test:release": "node --test infrastructure/release/verify-version.test.mjs"
```

- [ ] **Step 5: Run positive and negative gates**

Run:

```powershell
npm run test:release
node infrastructure/release/verify-version.mjs v0.1.0
node infrastructure/release/verify-version.mjs v9.9.9
```

Expected: tests pass, `v0.1.0` exits 0, and `v9.9.9` exits non-zero while naming all three mismatched manifests.

- [ ] **Step 6: Commit the version gate**

```powershell
git add infrastructure/release package.json
git commit -m "build: validate release versions"
```

---

### Task 2: Build and upload a draft Windows release

**Files:**
- Create: `.github/workflows/release-windows.yml`

**Interfaces:**
- Consumes: semantic version tag and the version validator from Task 1.
- Produces: draft GitHub Release containing Tauri's single generated `.msi` and a matching `.msi.sha256` asset.

- [ ] **Step 1: Create the pinned least-privilege workflow**

Create `.github/workflows/release-windows.yml` with this exact structure:

```yaml
name: Release Windows MSI

on:
  push:
    tags:
      - "v[0-9]*.[0-9]*.[0-9]*"

permissions: {}

jobs:
  release-windows:
    runs-on: windows-latest
    permissions:
      contents: write
    env:
      CARGO_TERM_COLOR: always
    steps:
      - name: Check out the release commit
        uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1

      - name: Install Node.js
        uses: actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7.0.0
        with:
          node-version: "22.14.0"
          cache: npm
          cache-dependency-path: frontend/package-lock.json

      - name: Install Rust
        uses: dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c
        with:
          toolchain: "1.98.0"
          targets: x86_64-pc-windows-msvc
          components: rustfmt, clippy

      - name: Restore frontend dependencies
        run: npm --prefix frontend ci

      - name: Verify release version
        shell: pwsh
        run: node infrastructure/release/verify-version.mjs $env:GITHUB_REF_NAME

      - name: Test release tooling
        run: npm run test:release

      - name: Verify frontend
        run: |
          npm --prefix frontend run test
          npm --prefix frontend run build

      - name: Verify Rust
        run: |
          cargo fmt --manifest-path backend/Cargo.toml --check
          cargo test --manifest-path backend/Cargo.toml
          cargo clippy --manifest-path backend/Cargo.toml --all-targets -- -D warnings

      - name: Install pinned Tauri CLI
        run: cargo install tauri-cli --version 2.11.4 --locked

      - name: Build MSI and create draft release
        id: tauri
        uses: tauri-apps/tauri-action@1deb371b0cd8bd54025b384f1cd735e725c4060f # v1.0.0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        with:
          projectPath: backend
          tauriScript: cargo tauri
          tagName: ${{ github.ref_name }}
          releaseName: "Dashy v__VERSION__"
          releaseBody: |
            Private Windows x64 test build.

            This MSI is not code-signed. Windows may show an Unknown publisher or SmartScreen warning.
            Complete the Windows release checklist before publishing this draft.
          releaseDraft: true
          prerelease: false
          generateReleaseNotes: true
          uploadUpdaterJson: false
          args: --bundles msi --target x86_64-pc-windows-msvc

      - name: Create MSI checksum
        id: checksum
        shell: pwsh
        run: |
          $artifacts = '${{ steps.tauri.outputs.artifactPaths }}' | ConvertFrom-Json
          $msi = @($artifacts | Where-Object { $_ -like '*.msi' })
          if ($msi.Count -ne 1) { throw "Expected one MSI artifact, found $($msi.Count)." }
          $checksumPath = "$($msi[0]).sha256"
          $hash = (Get-FileHash -Algorithm SHA256 -LiteralPath $msi[0]).Hash.ToLowerInvariant()
          "$hash  $([IO.Path]::GetFileName($msi[0]))" | Set-Content -Encoding ascii -NoNewline -LiteralPath $checksumPath
          "checksum_path=$checksumPath" >> $env:GITHUB_OUTPUT

      - name: Upload checksum to the draft release
        shell: pwsh
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: gh release upload $env:GITHUB_REF_NAME '${{ steps.checksum.outputs.checksum_path }}' --clobber
```

- [ ] **Step 2: Validate action pins against the reviewed tags**

Run:

```powershell
gh api repos/actions/checkout/git/ref/tags/v7.0.1 --jq .object.sha
gh api repos/actions/setup-node/git/ref/tags/v7.0.0 --jq .object.sha
gh api repos/tauri-apps/tauri-action/git/ref/tags/v1.0.0 --jq .object.sha
gh api repos/dtolnay/rust-toolchain/commits/stable --jq .sha
```

Expected SHAs, in order:

```text
3d3c42e5aac5ba805825da76410c181273ba90b1
820762786026740c76f36085b0efc47a31fe5020
1deb371b0cd8bd54025b384f1cd735e725c4060f
4360b52568e2003a75bf9bc1d59f33a8e3fc893c
```

If any reviewed tag now resolves to a different commit, stop and investigate rather than silently updating the plan or workflow.

- [ ] **Step 3: Verify the workflow references only one bundle type**

Run:

```powershell
rg -n "contents: write|--bundles msi|x86_64-pc-windows-msvc|releaseDraft: true|uploadUpdaterJson: false|certificate|nsis" .github/workflows/release-windows.yml
```

Expected: the required least-privilege, MSI, x64, draft, and no-updater settings are present; no certificate secret or NSIS build is present.

- [ ] **Step 4: Commit the workflow**

```powershell
git add .github/workflows/release-windows.yml
git commit -m "ci: publish draft Windows MSI releases"
```

---

### Task 3: Document the maintainer release process

**Files:**
- Modify: `README.md`
- Modify: `docs/WINDOWS_RELEASE_CHECKLIST.md`

**Interfaces:**
- Consumes: workflow and version gate from Tasks 1–2.
- Produces: deterministic maintainer steps from version change through draft publication.

- [ ] **Step 1: Add release commands to README**

Add a `### Create a Windows release` subsection under the build documentation:

```markdown
### Create a Windows release

1. Update the same `MAJOR.MINOR.PATCH` value in:
   - `backend/tauri.conf.json`
   - `backend/Cargo.toml`
   - `frontend/package.json`
2. Run the full test/build commands above and `npm run test:release`.
3. Commit the version change.
4. Create and push the matching tag:

   ```powershell
   git tag v0.2.0
   git push origin v0.2.0
   ```

5. Open the draft GitHub Release, download the MSI and `.sha256`, and complete
   `docs/WINDOWS_RELEASE_CHECKLIST.md` on a clean Windows x64 machine.
6. Publish the draft only after every release checkbox passes.

The initial private-test releases are unsigned. Do not describe an unsigned build
as suitable for public distribution.
```

- [ ] **Step 2: Add GitHub workflow evidence to the checklist**

Under `Automated gates` in `docs/WINDOWS_RELEASE_CHECKLIST.md`, add:

```markdown
- [ ] The tag version matches Tauri, Cargo, and frontend package versions.
- [ ] All workflow actions resolve to the reviewed immutable commits.
- [ ] The GitHub Release is still a draft during smoke testing.
- [ ] The downloaded MSI hash matches the published `.sha256` file.
```

- [ ] **Step 3: Run documentation and release-tool checks**

Run:

```powershell
git diff --check
npm run test:release
node infrastructure/release/verify-version.mjs v0.1.0
rg -n "Create a Windows release|release-windows.yml|WINDOWS_RELEASE_CHECKLIST|unsigned" README.md docs/WINDOWS_RELEASE_CHECKLIST.md
```

Expected: no whitespace errors, version tests pass, current version validates, and release documentation contains every required link/concept.

- [ ] **Step 4: Commit maintainer documentation**

```powershell
git add README.md docs/WINDOWS_RELEASE_CHECKLIST.md
git commit -m "docs: document Windows release workflow"
```

---

### Task 4: Prove the tagged workflow end to end

**Files:**
- Verify: `.github/workflows/release-windows.yml`
- Verify: GitHub draft release assets

**Interfaces:**
- Verifies all release interfaces produced by Tasks 1–3; produces no new source API.

- [ ] **Step 1: Run the same gates locally**

Run:

```powershell
npm --prefix frontend ci
npm run test:release
npm --prefix frontend run test
npm --prefix frontend run build
cargo fmt --manifest-path backend/Cargo.toml --check
cargo test --manifest-path backend/Cargo.toml
cargo clippy --manifest-path backend/Cargo.toml --all-targets -- -D warnings
```

Expected: every command exits 0.

- [ ] **Step 2: Push a private-test semantic version tag**

After the version commit is on `main`, create the exact matching tag and push only that tag:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

If `v0.1.0` already exists, increment all three manifests to the next unused version, commit that version, and tag the exact new value. Never move or overwrite an existing release tag.

- [ ] **Step 3: Inspect the workflow and draft release**

Run:

```powershell
$runId = gh run list --workflow release-windows.yml --limit 1 --json databaseId --jq '.[0].databaseId'
gh run watch $runId --exit-status
gh release view v0.1.0 --json isDraft,assets,url
```

Use the actual tag from Step 2 in the final command. Expected: workflow conclusion `success`, `isDraft: true`, one `.msi`, and one `.msi.sha256` asset.

- [ ] **Step 4: Verify the downloaded checksum**

Download both assets into a new temporary directory, then compare the MSI hash with the first field in the checksum file:

```powershell
$releaseDir = New-Item -ItemType Directory -Path (Join-Path $env:TEMP "dashy-release-check-$([guid]::NewGuid())")
gh release download v0.1.0 --dir $releaseDir.FullName
$msi = Get-ChildItem $releaseDir.FullName -Filter *.msi
$checksum = Get-Content -Raw "$($msi.FullName).sha256"
$expected = ($checksum -split '\s+')[0]
$actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $msi.FullName).Hash.ToLowerInvariant()
if ($actual -ne $expected) { throw "Release checksum mismatch." }
```

Use the actual tag from Step 2. Expected: no exception and exactly one MSI.

- [ ] **Step 5: Complete the clean-machine checklist before publishing**

Run every item in `docs/WINDOWS_RELEASE_CHECKLIST.md`. Publish the draft in GitHub only after all items pass. If any item fails, leave the release as a draft, fix the source on a new commit, and issue a new immutable patch-version tag.
