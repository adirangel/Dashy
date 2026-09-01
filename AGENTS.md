# Dashy architecture

Dashy is a compact, local-first desktop productivity dashboard.

- `frontend/` owns the React user interface and browser-side state.
- `backend/` owns the Tauri desktop shell and native capabilities.
- `docs/` contains current product documentation and the active backlog; completed implementation plans are not retained.
- `infrastructure/` owns the release tooling: version verification and the tests that pin the release workflow's shape.

Windows is the current release target. Preserve the existing desktop portability boundaries for future macOS and Linux support.

Keep data local by default, avoid provider scraping, never read provider credential files, make no application-originated network requests, and preserve every supported locale.
