# Modular Provider Onboarding UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a localized first-run onboarding window and reusable Settings provider manager that let users install, connect, enable, disable, or skip Claude, Codex, and GitHub independently.

**Architecture:** Route a third Tauri window label to an onboarding React surface, reuse one provider manager in onboarding and Settings, and keep process execution behind the typed APIs from the provider-backend plan. The main notch reads the persisted enabled-provider list, renders only those providers, and computes rail/join geometry from the active count.

**Tech Stack:** React, strict TypeScript, Vitest, Testing Library, i18next, CSS, Tauri 2 window/event APIs

**Spec:** `docs/WINDOWS_INSTALLER_DESIGN.md`

## Global Constraints

- This plan starts only after `2026-08-30-provider-setup-backend.md` is complete.
- Every installation and login action has a separate, explicit confirmation.
- The exact command and publisher are visible before installation starts.
- Raw native errors and command output are never rendered.
- Users can finish with any provider subset, including an empty subset.
- Disabled providers do not appear in the metric rail.
- English, Hebrew, Arabic, Spanish, Russian, French, Simplified Chinese, and Japanese must share the exact same message-key contract.
- Hebrew and Arabic remain RTL; every other supported locale remains LTR.
- The onboarding surface must be keyboard operable, work at 520px width, expose live status changes, and honor reduced motion.
- Reuse Dashy's existing glass palette and Segoe UI Variable typography. The distinctive visual device is a compact three-stop setup rail: each provider card uses its real accent color as a progress/status spine, while the rest of the surface stays quiet.

## File Structure

- `frontend/src/setup/api.ts`: strict TypeScript IPC types, validation, and native invocations.
- `frontend/src/setup/useProviderSetup.ts`: asynchronous provider state and per-provider busy/error state.
- `frontend/src/setup/ProviderManager.tsx`: reusable cards, enable controls, and inline consent confirmation.
- `frontend/src/setup/ProviderManager.test.tsx`: consent, combinations, failure sanitization, and accessibility tests.
- `frontend/src/onboarding/OnboardingApp.tsx`: first-run selection and completion flow.
- `frontend/src/onboarding/OnboardingApp.test.tsx`: first-run detection, selection, completion, and localization tests.
- `frontend/src/onboarding.css`: onboarding/provider-manager visual system.
- `frontend/src/settings/SettingsApp.tsx`: embeds the provider manager for later changes.
- `frontend/src/notch/NotchApp.tsx`: enabled-provider state and dynamic selection.
- `frontend/src/notch/MetricRail.tsx`: renders a supplied provider list rather than a global constant.
- `frontend/src/window.ts`: settings-change listener and onboarding completion boundary.
- `backend/tauri.conf.json`: hidden onboarding window definition.
- `backend/capabilities/default.json`: minimal onboarding window capability.
- `backend/src/lib.rs`: show onboarding on first run.
- `backend/src/desktop/mod.rs`: show/hide onboarding helpers.
- `backend/src/desktop/edge.rs`: rail size derived from enabled-provider count.
- `backend/src/desktop/controller.rs`: supplies enabled-provider count to native geometry.

---

### Task 1: Add the strict frontend provider-setup boundary

**Files:**
- Create: `frontend/src/setup/api.ts`
- Create: `frontend/src/setup/api.test.ts`
- Modify: `frontend/src/window.ts`
- Modify: `frontend/src/window.test.ts`

**Interfaces:**
- Consumes Tauri commands: `get_provider_setup_states`, `install_provider`, `login_provider`, `complete_onboarding`
- Produces: `ProviderSetupDefinition`, `ProviderSetupState`, `ProviderSetupAction`
- Produces: `getProviderSetupStates()`, `installProvider(provider)`, `loginProvider(provider)`, `completeOnboarding(providers)`
- Produces: `listenForSettingsChanges(handler)`

- [ ] **Step 1: Write failing strict-boundary tests**

Create `frontend/src/setup/api.test.ts`:

```typescript
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import { getProviderSetupStates, installProvider, loginProvider } from "./api";

const codex = {
  definition: {
    provider: "codex", publisher: "OpenAI", packageId: "OpenAI.Codex",
    installCommand: "winget install --id OpenAI.Codex --exact --source winget --interactive --accept-source-agreements --accept-package-agreements",
    installUrl: "https://learn.chatgpt.com/docs/codex/cli",
    loginCommand: "codex login",
  },
  status: "notInstalled",
};

describe("provider setup IPC", () => {
  beforeEach(() => mocks.invoke.mockReset());

  it("accepts the exact setup contract", async () => {
    mocks.invoke.mockResolvedValue([codex]);
    await expect(getProviderSetupStates()).resolves.toEqual([codex]);
  });

  it("rejects extra native fields before they reach the UI", async () => {
    mocks.invoke.mockResolvedValue([{ ...codex, token: "secret" }]);
    await expect(getProviderSetupStates()).rejects.toThrow(/invalid provider setup response/i);
  });

  it("sends only the provider enum for native actions", async () => {
    mocks.invoke.mockResolvedValue({ ...codex, status: "connected" });
    await installProvider("codex");
    await loginProvider("codex");
    expect(mocks.invoke.mock.calls).toEqual([
      ["install_provider", { request: { provider: "codex" } }],
      ["login_provider", { request: { provider: "codex" } }],
    ]);
  });
});
```

- [ ] **Step 2: Run the test and confirm the module is missing**

Run:

```powershell
npm --prefix frontend run test -- src/setup/api.test.ts
```

Expected: FAIL because `src/setup/api.ts` does not exist.

- [ ] **Step 3: Implement validated setup APIs**

Create `frontend/src/setup/api.ts` with these public types:

