# Dashy Side-Notch UI Design

**Date:** 2026-08-29  
**Status:** Proposed for user review  
**Project:** Dashy desktop application  
**Reference interaction:** https://x.com/hivinz_/status/2093651446927626294

## 1. Summary

Dashy will replace its conventional dashboard window with an ambient, edge-mounted usage monitor inspired by the supplied side-notch reference. The application stays completely outside the visible work area while idle. When the pointer approaches its configured screen edge, a compact glass rail appears. Hovering a metric opens its detailed card toward the center of the screen.

The first release contains three metrics only:

- Claude usage
- Codex usage
- GitHub contribution streak

The existing tasks interface is removed from the active product. A possible optional fourth tasks metric remains in the backlog.

## 2. Product Goals

- Make AI usage and GitHub activity available at a glance without occupying permanent screen space.
- Match the reference interaction: proximity reveal, provider selection by hover, smooth card switching, and automatic dismissal.
- Preserve the real authenticated provider integrations already implemented in Dashy.
- Feel native to Windows through acrylic glass, system tray integration, monitor-aware positioning, and fullscreen awareness.
- Support right, left, and top screen placement from the first release.
- Support English, Hebrew, Arabic, Spanish, Russian, French, Simplified Chinese, and Japanese from the first release.

## 3. Non-Goals

- Reintroducing tasks in the first side-notch release.
- Supporting bottom-edge placement.
- Displaying model-specific or preview-only usage limits.
- Adding new external account providers.
- Reproducing macOS menu-bar behavior or macOS-only materials.
- Installing or updating provider CLIs from inside the notch UI.

## 4. Experience Model

Dashy has four interaction states.

### 4.1 Hidden

- The Dashy window is completely outside the selected monitor's usable work area.
- No visible handle remains on screen.
- A native Windows proximity controller continues observing the pointer position.
- Dashy does not reserve desktop work area and does not block edge clicks or scrollbars.

### 4.2 Rail revealed

- Entering a 28 px activation zone along the configured edge starts the reveal.
- The rail slides into the selected monitor's usable work area.
- Right and left placement show three vertically stacked metrics.
- Top placement shows the same three metrics horizontally.
- Each metric contains a monochrome provider glyph, a colored progress ring, and one compact value.

### 4.3 Provider card expanded

- Hovering or focusing a rail metric opens its provider card toward the center of the screen.
- Moving directly between metrics replaces the card content without dismissing the rail.
- The card and rail form one continuous safe hover region.
- A short pointer path between rail and card must not close the surface.

### 4.4 Pinned

- Clicking the currently selected metric pins its card open and requests a refresh for that provider.
- The selected progress ring shows one restrained refresh rotation.
- Clicking the selected metric again, pressing Escape, or clicking outside unpins it.
- Keyboard focus remains inside the visible Dashy surface while pinned and returns to the selected metric when unpinned.

## 5. Timing and Motion

- Proximity sampling target: every 40 ms while Dashy is running.
- Reveal dwell: 100 ms inside the activation zone to reject accidental passes.
- Rail enter animation: 220 ms, ease-out.
- Card enter or provider-switch animation: 180 ms, ease-out with opacity and short directional translation.
- Close grace period: 420 ms after the pointer leaves the combined rail/card safe region.
- Exit animation: 180 ms, ease-in.
- No scale-based hover effect that changes layout.
- `prefers-reduced-motion` removes translation and rotation, reduces fades to near-instant state changes, and retains all functionality.

The visual signature is the edge shape itself. Motion remains limited to the reveal, provider-card transition, and explicit refresh feedback.

## 6. Placement and Monitor Behavior

### 6.1 Supported placements

- **Right:** rail at the right edge; detail cards open left.
- **Left:** rail at the left edge; detail cards open right.
- **Top:** rail at the top edge; detail cards open downward.

The notch silhouette, pointer wedge, rounded corners, layout axis, and animation direction rotate with placement. Content reading direction is independent from placement.

### 6.2 Monitor selection

- The user selects a monitor in Settings.
- The primary monitor is the default.
- The selection is stored using the most stable monitor identifier exposed by the platform, with name and geometry as recovery metadata.
- If the stored monitor is unavailable, Dashy falls back to the primary monitor without discarding the saved preference.
- Positioning uses the monitor's usable work area so it does not overlap the Windows taskbar.
- Display topology and DPI changes trigger safe repositioning.

### 6.3 Fullscreen behavior

- Dashy hides automatically when another application occupies the selected monitor in fullscreen mode.
- Settings include an `Always show over fullscreen apps` override, disabled by default.
- Tray access remains available while the notch is suppressed.

## 7. Visual Direction

The geometry and behavior follow the reference, but the material is dark Windows acrylic rather than opaque black.

### 7.1 Core tokens

