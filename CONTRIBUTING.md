# Contributing to Dashy

Thanks for helping. Dashy is small and opinionated, so a few rules keep it that
way. Read this once before opening an issue or a pull request.

## The rules that never bend

- **CLI-only.** Every signal comes from a locally installed, already
  authenticated provider CLI. Dashy never reads a credential file, never stores
  a token, and never makes a network request of its own. A change that needs
  any of those will not be merged, however useful it looks.
- **Bounded parsing.** Provider modules read only the fields the interface
  shows, with timeouts and output-size limits. Unknown output degrades to an
  *Unavailable* tile, never to a guess or a fabricated zero.
- **Every locale ships.** A new string lands in all eight locale files (`en`,
  `he`, `ar`, `es`, `ru`, `fr`, `zh-CN`, `ja`) in the same change. The
  localization test enforces the key set; RTL is verified in Hebrew.
- **Keep the design language.** The notch, cards, Settings, and onboarding
  share one look. Reuse the existing classes and tokens before adding new ones,
  and include a screenshot in any pull request that changes what a user sees.

## Set up a development machine

Windows is the release target. Open PowerShell in the repository and let the
bootstrap script check what is missing, then install it:

```powershell
.\install.ps1 -CheckOnly
.\install.ps1
```

Sign in to only the provider CLIs you need (`gh auth login`, `claude auth
login`, `codex login`, `grok login`, `cursor-agent login`). The script never
signs in for you and Dashy never asks for a token.

## Run it

Browser preview of the notch with deterministic fixture data (no providers,
no native edge behavior):

```powershell
Set-Location frontend
npm run dev
```

Then open `http://localhost:5173/?fixture=1&placement=right&provider=claude&background=bright`.

The desktop application:

```powershell
Set-Location backend
cargo tauri dev --no-watch
```

## Before you push

The same gates run in CI on every pull request (`.github/workflows/verify.yml`);
run them locally first:

```powershell
Set-Location frontend
npx tsc --noEmit -p .
npm test
npm run build

Set-Location ..
npm run test:release

Set-Location backend
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

If you touched the app icon, edit `backend/icons/icon.svg` and run
`npm run icons:build`; CI fails when `icon.ico` no longer matches the SVG.

## Pull requests

- One topic per pull request. Small, reviewable, with a description that says
  what changed and why.
- Tests come with the change: a parser fix brings the real CLI output that
  broke it as a fixture; a UI change updates the component tests.
- CI must be green. There is no branch protection, so a red check is a
  signal to fix, not a formality to bypass.
- Screenshots for anything visible. The visual fixture URL above renders the
  notch; use it so reviewers see exactly what you saw.
- Commit messages explain the reasoning, not just the diff.

## Reporting problems

Use the issue templates. For a provider that shows *Unavailable*, attach the
local diagnostics log (Settings → Diagnostics → **Open log folder** →
`dashy.log`). It records only timestamps, provider names, outcomes, and
durations, so it is safe to share.

Security problems go through the [security policy](SECURITY.md), not a public
issue.

## Releases

Maintainers cut releases with the guarded procedure in
[docs/RELEASE.md](docs/RELEASE.md). Contributors do not need to touch version
numbers.