```typescript
export type ProviderSetupDefinition = {
  provider: ProviderId;
  publisher: string;
  packageId: string;
  installCommand: string;
  installUrl: string;
  loginCommand: string;
};

export type ProviderSetupState = {
  definition: ProviderSetupDefinition;
  status: ProviderStatus;
};

export type ProviderSetupAction = "install" | "login";
```

Implement exact-key guards for the definition and state. Accept only the three known provider IDs and five known provider statuses. `getProviderSetupStates` validates the complete array; install/login validate the returned state. Export these invocations:

```typescript
export async function getProviderSetupStates(): Promise<ProviderSetupState[]>;
export async function installProvider(provider: ProviderId): Promise<ProviderSetupState>;
export async function loginProvider(provider: ProviderId): Promise<ProviderSetupState>;
```

In `frontend/src/window.ts`, add:

```typescript
const SETTINGS_CHANGED_EVENT = "dashy://settings-changed";

export const completeOnboarding = (enabledProviders: ProviderId[]) =>
  invoke<AppSettings>("complete_onboarding", { enabledProviders });

export async function listenForSettingsChanges(
  handler: (settings: AppSettings) => void,
): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => undefined;
  return listen<AppSettings>(SETTINGS_CHANGED_EVENT, ({ payload }) => handler(payload));
}
```

- [ ] **Step 4: Add the settings-event wire test**

Extend `frontend/src/window.test.ts` by mocking `@tauri-apps/api/event`. Capture the registered handler, emit a complete `AppSettings` payload, and assert that `listenForSettingsChanges` forwards it and returns the native unlisten function.

- [ ] **Step 5: Run boundary tests and build**

Run:

```powershell
npm --prefix frontend run test -- src/setup/api.test.ts src/window.test.ts
npm --prefix frontend run build
```

Expected: strict setup IPC tests, settings event tests, and TypeScript build pass.

- [ ] **Step 6: Commit the frontend boundary**

```powershell
git add frontend/src/setup/api.ts frontend/src/setup/api.test.ts frontend/src/window.ts frontend/src/window.test.ts
git commit -m "feat: add provider setup frontend boundary"
```

---

### Task 2: Build the reusable provider manager with inline consent

**Files:**
- Create: `frontend/src/setup/ProviderManager.tsx`
- Create: `frontend/src/setup/ProviderManager.test.tsx`
- Create: `frontend/src/setup/useProviderSetup.ts`

**Interfaces:**
- Consumes: setup APIs from Task 1
- Produces: `useProviderSetup(): ProviderSetupController`
- Produces: `ProviderManager` with `enabledProviders` and `onEnabledChange`

- [ ] **Step 1: Write failing user-flow tests**

Create `frontend/src/setup/ProviderManager.test.tsx` with a three-state fixture and cover these exact behaviors:

```typescript
const mocks = {
  install: vi.fn<(provider: ProviderId) => Promise<void>>().mockResolvedValue(undefined),
  login: vi.fn<(provider: ProviderId) => Promise<void>>().mockResolvedValue(undefined),
  reload: vi.fn<() => Promise<void>>().mockResolvedValue(undefined),
  onEnabledChange: vi.fn<(providers: ProviderId[]) => void>(),
};

const metadata: Record<ProviderId, Omit<ProviderSetupDefinition, "provider">> = {
  claude: { publisher: "Anthropic", packageId: "Anthropic.ClaudeCode", installCommand: "winget install --id Anthropic.ClaudeCode --exact --source winget --interactive --accept-source-agreements --accept-package-agreements", installUrl: "https://code.claude.com/docs/en/setup", loginCommand: "claude auth login --claudeai" },
  codex: { publisher: "OpenAI", packageId: "OpenAI.Codex", installCommand: "winget install --id OpenAI.Codex --exact --source winget --interactive --accept-source-agreements --accept-package-agreements", installUrl: "https://learn.chatgpt.com/docs/codex/cli", loginCommand: "codex login" },
  github: { publisher: "GitHub", packageId: "GitHub.cli", installCommand: "winget install --id GitHub.cli --exact --source winget --interactive --accept-source-agreements --accept-package-agreements", installUrl: "https://cli.github.com/", loginCommand: "gh auth login --web" },
};

function renderManager(options: {
  claudeStatus?: ProviderStatus;
  codexStatus?: ProviderStatus;
  githubStatus?: ProviderStatus;
  enabledProviders?: ProviderId[];
} = {}) {
  const status = {
    claude: options.claudeStatus ?? "connected",
    codex: options.codexStatus ?? "connected",
    github: options.githubStatus ?? "connected",
  } satisfies Record<ProviderId, ProviderStatus>;
  const states = (["claude", "codex", "github"] as ProviderId[]).map((provider) => ({
    definition: { provider, ...metadata[provider] }, status: status[provider],
  }));
  render(<ProviderManager
    controller={{ states, busyProvider: null, failureProvider: null, loadFailed: false, install: mocks.install, login: mocks.login, reload: mocks.reload }}
    enabledProviders={options.enabledProviders ?? ["claude", "codex", "github"]}
    onEnabledChange={mocks.onEnabledChange}
  />);
}

it("shows the publisher and exact command before installation is confirmed", async () => {
  renderManager({ codexStatus: "notInstalled" });
  fireEvent.click(screen.getByRole("button", { name: "Install Codex" }));
  expect(screen.getByText("OpenAI.Codex")).toBeInTheDocument();
  expect(screen.getByText(/winget install --id OpenAI\.Codex/)).toBeInTheDocument();
  expect(mocks.install).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Confirm installation" }));
  expect(mocks.install).toHaveBeenCalledExactlyOnceWith("codex");
});

it("requires a separate confirmation before login", async () => {
  renderManager({ claudeStatus: "notAuthenticated" });
  fireEvent.click(screen.getByRole("button", { name: "Connect Claude" }));
  expect(screen.getByText("claude auth login --claudeai")).toBeInTheDocument();
  expect(mocks.login).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Open official login" }));
  expect(mocks.login).toHaveBeenCalledExactlyOnceWith("claude");
});

it("lets every provider be enabled or skipped independently", async () => {
  renderManager({ enabledProviders: ["codex"] });
  expect(screen.getByRole("checkbox", { name: "Use Codex in Dashy" })).toBeChecked();
  expect(screen.getByRole("checkbox", { name: "Use Claude in Dashy" })).not.toBeChecked();
  expect(screen.getByRole("checkbox", { name: "Use GitHub in Dashy" })).not.toBeChecked();
  fireEvent.click(screen.getByRole("checkbox", { name: "Use GitHub in Dashy" }));
  expect(mocks.onEnabledChange).toHaveBeenCalledWith(["codex", "github"]);
});
```

