# Creating a Windows release

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
   Set-Location frontend
   npm install --package-lock-only
   Set-Location ..
   ```

   Run it from inside `frontend`: invoking npm from the repository root with
   `--prefix frontend` makes npm link the root package into the frontend as a
   `file:..` dependency, which must not appear in either file.

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

6. Complete [the Windows release checklist](WINDOWS_RELEASE_CHECKLIST.md) on
   a clean Windows x64 machine. Publish the draft only when every automated and
   clean-machine gate passes.

Private-test MSI releases are unsigned. Windows may show an **Unknown publisher**
or SmartScreen warning; do not describe an unsigned build as suitable for public
distribution.
