# Windows Release Checklist

Use this checklist for every private Windows x64 MSI candidate before publishing
or promoting a GitHub Release. Record the results with the release evidence; do
not treat a developer workstation as a clean-machine result.

## Automated gates

- [ ] The release tag is `vMAJOR.MINOR.PATCH` and exactly matches the versions in
      `backend/tauri.conf.json`, `backend/Cargo.toml`, `frontend/package.json`, and
      both `version` fields at the root of `frontend/package-lock.json` and its
      `packages[""]` entry.
- [ ] Maintainers edited only the three version manifests, then ran
      `npm --prefix frontend install --package-lock-only` immediately after the
      frontend edit and one unlocked Cargo gate to regenerate the tracked lockfiles.
      Every final Cargo test, Clippy, and Tauri build gate enforced `--locked`; the
      release commit contains exactly the three manifests, `backend/Cargo.lock`,
      and `frontend/package-lock.json` (five reviewed files total).
- [ ] The matching release tag was created once for the version commit; no existing
      tag was moved, deleted, or reused.
- [ ] An active GitHub tag ruleset covers the release-tag namespace and blocks tag
      updates and deletions without granting the release workflow a bypass.
- [ ] `npm run test:release` passes for the release tag.
- [ ] Frontend tests pass.
- [ ] Frontend production build passes.
- [ ] Rust formatting, tests, and Clippy pass.
- [ ] The MSI bundle completes for `x86_64-pc-windows-msvc`.
- [ ] Every action in `release-windows.yml` uses the reviewed immutable commit pin.
- [ ] The read-only build job fetched `origin/main` and proved the tag commit is an
      ancestor before dependency setup or build execution.
- [ ] The build job had only `contents: read`, the separate release job had only
      `contents: write`, and the release job did not check out or execute repository
      code.
- [ ] The immutable-pinned official artifact actions transferred exactly the MSI
      and its matching `.sha256`; the download action validated the workflow
      artifact digest, and the release step revalidated file names, sizes, SHA-256
      values, checksum content, and digest metadata format.
- [ ] The tagged `release-windows.yml` run completed successfully.
- [ ] The release job resolved the exact live `refs/tags/<tag>` reference through
      the commits API (never a same-named branch) and matched the lightweight or
      annotated tag's peeled commit to the triggering workflow SHA both immediately
      before mutation and after final asset/metadata verification.
- [ ] The GitHub Release remains a draft during review and smoke testing.
- [ ] The draft contains exactly one `.msi` asset and its one matching
      `.msi.sha256` asset, with no NSIS installer or plain executable.
- [ ] The downloaded MSI SHA-256 matches the first field in its downloaded
      `.msi.sha256` file. Use the reproducible download-and-compare command in
      [Create a Windows release](../README.md#create-a-windows-release).
- [ ] The draft title, tag, and generated notes identify the intended version.
- [ ] The draft clearly states that the private-test MSI is unsigned and may show
      an Unknown publisher or SmartScreen warning, when code signing is not enabled.

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

## Publish decision

- [ ] Every automated, clean-machine, upgrade, and uninstall gate above passes.
- [ ] The release was kept as a draft until all checks passed.
- [ ] Publish the draft only after the responsible maintainer has recorded the
      evidence. If a gate fails, leave the draft unpublished, fix the source in a
      new commit, and create a new immutable patch-version tag.