Import `fireEvent` from Testing Library; no new test dependency is required.

- [ ] **Step 2: Run the component test and confirm it fails**

Run:

```powershell
npm --prefix frontend run test -- src/setup/ProviderManager.test.tsx
```

Expected: FAIL because `ProviderManager` and `useProviderSetup` do not exist.

- [ ] **Step 3: Implement per-provider asynchronous state**

Create `useProviderSetup.ts` with:

```typescript
export type ProviderSetupController = {
  states: ProviderSetupState[] | null;
  busyProvider: ProviderId | null;
  failureProvider: ProviderId | null;
  loadFailed: boolean;
  reload: () => Promise<void>;
  install: (provider: ProviderId) => Promise<void>;
  login: (provider: ProviderId) => Promise<void>;
};
```

On mount, call `getProviderSetupStates`. For install/login, set only that provider busy, call the typed API, replace the returned state by provider ID, and clear busy in `finally`. Store only `failureProvider` or the boolean `loadFailed`; discard the native error object so raw messages cannot render. A successful reload clears `loadFailed`.

- [ ] **Step 4: Implement the presentational provider manager**

Use this prop contract:

```typescript
type ProviderManagerProps = {
  controller: ProviderSetupController;
  enabledProviders: ProviderId[];
  onEnabledChange: (providers: ProviderId[]) => void;
};
```

Render provider cards in `ProviderId` order `claude`, `codex`, `github`. Each card contains:

- a checkbox labeled `setup.useProvider`;
- localized provider name and status;
- Install only for `notInstalled`;
- Connect only for `notAuthenticated`;
- Retry for `stale` or `unavailable`, calling `controller.reload` without opening a login flow;
- Connected state with no login button;
- one inline confirmation region at a time, using `role="group"` and `aria-labelledby`;
- a manual-help link using `installUrl` after an action failure;
- `aria-busy` only on the active provider card.

If `loadFailed` is true before provider definitions are available, render the localized `setup.actionFailure` message and a Retry button; never interpolate the rejected native error.

Keep `pendingAction` as `{ provider, action } | null` inside the component. The first click only populates this value. Only the confirmation button calls `controller.install` or `controller.login`.

- [ ] **Step 5: Run the provider-manager tests**

Run:

```powershell
npm --prefix frontend run test -- src/setup/ProviderManager.test.tsx
```

Expected: consent, provider-combination, busy-state, and sanitized-failure tests pass.

- [ ] **Step 6: Commit the reusable manager**

```powershell
git add frontend/src/setup
git commit -m "feat: build modular provider manager"
```

---

### Task 3: Add the first-run onboarding window and completion lifecycle

**Files:**
- Create: `frontend/src/onboarding/OnboardingApp.tsx`
- Create: `frontend/src/onboarding/OnboardingApp.test.tsx`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.test.tsx`
- Modify: `backend/tauri.conf.json`
- Modify: `backend/capabilities/default.json`
- Modify: `backend/src/desktop/mod.rs`
- Modify: `backend/src/desktop/controller.rs`
- Modify: `backend/src/desktop/commands.rs`
- Modify: `backend/src/lib.rs`

**Interfaces:**
- Consumes: `ProviderManager`, `getSettings`, `completeOnboarding`
- Produces: native window label `onboarding`
- Produces: `show_onboarding_window` and `hide_onboarding_window`

- [ ] **Step 1: Write failing onboarding selection tests**

Create `frontend/src/onboarding/OnboardingApp.test.tsx` and mock the setup controller. Cover connected-provider preselection and empty completion:

```typescript
const mocks = vi.hoisted(() => ({
  getSettings: vi.fn(),
  completeOnboarding: vi.fn(),
  controller: vi.fn(),
}));

vi.mock("../window", () => ({
  getSettings: mocks.getSettings,
  completeOnboarding: mocks.completeOnboarding,
}));
vi.mock("../setup/useProviderSetup", () => ({
  useProviderSetup: () => mocks.controller(),
}));

const metadata: Record<ProviderId, Omit<ProviderSetupDefinition, "provider">> = {
  claude: { publisher: "Anthropic", packageId: "Anthropic.ClaudeCode", installCommand: "winget install --id Anthropic.ClaudeCode --exact --source winget --interactive --accept-source-agreements --accept-package-agreements", installUrl: "https://code.claude.com/docs/en/setup", loginCommand: "claude auth login --claudeai" },
  codex: { publisher: "OpenAI", packageId: "OpenAI.Codex", installCommand: "winget install --id OpenAI.Codex --exact --source winget --interactive --accept-source-agreements --accept-package-agreements", installUrl: "https://learn.chatgpt.com/docs/codex/cli", loginCommand: "codex login" },
  github: { publisher: "GitHub", packageId: "GitHub.cli", installCommand: "winget install --id GitHub.cli --exact --source winget --interactive --accept-source-agreements --accept-package-agreements", installUrl: "https://cli.github.com/", loginCommand: "gh auth login --web" },
};

const cleanSettings: AppSettings = {
  placement: "right", monitor: null, locale: "en", alwaysShowOverFullscreen: false,
  onboardingCompleted: false, enabledProviders: [],
};

