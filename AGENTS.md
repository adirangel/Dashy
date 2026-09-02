# Dashy architecture

Dashy is a compact, local-first desktop productivity dashboard.

- `frontend/` owns the React user interface and browser-side state.
- `backend/` owns the Tauri desktop shell and native capabilities.
- `docs/` contains current product documentation and the active backlog; completed implementation plans are not retained.
- `infrastructure/` owns the build tooling: release version verification, the tests that pin the release workflows' shape, the release asset staging script, and the app-icon generator (`backend/icons/icon.svg` is the source; `npm run icons:build` regenerates `icon.ico`, `icon.icns`, and the PNG icons).

Windows, macOS, and Linux are release targets. Platform-specific code lives behind `backend/src/desktop/platform.rs` (Win32 in `windows.rs`, CoreGraphics in `macos.rs`, GDK in `linux.rs`) and `backend/src/dashboard/unix.rs`; the edge machine, controller, providers, and UI stay platform-neutral. A platform backend degrades gracefully (no cursor, not fullscreen) instead of failing the controller.

Keep data local by default, avoid provider scraping, never read provider credential files, make no application-originated network requests, and preserve every supported locale.
