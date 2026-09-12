# Working on Dashy

Dashy is a local-first desktop dashboard built with React, TypeScript and Tauri 2.
Windows, macOS and Linux are release targets.

## Source and ownership

- Work in the source checkout (it contains `package.json`, `frontend/` and `backend/`). An installed folder containing only `dashy.exe` is not the source.
- `frontend/`: React UI, browser state, eight locales and component tests. Read `frontend/AGENTS.md` before editing.
- `backend/`: Tauri commands, provider processes, settings and desktop control. Read `backend/AGENTS.md` before editing.
- `infrastructure/`: release/version checks, Windows installer regression tests, asset staging and icon generation.
- `docs/`: product documentation, release procedures and the active backlog. Do not retain completed implementation plans.

## Product boundaries

- All provider data comes from locally installed, authenticated CLIs. Never read provider credential files, store tokens, scrape providers or add application-originated network requests.
- Keep subprocess timeouts and output bounds; unknown output must remain unavailable, not a fabricated zero.
- Install/login actions require the existing explicit provider action. Dashy's MSI installs the app; `install.ps1` and `install.sh` are contributor bootstrap scripts, not end-user setup.
- After a Windows provider install, verify the command using persisted PATH values. Preserve existing user PATH entries and expandable variables; never persist the inherited process PATH or overwrite the machine PATH.
- Preserve all eight locales and Hebrew/Arabic RTL. Reuse existing UI classes and tokens.

## Platform boundaries

- Shared edge state, controller and providers stay platform-neutral. Native code belongs behind `backend/src/desktop/platform.rs` and its Windows/macOS/Linux implementations; Unix CLI discovery lives in `backend/src/dashboard/unix.rs`.
- Missing monitor/cursor/fullscreen information must be recoverable.
- The Windows notch starts hidden. Apply host bounds and WebView visibility on the owning UI thread, without taking focus on a hover reveal. Test cold startup, repeated reveals and transient bootstrap failures.
- `backend/icons/icon.svg` is the icon source; run `npm run icons:build` to regenerate platform assets.

## Validation

Run checks appropriate to the change; the full CI gate is:

```sh
cd frontend
npx tsc --noEmit -p .
cd ..
npm test
npm run build
npm run test:release
cargo fmt --manifest-path backend/Cargo.toml --check
cargo clippy --manifest-path backend/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path backend/Cargo.toml --locked
```

`test:release` includes the Windows PATH tests and runs on Windows. Keep tests isolated from real provider accounts, installers and the user's registry.
For Windows startup/setup changes, also use `docs/WINDOWS_RELEASE_CHECKLIST.md`; automated tests do not prove a cold logon or a clean-machine installation.
See `CONTRIBUTING.md` for development and `docs/RELEASE.md` for releases. Do not bump versions or publish a release as part of a routine fix.