function states(status: Record<ProviderId, ProviderStatus>): ProviderSetupState[] {
  return (["claude", "codex", "github"] as ProviderId[]).map((provider) => ({
    definition: { provider, ...metadata[provider] },
    status: status[provider],
  }));
}

beforeEach(() => {
  mocks.getSettings.mockResolvedValue(cleanSettings);
  mocks.completeOnboarding.mockResolvedValue({ ...cleanSettings, onboardingCompleted: true });
  mocks.controller.mockReturnValue({
    states: states({ claude: "notInstalled", codex: "notInstalled", github: "notInstalled" }),
    busyProvider: null, failureProvider: null, loadFailed: false,
    reload: vi.fn(), install: vi.fn(), login: vi.fn(),
  });
});

it("preselects discovered connected providers but keeps every choice editable", async () => {
  mocks.getSettings.mockResolvedValue({ ...cleanSettings, enabledProviders: [] });
  mocks.controller.mockReturnValue({
    ...mocks.controller(),
    states: states({ claude: "connected", codex: "notInstalled", github: "connected" }),
  });
  render(<OnboardingApp />);
  expect(await screen.findByRole("checkbox", { name: "Use Claude in Dashy" })).toBeChecked();
  expect(screen.getByRole("checkbox", { name: "Use Codex in Dashy" })).not.toBeChecked();
  expect(screen.getByRole("checkbox", { name: "Use GitHub in Dashy" })).toBeChecked();
});

it("can finish with no providers and persists the exact empty selection", async () => {
  render(<OnboardingApp />);
  fireEvent.click(await screen.findByRole("button", { name: "Finish setup" }));
  expect(mocks.completeOnboarding).toHaveBeenCalledExactlyOnceWith([]);
});
```

- [ ] **Step 2: Run the onboarding test and confirm it fails**

Run:

```powershell
npm --prefix frontend run test -- src/onboarding/OnboardingApp.test.tsx src/App.test.tsx
```

Expected: FAIL because the onboarding component and window route do not exist.

- [ ] **Step 3: Implement the onboarding React surface**

`OnboardingApp` loads settings and provider states in parallel, applies the persisted locale, and initializes its editable selection once. If this is a clean install, select providers whose discovered status is `connected`; if saved providers exist, preserve that explicit set. Render:

```tsx
<main className="onboarding-app">
  <header className="onboarding-header">
    <p>{t("setup.eyebrow")}</p>
    <h1>{t("setup.title")}</h1>
    <span>{t("setup.description")}</span>
  </header>
  <ProviderManager
    controller={controller}
    enabledProviders={enabledProviders}
    onEnabledChange={setEnabledProviders}
  />
  <footer>
    <span role="status" aria-live="polite">{message}</span>
    <button type="button" onClick={finish}>{t("setup.finish")}</button>
  </footer>
</main>
```

`finish` calls `completeOnboarding(enabledProviders)` once, disables the footer button while pending, and renders only a localized failure message if the command rejects.

In `App.tsx`, route `windowLabel === "onboarding"` to `<OnboardingApp />`. Add an App routing test that expects the localized onboarding heading.

- [ ] **Step 4: Define and authorize the native onboarding window**

Add this window to `backend/tauri.conf.json`:

```json
{
  "label": "onboarding",
  "title": "Set up Dashy",
  "width": 680,
  "height": 640,
  "minWidth": 520,
  "minHeight": 560,
  "visible": false,
  "center": true,
  "resizable": true,
  "decorations": true,
  "transparent": false,
  "skipTaskbar": false
}
```

Add an `onboarding-capability` entry to `backend/capabilities/default.json` with `"windows": ["onboarding"]` and only `"core:default"`.

- [ ] **Step 5: Show onboarding only when required and close it atomically**

In `backend/src/desktop/mod.rs`, add `show_onboarding_window` and `hide_onboarding_window` following the existing settings-window error style. In `backend/src/lib.rs`, after settings are loaded and managed, call `show_onboarding_window` only when `current_settings.onboarding_completed` is false.

In `complete_onboarding`, persist settings first, publish the settings event, refresh the tray, then hide onboarding. If persistence fails, keep the window open.

When the tray Settings action is used while onboarding is incomplete, reopen the onboarding window instead of the normal Settings window. This gives a user who closed first-run setup a deterministic way back without enabling the edge rail prematurely.

Add the onboarding window handle to `TauriWindowPort::native_handles` so an active onboarding window is excluded from fullscreen-app detection.

- [ ] **Step 6: Run window-routing and backend lifecycle tests**

Run:

```powershell
npm --prefix frontend run test -- src/onboarding/OnboardingApp.test.tsx src/App.test.tsx
cargo test --manifest-path backend/Cargo.toml config_tests
cargo test --manifest-path backend/Cargo.toml desktop::commands
```

Expected: onboarding routes only to its label, clean completion persists exact providers, and native configuration tests pass.

- [ ] **Step 7: Commit first-run onboarding**

```powershell
git add frontend/src/onboarding frontend/src/App.tsx frontend/src/App.test.tsx backend/tauri.conf.json backend/capabilities/default.json backend/src
git commit -m "feat: add first-run provider onboarding"
```

---

### Task 4: Localize and style the provider setup experience

**Files:**
- Create: `frontend/src/onboarding.css`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/locales/en.ts`
- Modify: `frontend/src/locales/he.ts`
- Modify: `frontend/src/locales/ar.ts`
- Modify: `frontend/src/locales/es.ts`
- Modify: `frontend/src/locales/ru.ts`
- Modify: `frontend/src/locales/fr.ts`
- Modify: `frontend/src/locales/zh-CN.ts`
- Modify: `frontend/src/locales/ja.ts`
- Modify: `frontend/src/i18n.test.ts`

