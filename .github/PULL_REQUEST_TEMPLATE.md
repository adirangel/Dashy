## What

<!-- What changed and why. One topic per pull request. -->

## Checklist

- [ ] Frontend: `npx tsc --noEmit -p .`, `npm test`, `npm run build` pass.
- [ ] Backend: `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, `cargo test --locked` pass.
- [ ] New strings exist in all eight locale files.
- [ ] Tests cover the change (a parser fix includes the real CLI output as a fixture).
- [ ] The change keeps the CLI-only contract: no credential files, no application-originated network requests.
- [ ] UI change: screenshot attached (fixture URL in CONTRIBUTING.md).