- Deep glass: `rgba(7, 10, 12, 0.82)`
- Raised glass: `rgba(13, 18, 21, 0.76)`
- Glass border: `rgba(255, 255, 255, 0.12)`
- Inner highlight: `rgba(255, 255, 255, 0.08)`
- Primary text: `#F4F7F5`
- Secondary text: `#98A39D`
- Track: `rgba(255, 255, 255, 0.12)`
- Claude accent: `#FF7548`
- Codex accent: `#62E6A7`
- GitHub accent: `#8DEB78`
- Warning: `#FFC56E`
- Error: `#FF7474`

The material uses native acrylic behind a semi-transparent CSS surface. Text contrast must remain readable over both bright and dark wallpapers. Provider color is an accent and never the only carrier of status.

### 7.2 Typography

- UI and data: `Segoe UI Variable`, then `Segoe UI`, then system sans-serif.
- Percentages, streak values, and reset times use tabular numerals.
- Translated labels may wrap to two lines inside detail cards but never inside the compact rail.
- Compact values remain visually stable when changing from one to three digits.

### 7.3 Approximate geometry

- Side rail: 70 px wide, sized vertically to its three metric rows and edge curves.
- Top rail: 250-280 px wide and approximately 70 px high.
- Compact metric hit target: at least 44 x 44 px.
- Detail card: approximately 300 px wide; height is content-driven within defined provider limits.
- Rail/card gap is visually closed by a notch pointer wedge so the pieces read as one object.

Final pixel values may be tuned through screenshot comparison, but the compact proportions and visual hierarchy are binding.

## 8. Provider Presentation

### 8.1 Compact rail

Provider glyphs appear only inside the compact rings. Expanded cards use names without an additional logo.

- **Claude:** ring shows the lower remaining percentage across its supported general limits.
- **Codex:** ring shows the lower remaining percentage across its supported general limits.
- **GitHub:** ring centers the current streak in days, such as `12d`; ring color/intensity represents today's contribution level.

The compact view must not show fabricated zeroes. Missing data uses a neutral ring and a concise translated status.

### 8.2 Claude card

- Provider name
- Short-window/session remaining percentage and progress bar
- Short-window reset time
- Weekly/all-models remaining percentage and progress bar
- Weekly reset time
- Last successful refresh when stale
- Actionable translated states for not installed, sign-in required, unavailable, or stale

### 8.3 Codex card

- Provider name
- Short general-window remaining percentage and progress bar
- Short-window reset time
- Weekly general-window remaining percentage and progress bar
- Weekly reset time
- Last successful refresh when stale
- Actionable translated states for not installed, sign-in required, unavailable, or stale

Model-specific, preview, special-program, or unrelated rate-limit buckets remain excluded.

### 8.4 GitHub card

- Current streak in days
- Today's contribution count and level
- Twelve-week contribution heatmap aligned to calendar weekdays
- Last successful refresh when stale
- Actionable translated states for CLI not installed, sign-in required, unavailable, or stale

GitHub does not display a fabricated usage percentage.

## 9. Data Contract Changes

The existing provider integration remains the source of truth, but the usage contract expands so the UI does not lose the separate general limits.

Each Claude and Codex usage snapshot exposes:

- provider status
- summary remaining percentage, calculated as the minimum valid remaining percentage
- optional short-window usage object
- optional weekly-window usage object
- each window's label key, remaining percentage, reset timestamp, and optional provider-safe reset label
- last successful refresh timestamp

The backend continues to clamp percentages to 0-100, reject malformed or unrecognized output, exclude provider-specific private fields, and serialize no account identity.

GitHub retains its contribution-day records and current streak. Today's contribution count and level are derived from the local-date calendar record rather than a percentage.

Click refresh becomes provider-scoped. It refreshes only the selected provider, retains the other two cached values, coalesces duplicate requests for that provider, and keeps stale fallback behavior.

## 10. Localization

### 10.1 Initial languages

- English (`en`) — default
- Hebrew (`he`)
- Arabic (`ar`)
- Spanish (`es`)
- Russian (`ru`)
- French (`fr`)
- Simplified Chinese (`zh-CN`)
- Japanese (`ja`)

### 10.2 Direction and formatting

- Hebrew and Arabic set the content direction to RTL.
- English, Spanish, Russian, French, Chinese, and Japanese use LTR.
- Rail placement never changes solely because of language direction.
- Internal card alignment, label/value ordering, chevrons, and pointer-aware spacing mirror for RTL.
- Percentages, contribution counts, and streaks use locale-aware number formatting.
- Reset times use locale-aware date/time formatting and the machine's current time zone.
- Provider names remain their recognized product names.

### 10.3 Translation architecture

- All visible copy uses stable translation keys; no UI string is embedded directly in provider components.
- Translation resources are split by locale and validated against the English key set.
- Missing keys fall back to English and are detected in tests.
- Settings allow explicit language selection. English is the first-run default.
- The architecture permits adding locales without changing provider or window-control code.

## 11. Settings and System Tray

Dashy remains a background utility rather than a conventional window.

### 11.1 Tray menu

- Show Dashy
- Refresh all providers
- Placement: Right, Left, Top
- Monitor selection
- Settings
- Quit Dashy

### 11.2 Notch context menu