**Interfaces:**
- Produces: `Messages.setup` with the same leaves in all eight locale files.
- Produces: `.onboarding-*` and `.provider-setup-*` classes used by Tasks 2–3.

- [ ] **Step 1: Extend the English message contract first**

Add this shape to `Messages` in `frontend/src/locales/en.ts`:

```typescript
setup: {
  eyebrow: string; title: string; description: string; useProvider: string;
  connected: string; notInstalled: string; signInRequired: string; needsAttention: string;
  install: string; connect: string; retry: string; cancel: string;
  confirmInstall: string; confirmLogin: string; installDisclosure: string; loginDisclosure: string;
  publisher: string; packageId: string; command: string; manualHelp: string;
  finish: string; finishFailure: string; actionFailure: string; loading: string;
},
```

Add this exact English object:

```typescript
setup: {
  eyebrow: "DASHY / SETUP", title: "Choose what Dashy watches", description: "Connect only the tools you use. You can change this later in Settings.",
  useProvider: "Use {{provider}} in Dashy", connected: "Connected", notInstalled: "Not installed", signInRequired: "Sign in required", needsAttention: "Needs attention",
  install: "Install {{provider}}", connect: "Connect {{provider}}", retry: "Retry", cancel: "Cancel", confirmInstall: "Confirm installation", confirmLogin: "Open official login",
  installDisclosure: "Dashy will open a visible terminal and run this WinGet command.", loginDisclosure: "Dashy will open the provider's official login in a visible terminal and browser.",
  publisher: "Publisher", packageId: "Package", command: "Command", manualHelp: "Open official installation guide", finish: "Finish setup",
  finishFailure: "Dashy could not save your provider selection.", actionFailure: "Provider setup needs attention.", loading: "Checking installed tools",
},
```

- [ ] **Step 2: Run the locale contract test and confirm seven failures**

Run:

```powershell
npm --prefix frontend run test -- src/i18n.test.ts
```

Expected: FAIL because the seven non-English locales lack the new leaf keys.

- [ ] **Step 3: Add complete human-readable translations to every locale**

Add these exact `setup` objects. Keep provider product names and literal shell commands unchanged.

`frontend/src/locales/he.ts`:

```typescript
setup: {
  eyebrow: "DASHY / הגדרה", title: "בחרו במה Dashy יצפה", description: "חברו רק את הכלים שבהם אתם משתמשים. אפשר לשנות זאת מאוחר יותר בהגדרות.",
  useProvider: "השתמשו ב־{{provider}} ב־Dashy", connected: "מחובר", notInstalled: "לא מותקן", signInRequired: "נדרשת התחברות", needsAttention: "דורש טיפול",
  install: "התקנת {{provider}}", connect: "חיבור {{provider}}", retry: "ניסיון חוזר", cancel: "ביטול", confirmInstall: "אישור ההתקנה", confirmLogin: "פתיחת ההתחברות הרשמית",
  installDisclosure: "Dashy יפתח מסוף גלוי ויריץ את פקודת WinGet הזאת.", loginDisclosure: "Dashy יפתח את ההתחברות הרשמית של הספק במסוף גלוי ובדפדפן.",
  publisher: "מפרסם", packageId: "חבילה", command: "פקודה", manualHelp: "פתיחת מדריך ההתקנה הרשמי", finish: "סיום ההגדרה",
  finishFailure: "Dashy לא הצליח לשמור את בחירת הספקים.", actionFailure: "הגדרת הספק דורשת טיפול.", loading: "בודק כלים מותקנים",
},
```

`frontend/src/locales/ar.ts`:

```typescript
setup: {
  eyebrow: "DASHY / الإعداد", title: "اختر ما يراقبه Dashy", description: "صِل الأدوات التي تستخدمها فقط. يمكنك تغيير ذلك لاحقًا من الإعدادات.",
  useProvider: "استخدام {{provider}} في Dashy", connected: "متصل", notInstalled: "غير مثبت", signInRequired: "يلزم تسجيل الدخول", needsAttention: "يحتاج إلى إجراء",
  install: "تثبيت {{provider}}", connect: "ربط {{provider}}", retry: "إعادة المحاولة", cancel: "إلغاء", confirmInstall: "تأكيد التثبيت", confirmLogin: "فتح تسجيل الدخول الرسمي",
  installDisclosure: "سيفتح Dashy طرفية ظاهرة وينفذ أمر WinGet هذا.", loginDisclosure: "سيفتح Dashy تسجيل الدخول الرسمي للموفر في طرفية ظاهرة والمتصفح.",
  publisher: "الناشر", packageId: "الحزمة", command: "الأمر", manualHelp: "فتح دليل التثبيت الرسمي", finish: "إنهاء الإعداد",
  finishFailure: "تعذر على Dashy حفظ اختيار الموفرين.", actionFailure: "يحتاج إعداد الموفر إلى إجراء.", loading: "جارٍ فحص الأدوات المثبتة",
},
```

`frontend/src/locales/es.ts`:

```typescript
setup: {
  eyebrow: "DASHY / CONFIGURACIÓN", title: "Elige qué supervisa Dashy", description: "Conecta solo las herramientas que utilizas. Puedes cambiarlo más adelante en Ajustes.",
  useProvider: "Usar {{provider}} en Dashy", connected: "Conectado", notInstalled: "No instalado", signInRequired: "Debes iniciar sesión", needsAttention: "Requiere atención",
  install: "Instalar {{provider}}", connect: "Conectar {{provider}}", retry: "Reintentar", cancel: "Cancelar", confirmInstall: "Confirmar instalación", confirmLogin: "Abrir inicio de sesión oficial",
  installDisclosure: "Dashy abrirá una terminal visible y ejecutará este comando de WinGet.", loginDisclosure: "Dashy abrirá el inicio de sesión oficial del proveedor en una terminal visible y en el navegador.",
  publisher: "Editor", packageId: "Paquete", command: "Comando", manualHelp: "Abrir la guía oficial de instalación", finish: "Finalizar configuración",
  finishFailure: "Dashy no pudo guardar la selección de proveedores.", actionFailure: "La configuración del proveedor requiere atención.", loading: "Comprobando las herramientas instaladas",
},
```

