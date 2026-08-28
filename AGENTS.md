# Dashy architecture

Dashy is a compact, local-first desktop productivity dashboard.

- `frontend/` owns the React user interface and browser-side state.
- `backend/` owns the Tauri desktop shell and native capabilities.
- `agents/` documents future automation boundaries; no autonomous agents ship in the MVP.
- `infrastructure/` owns packaging and delivery configuration.

Keep data local by default, avoid provider scraping, and preserve Hebrew and English support.