Right-clicking the visible notch opens the same essential controls in a compact menu.

### 11.3 Settings

- Placement
- Monitor
- Language
- Hide in fullscreen / always show override
- Launch at startup
- Optional manual refresh
- Provider connection status with setup guidance

`Launch at startup` is available but disabled by default. Settings persist locally and contain no provider credentials.

## 12. Native Architecture

### 12.1 Edge proximity controller

A Rust-side controller owns pointer proximity and window state. It reads global cursor position without installing a browser-sized invisible hit surface. Its state machine is:

`Suppressed | Hidden | RailVisible | CardVisible | Pinned`

Inputs include pointer position, selected edge, selected monitor work area, fullscreen suppression, React hover/focus state, pin state, and close timers.

### 12.2 Window controller

- Keeps the transparent, undecorated Dashy window out of the work area while hidden.
- Positions and sizes the window for the selected orientation and current visible state.
- Applies always-on-top only while Dashy is eligible to appear.
- Repositions on monitor, work-area, taskbar, and DPI changes.
- Prevents focus theft during proximity-only reveal; focus is requested only after explicit click or keyboard activation.

### 12.3 React surface

- Renders orientation-specific rail geometry from a shared provider model.
- Owns visual transitions, selected provider, hover/focus safe region, and translated content.
- Sends semantic state events to Rust rather than raw pointer coordinates.
- Keeps provider cards independent so each can be tested without the native window controller.

### 12.4 Supporting desktop integrations

- System tray menu
- Persisted settings store
- Windows startup registration, opt-in only
- Fullscreen foreground-window detection
- Multi-monitor enumeration and work-area positioning

## 13. Error, Loading, and Stale States

- Dashy never renders a numeric zero when provider data is unavailable.
- Initial loading uses a neutral ring with a subtle, reduced-motion-safe activity indication.
- `Not installed` and `Sign in required` cards include translated setup guidance.
- A failed refresh retains the last successful value, marks the card stale, and displays the last successful refresh time.
- Provider failures are isolated; one failed provider does not prevent the rail or other cards from appearing.
- Error details remain sanitized and never show raw CLI output, account names, emails, organizations, or tokens.

## 14. Accessibility and Input

- Every metric is reachable by keyboard when Dashy is explicitly opened through the tray or shortcut.
- Arrow keys move between rail metrics according to orientation.
- Enter or Space pins/refreshes the selected provider.
- Escape closes the card and then hides the rail.
- Visible focus treatment is distinct from provider progress color.
- Dynamic refresh status uses a polite live region.
- Progress values expose accessible names and values.
- Hover is not the only supported interaction; tray, keyboard, focus, and click provide equivalent access.
- Motion respects `prefers-reduced-motion`.
- Compact hit targets are at least 44 x 44 px.

## 15. Testing Strategy

### 15.1 Rust tests

- Edge activation for right, left, and top placements
- Reveal dwell and close grace timing
- Safe-region transitions between rail and card
- Pin and unpin state transitions
- Monitor fallback and DPI/work-area repositioning
- Fullscreen suppression and override
- Provider-scoped refresh coalescing and stale fallback
- Settings persistence without credentials

### 15.2 React tests

- Hidden, rail, expanded, and pinned render states
- Provider switching without rail dismissal
- Claude and Codex dual-window rendering
- GitHub streak, today count, and weekday-aligned heatmap
- Loading, unavailable, not-installed, not-authenticated, and stale states
- Right, left, and top orientation classes and geometry
- Keyboard navigation and focus restoration
- Reduced-motion behavior
- Complete translation-key parity across all eight locales
- RTL behavior for Hebrew and Arabic

### 15.3 End-to-end verification

- Visual screenshot checks for all three placements over bright and dark backgrounds
- Proximity reveal from outside the work area
- Pointer travel between rail and every provider card without flicker
- Multi-monitor selection and fallback
- Automatic fullscreen suppression
- Tray controls, language switching, startup toggle, and clean quit
- Live read-only GitHub, Codex, and Claude data rendering without account-detail disclosure

## 16. Acceptance Criteria

- Dashy is completely invisible while idle and reveals from the configured edge when the pointer enters the proximity zone.
- The rail and provider card match the reference interaction without blocking normal edge clicks while hidden.
- Right, left, and top placements work on the selected monitor.
- Claude and Codex each show separate short and weekly general limits, with the compact ring showing the lower remaining value.
- GitHub shows streak days in the compact rail and real contribution data in its card.
- Dark acrylic glass remains readable over varied desktop backgrounds.
- Fullscreen applications suppress Dashy by default.
- Tray and notch context menus expose the approved controls.
- Launch at startup is opt-in and disabled by default.
- All eight approved languages are available; Hebrew and Arabic render correctly in RTL.
- No tasks UI is present in the first release.
- Existing privacy, timeout, caching, and provider-isolation guarantees remain intact.

## 17. Backlog

- Optional fourth rail metric that opens a dedicated tasks card.
- Additional languages based on user demand.
- Bottom-edge placement only if future demand justifies the additional geometry.