`frontend/src/locales/ru.ts`:

```typescript
setup: {
  eyebrow: "DASHY / НАСТРОЙКА", title: "Выберите, что отслеживает Dashy", description: "Подключите только те инструменты, которыми пользуетесь. Это можно изменить позже в настройках.",
  useProvider: "Использовать {{provider}} в Dashy", connected: "Подключено", notInstalled: "Не установлено", signInRequired: "Требуется вход", needsAttention: "Требует внимания",
  install: "Установить {{provider}}", connect: "Подключить {{provider}}", retry: "Повторить", cancel: "Отмена", confirmInstall: "Подтвердить установку", confirmLogin: "Открыть официальный вход",
  installDisclosure: "Dashy откроет видимый терминал и выполнит эту команду WinGet.", loginDisclosure: "Dashy откроет официальный вход провайдера в видимом терминале и браузере.",
  publisher: "Издатель", packageId: "Пакет", command: "Команда", manualHelp: "Открыть официальное руководство по установке", finish: "Завершить настройку",
  finishFailure: "Dashy не удалось сохранить выбор провайдеров.", actionFailure: "Настройка провайдера требует внимания.", loading: "Проверка установленных инструментов",
},
```

`frontend/src/locales/fr.ts`:

```typescript
setup: {
  eyebrow: "DASHY / CONFIGURATION", title: "Choisissez ce que Dashy surveille", description: "Connectez uniquement les outils que vous utilisez. Vous pourrez modifier ce choix plus tard dans les paramètres.",
  useProvider: "Utiliser {{provider}} dans Dashy", connected: "Connecté", notInstalled: "Non installé", signInRequired: "Connexion requise", needsAttention: "Nécessite votre attention",
  install: "Installer {{provider}}", connect: "Connecter {{provider}}", retry: "Réessayer", cancel: "Annuler", confirmInstall: "Confirmer l’installation", confirmLogin: "Ouvrir la connexion officielle",
  installDisclosure: "Dashy ouvrira un terminal visible et exécutera cette commande WinGet.", loginDisclosure: "Dashy ouvrira la connexion officielle du fournisseur dans un terminal visible et le navigateur.",
  publisher: "Éditeur", packageId: "Paquet", command: "Commande", manualHelp: "Ouvrir le guide d’installation officiel", finish: "Terminer la configuration",
  finishFailure: "Dashy n’a pas pu enregistrer la sélection des fournisseurs.", actionFailure: "La configuration du fournisseur nécessite votre attention.", loading: "Vérification des outils installés",
},
```

`frontend/src/locales/zh-CN.ts`:

```typescript
setup: {
  eyebrow: "DASHY / 设置", title: "选择 Dashy 要监控的内容", description: "只连接你使用的工具。之后可随时在设置中更改。",
  useProvider: "在 Dashy 中使用 {{provider}}", connected: "已连接", notInstalled: "未安装", signInRequired: "需要登录", needsAttention: "需要处理",
  install: "安装 {{provider}}", connect: "连接 {{provider}}", retry: "重试", cancel: "取消", confirmInstall: "确认安装", confirmLogin: "打开官方登录",
  installDisclosure: "Dashy 将打开可见终端并运行此 WinGet 命令。", loginDisclosure: "Dashy 将在可见终端和浏览器中打开服务商的官方登录流程。",
  publisher: "发布者", packageId: "软件包", command: "命令", manualHelp: "打开官方安装指南", finish: "完成设置",
  finishFailure: "Dashy 无法保存服务商选择。", actionFailure: "服务商设置需要处理。", loading: "正在检查已安装的工具",
},
```

`frontend/src/locales/ja.ts`:

```typescript
setup: {
  eyebrow: "DASHY / セットアップ", title: "Dashy で確認するサービスを選択", description: "使用するツールだけを接続してください。後で設定から変更できます。",
  useProvider: "Dashy で {{provider}} を使用", connected: "接続済み", notInstalled: "未インストール", signInRequired: "サインインが必要", needsAttention: "確認が必要",
  install: "{{provider}} をインストール", connect: "{{provider}} に接続", retry: "再試行", cancel: "キャンセル", confirmInstall: "インストールを確認", confirmLogin: "公式ログインを開く",
  installDisclosure: "Dashy は表示されたターミナルを開き、この WinGet コマンドを実行します。", loginDisclosure: "Dashy は表示されたターミナルとブラウザーでプロバイダーの公式ログインを開きます。",
  publisher: "発行元", packageId: "パッケージ", command: "コマンド", manualHelp: "公式インストールガイドを開く", finish: "セットアップを完了",
  finishFailure: "Dashy はプロバイダーの選択を保存できませんでした。", actionFailure: "プロバイダーのセットアップを確認してください。", loading: "インストール済みツールを確認中",
},
```

- [ ] **Step 4: Implement the visual system**

Import `./onboarding.css` only for the onboarding/settings bundle path from `App.tsx`. Define these fixed tokens in `onboarding.css`, reusing the existing palette:

```css
.onboarding-app {
  --setup-surface: rgba(13, 18, 21, .94);
  --setup-panel: rgba(255, 255, 255, .045);
  --setup-border: rgba(255, 255, 255, .12);
  min-height: 100vh;
  padding: 30px;
  overflow: auto;
  color: var(--text-primary);
  background: radial-gradient(circle at 12% 0%, rgba(98, 230, 167, .1), transparent 32%), #080b0d;
}
```

