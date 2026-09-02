# Security Policy

## Supported versions

Only the latest release on the
[Releases page](https://github.com/adirangel/Dashy/releases/latest) receives
fixes. Update before reporting.

## Reporting a vulnerability

Please do not open a public issue for a security problem.

Report it privately through GitHub's vulnerability reporting for this
repository:

**https://github.com/adirangel/Dashy/security/advisories/new**

Include what you found, how to reproduce it, and which release you tested. You
will get an acknowledgement within a few days and a fix, or an explanation, as
soon as one is ready. Credit is given in the advisory unless you prefer
otherwise.

## What counts

Dashy makes specific promises; anything that breaks one of them is in scope:

- It never reads provider credential files, tokens, cookies, or password
  managers.
- It makes no network requests of its own; every signal comes from a local,
  already authenticated CLI.
- It runs only allowlisted executables, resolved from fixed locations, with
  bounded timeouts and output limits.
- It shows the user nothing from a CLI beyond the reviewed status and usage
  fields, and its local diagnostics log holds no CLI output or identity.
- Its windows expose only the Tauri commands their capability files allow.

Problems in the provider CLIs themselves (Claude Code, Codex, GitHub CLI, Grok
Build, Cursor) belong to their vendors.

## Unsigned builds

Current releases are not code-signed or notarized, so Windows may show a
SmartScreen or Unknown publisher warning and macOS may refuse to open the app
until it is allowed under Privacy & Security. Verify a download against the
`.sha256` file published with every release before installing.
