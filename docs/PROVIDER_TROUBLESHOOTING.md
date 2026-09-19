# Provider startup and installation troubleshooting

Dashy reads provider data through the locally installed CLI. An installed app,
a CLI command that can start, and an authenticated subscription are separate
requirements. Missing data must never be displayed as zero usage.

## What the September 2026 logs establish

The supplied logs contained successful Claude refreshes followed by near-immediate
failures for Claude and Codex. The old runner mapped local process/pipe errors to
`provider network request failed`, so those lines do not establish a network
outage or identify the Windows error. GitHub, Grok and Cursor were reported as
missing executables; the logs do not establish that their subscriptions failed.
The old logs did not record install/login failure stages or exit codes.

## Windows recovery

- CLI discovery refreshes the persisted machine and user PATH before consulting
  the inherited startup PATH, and also checks known official installer homes.
- Codex portable WinGet installations can have only a target-qualified executable
  with no `codex` alias. Dashy can launch that payload directly. An explicit
  Install action creates a relative `codex.cmd` next to the payload when no command
  exists, preserves saved PATH entries and verifies discovery in a fresh shell.
  Existing commands are never overwritten. A protected package directory can
  still require repair through the official installer with appropriate permissions.
- WinGet's no-applicable-update exit code proceeds to command repair and
  verification. Other installer failures remain failures.
- Successful installation requires a bounded `--version` probe, not just the
  existence of an executable or wrapper. Installation does not sign the user in.
- Native, nested npm, hoisted npm and legacy Codex payloads are recognized.
  Current Claude npm native payloads and legacy Node-based installs are both
  supported, including nested and hoisted optional dependencies.
- Children use a stable user directory and native Windows executable separators.
  A CLI that cannot start offers the existing explicit installation action.

## Usage response compatibility

Codex account, billing and model metadata can change without changing the usage
windows. Dashy ignores metadata it does not display and still validates the
selected general bucket, window duration, percentage and reset timestamp. A
weekly-only bucket remains valid; a missing window is never invented.

Claude can return a successful JSON result containing only a subscription
confirmation. Dashy reports this as authenticated but without usage limits,
separately from unsupported output. It does not turn that response into 0% used
or claim that another login will fix it. Check `/usage` in Claude Code and retry
Dashy's refresh when the CLI supplies the window values.

A local comparison with Claude 2.1.257 and a temporary official 2.1.278 binary
returned this confirmation without limits, both with and without safe mode.
The latter's diagnostic log reported a fieldless/non-object usage response.
That observation does not establish the cause on another computer. An update
alone did not restore local usage data, and end-to-end Claude validation remains
open until the CLI returns actual windows. No provider credentials or private
endpoints are accessed by Dashy to work around this failure.

## Reading new diagnostics

Startup failures retain the numeric OS code. Local pipe/protocol failures are
process failures, not evidence of a network problem. Setup failures identify
`install`, `command verification`, or `login`, with a numeric exit/OS code where
available. Diagnostics never include CLI output, arguments, executable paths,
account identity or credentials.

Share both `dashy.log` and `dashy.log.1` from Settings > Diagnostics, the Dashy
version, and the visible installer message. Historical generic network errors
cannot be retrospectively converted into a specific Windows error code.

## Validation boundaries

Regression tests use temporary executable layouts, fake runners and an isolated
PowerShell launcher. They never install a provider or change the user's registry.
A development-machine `--version` check proves local launch only. Complete the
clean-machine, first-login and upgrade checks in [the Windows release checklist](WINDOWS_RELEASE_CHECKLIST.md)
before treating the fix as verified on the affected computer.

References: [WinGet Codex portable manifest](https://github.com/microsoft/winget-pkgs/blob/master/manifests/o/OpenAI/Codex/0.100.0/OpenAI.Codex.installer.yaml),
[WinGet return codes](https://github.com/microsoft/winget-cli/blob/master/doc/windows/package-manager/winget/returnCodes.md).

An opt-in local check is available with
`cargo run --manifest-path backend/Cargo.toml --example provider_smoke --locked`.
It uses the same bounded runners as the app, requires existing CLI logins, and
prints only provider outcomes and durations. It does not install or sign in to
providers and does not print account identity, raw responses or credentials.