Use a two-column provider-card grid above 620px and one column below it. Give each card a 3px logical-start status spine using its existing provider accent; do not add provider logos beyond the small glyph already used by Dashy. Inline confirmation expands inside the card rather than opening a generic modal. Keep all radii between 12px and 20px, body text at least 13px, controls at least 40px high, and focus rings from `--codex`.

Add:

```css
[dir="rtl"] .onboarding-app { text-align: right; }
@media (max-width: 620px) { .provider-setup-grid { grid-template-columns: 1fr; } }
@media (prefers-reduced-motion: reduce) { .provider-setup-card { transition: none; } }
```

- [ ] **Step 5: Run localization and UI tests**

Run:

```powershell
npm --prefix frontend run test -- src/i18n.test.ts src/setup/ProviderManager.test.tsx src/onboarding/OnboardingApp.test.tsx
npm --prefix frontend run build
```

Expected: all eight locale contracts, RTL checks, setup components, and TypeScript build pass.

- [ ] **Step 6: Commit localized styling**

```powershell
git add frontend/src/locales frontend/src/i18n.test.ts frontend/src/onboarding.css frontend/src/App.tsx
git commit -m "feat: localize provider onboarding"
```

---

### Task 5: Reuse provider management in Settings

**Files:**
- Modify: `frontend/src/settings/SettingsApp.tsx`
- Modify: `frontend/src/settings/SettingsApp.test.tsx`
- Modify: `frontend/src/settings.css`

**Interfaces:**
- Consumes: `ProviderManager`, `useProviderSetup`, and `updateSettings`
- Persists: `SettingsPatch.enabledProviders`

- [ ] **Step 1: Replace the old passive-status test with provider-management tests**

Add these assertions to `SettingsApp.test.tsx`:

```typescript
it("persists an independently enabled provider set from Settings", async () => {
  render(<SettingsApp />);
  fireEvent.click(await screen.findByRole("checkbox", { name: "Use GitHub in Dashy" }));
  expect(mocks.updateSettings).toHaveBeenCalledWith({ enabledProviders: ["claude", "codex"] });
});

it("offers repair actions without exposing raw native errors", async () => {
  mocks.getProviderSetupStates.mockRejectedValue(new Error("token=raw-secret"));
  render(<SettingsApp />);
  expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
  expect(document.body.textContent).not.toContain("raw-secret");
  expect(screen.getByText("Provider setup needs attention.")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run Settings tests and confirm the missing manager behavior**

Run:

```powershell
npm --prefix frontend run test -- src/settings/SettingsApp.test.tsx
```

Expected: FAIL because Settings still renders passive provider paragraphs.

- [ ] **Step 3: Replace passive status with the reusable manager**

Remove `ProviderState`, `PROVIDERS`, and the direct provider snapshot load from `SettingsApp`. Instantiate `useProviderSetup` and render `ProviderManager` below the general settings panel. On selection change:

```typescript
const saveProviders = async (enabledProviders: ProviderId[]) => {
  const saved = await save({ enabledProviders });
  if (saved) setSettings(saved);
};
```

Pass the confirmed `settings.enabledProviders`; do not optimistically mutate the selected list before Rust persistence succeeds.

Keep Refresh All for enabled dashboard data as a separate action, labeled with the existing `actions.refreshAll` translation.

- [ ] **Step 4: Adjust Settings layout without duplicating onboarding CSS**

Use the shared provider-manager classes from `onboarding.css`. In `settings.css`, only define placement within the existing scroll surface and remove the obsolete `.provider-status p` grid rules.

- [ ] **Step 5: Run Settings and locale tests**

Run:

```powershell
npm --prefix frontend run test -- src/settings/SettingsApp.test.tsx src/i18n.test.ts
npm --prefix frontend run build
```

Expected: settings persistence, sanitized failures, localization, and build pass.

- [ ] **Step 6: Commit Settings integration**

```powershell
git add frontend/src/settings frontend/src/settings.css
git commit -m "feat: manage providers from settings"
```

---

### Task 6: Render and size only enabled notch metrics

**Files:**
- Modify: `frontend/src/notch/NotchApp.tsx`
- Modify: `frontend/src/notch/MetricRail.tsx`
- Modify: `frontend/src/notch/NotchApp.test.tsx`
- Modify: `frontend/src/notch/NotchInteractions.test.tsx`
- Modify: `frontend/src/notch.css`
- Modify: `backend/src/desktop/edge.rs`
- Modify: `backend/src/desktop/controller.rs`
- Modify: `backend/tests/desktop_shell.rs`

**Interfaces:**
- Consumes: `AppSettings.enabledProviders` and `listenForSettingsChanges`
- Produces: `MetricRail.providers: ProviderId[]`
- Produces: `EdgeInput.provider_count: u8`
- Produces: provider-count-aware native rail dimensions: 110px, 190px, or 270px logical extent

- [ ] **Step 1: Write failing one-, two-, and zero-provider rail tests**

Add frontend tests:

```typescript
it.each([
  [["claude"], 1, "110"],
  [["claude", "github"], 2, "190"],
  [["claude", "codex", "github"], 3, "270"],
] as const)("renders only enabled providers %#", async (enabledProviders, count, extent) => {
  mocks.getSettings.mockResolvedValue({ ...settings, enabledProviders });
  render(<NotchApp snapshot={snapshot} />);
  expect(await screen.findAllByRole("button", { name: /Claude|Codex|GitHub/ })).toHaveLength(count);
  expect(screen.getByTestId("notch-surface")).toHaveStyle({ "--rail-extent": `${extent}px` });
});

