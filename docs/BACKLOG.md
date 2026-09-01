# Dashy Backlog

## Future product ideas

- **Tasks as an additional side-notch metric:** Consider a tasks tile that opens a dedicated tasks card alongside the provider tiles.
- **Signed Windows releases:** Sign the Dashy executable and MSI in CI before public distribution so Windows can verify the publisher and build reputation.
- **Automatic updates:** Add a signed in-app update path after the release and signing process is stable.
- **Windows ARM64:** Add and verify a native ARM64 installer when demand justifies a second Windows artifact.
- **macOS and Linux packages:** Preserve platform boundaries while the installer remains Windows-only, then design native distribution and provider setup for each operating system.
- **Grok stdio billing follow-up:** Grok Build 1.0.x rejects `x.ai/billing` over `agent stdio` while signed out (`-32601`); Dashy degrades to an unavailable tile if a signed-in build behaves the same. Re-verify against newer Grok releases and pin the live handshake once billing is confirmed over stdio.

## Explicitly skipped

- **Gemini provider:** The Gemini CLI exposes no headless quota or usage query — its per-model "usage left" view is interactive-only, and the only programmatic path requires reading the CLI's OAuth credential file and calling Google's private Cloud Code endpoints directly, which violates Dashy's no-credential-access and no-network principles. Google also deprecated consumer "Login with Google" access for the Gemini CLI in June 2026. Revisit only if the CLI gains a supported non-interactive usage command.
