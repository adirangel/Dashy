# Windows Release Checklist

Use this checklist for every private Windows x64 MSI candidate before publishing
or promoting a GitHub Release. Record the results with the release evidence; do
not treat a developer workstation as a clean-machine result.

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