it("renders no native surface when every provider is disabled", async () => {
  mocks.getSettings.mockResolvedValue({ ...settings, enabledProviders: [] });
  render(<NotchApp snapshot={snapshot} />);
  await waitFor(() => expect(screen.queryByTestId("notch-surface")).not.toBeInTheDocument());
});
```

Add a Rust geometry test:

```rust
#[test]
fn rail_extent_tracks_enabled_provider_count() {
    assert_eq!(rail_logical_extent(1), 110);
    assert_eq!(rail_logical_extent(2), 190);
    assert_eq!(rail_logical_extent(3), 270);
}
```

- [ ] **Step 2: Run focused tests and confirm fixed-three behavior fails**

Run:

```powershell
npm --prefix frontend run test -- src/notch/NotchApp.test.tsx src/notch/NotchInteractions.test.tsx
cargo test --manifest-path backend/Cargo.toml rail_extent_tracks_enabled_provider_count
```

Expected: frontend always renders three buttons and Rust has no count-aware geometry.

- [ ] **Step 3: Make the React rail provider-driven**

Add a required `providers: ProviderId[]` prop to `MetricRail` and map that list. In `NotchApp`, load `enabledProviders` with settings and subscribe to `listenForSettingsChanges`. Keep the selected provider valid with:

```typescript
useEffect(() => {
  if (!enabledProviders.includes(selectedProvider) && enabledProviders[0]) {
    lastSelected.current = enabledProviders[0];
    setSelectedProvider(enabledProviders[0]);
  }
}, [enabledProviders, selectedProvider]);
```

Use `enabledProviders` for arrow-key wraparound. If it is empty, render `<main className="notch-stage" data-testid="notch-app" />` without a surface.

Compute:

```typescript
const railExtent = 30 + enabledProviders.length * 80;
const selectedIndex = Math.max(0, enabledProviders.indexOf(selectedProvider));
const joinOffset = (selectedIndex - (enabledProviders.length - 1) / 2) * 80;
```

Expose `--rail-extent` and `--join-track-offset` as inline CSS properties and remove the three fixed `.notch-join[data-provider]` offset rules.

Derive `data-logical-size` from the same extent: collapsed side `${70}x${railExtent}`, collapsed top `${railExtent}x${70}`, while expanded side remains `370x360` and expanded top remains `340x430`.

- [ ] **Step 4: Make native geometry use the same count formula**

In `backend/src/desktop/edge.rs`, replace fixed side-height/top-width use with:

```rust
fn rail_logical_extent(provider_count: u8) -> u32 {
    30 + 80 * u32::from(provider_count.clamp(1, 3))
}
```

Add `provider_count: u8` to `EdgeInput`. Pass it through `window_layout_scaled`, using the extent as side rail height and top rail width. Expanded card dimensions remain at least 370x360 for side placement and 340x430 for top placement.

In `DesktopController::step`, set `provider_count` from `settings.enabled_providers.len()` after the zero-provider gate. Add `provider_count: 3` to every pre-existing `EdgeInput` test literal so those tests retain their original three-provider geometry; use `1`, `2`, and `3` only in the new modular-size cases.

- [ ] **Step 5: Update join/rail CSS**

Replace fixed dimensions with:

```css
.placement-right:not(.is-expanded),
.placement-left:not(.is-expanded) { width: 70px; height: var(--rail-extent); }
.placement-right .metric-rail,
.placement-left .metric-rail { width: 70px; height: var(--rail-extent); }
.placement-top:not(.is-expanded) { width: var(--rail-extent); height: 70px; }
.placement-top .metric-rail { width: var(--rail-extent); height: 70px; }
```

Keep expanded card sizes unchanged. The join continues to use `var(--join-track-offset)` for both placements.

- [ ] **Step 6: Run notch and native geometry suites**

Run:

```powershell
npm --prefix frontend run test -- src/notch/NotchApp.test.tsx src/notch/NotchInteractions.test.tsx
cargo test --manifest-path backend/Cargo.toml desktop::edge
cargo test --manifest-path backend/Cargo.toml --test desktop_shell
```

Expected: one/two/three provider geometry, keyboard wrapping, join position, and zero-provider suppression pass.

- [ ] **Step 7: Commit modular rail rendering**

```powershell
git add frontend/src/notch frontend/src/notch.css backend/src/desktop/edge.rs backend/src/desktop/controller.rs backend/tests/desktop_shell.rs
git commit -m "feat: size the notch for enabled providers"
```

---

### Task 7: Run visual, accessibility, and full-suite verification

**Files:**
- Verify: `frontend/src/setup/ProviderManager.test.tsx`
- Verify: `frontend/src/onboarding/OnboardingApp.test.tsx`

**Interfaces:**
- Verifies all UI interfaces produced by Tasks 1–6; produces no new runtime API.

- [ ] **Step 1: Run the complete frontend suite**

Run:

```powershell
npm --prefix frontend run test
npm --prefix frontend run build
```

Expected: all Vitest tests pass and strict TypeScript/Vite build succeeds.

- [ ] **Step 2: Run the complete backend suite**

Run:

```powershell
cargo fmt --manifest-path backend/Cargo.toml --check
cargo test --manifest-path backend/Cargo.toml
cargo clippy --manifest-path backend/Cargo.toml --all-targets -- -D warnings
```

Expected: formatting, all Rust tests, and Clippy pass with zero warnings.

- [ ] **Step 3: Perform a native visual and keyboard check**

Run:

```powershell
Set-Location backend
cargo tauri dev --no-watch
```

Verify the onboarding window at 680x640 and its 520px minimum width; switch to Hebrew and Arabic to inspect RTL; finish with one provider, two providers, and no providers; reopen Settings to add/remove providers; and confirm the edge rail changes size without a gray native rectangle. Use Tab and Shift+Tab through every checkbox, disclosure, confirmation, cancel, help link, and Finish button; use Enter/Space to activate each control; confirm inline status announcements change without moving focus. Capture no credentials or terminal contents in screenshots.

- [ ] **Step 4: Confirm the worktree is clean after verification**

```powershell
git status --short
```

Expected: no uncommitted files remain after the six implementation commits.
