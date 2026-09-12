# Frontend rules

- Use React function components and strict TypeScript.
- Keep the dashboard usable from 320px wide and preserve RTL support.
- Prefer semantic HTML, accessible labels, and CSS custom properties.
- Keep deterministic sample data confined to development fixtures and tests; production data comes from typed Tauri commands.
- Startup settings/view queries must recover from transient failure. Preserve event revisions so an older query cannot overwrite a newer event, and cancel retries on unmount.

