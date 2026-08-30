import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  currentWindowLabel: vi.fn(), getSettings: vi.fn(), completeOnboarding: vi.fn(), listMonitors: vi.fn(),
  setTrayLabels: vi.fn(), getDashboardSnapshot: vi.fn(), isEnabled: vi.fn(),
  providerSetupController: vi.fn(),
  isTauriRuntime: vi.fn(), listenForLocaleChanges: vi.fn(), unlistenLocale: vi.fn(),
  listenForEdgeView: vi.fn(), unlistenEdgeView: vi.fn(),
  listenForSettingsChanges: vi.fn(), unlistenSettingsChanges: vi.fn(),
  listenForDashboardCacheChanged: vi.fn(), unlistenDashboardCacheChanged: vi.fn(),
  isCurrentWindowActive: vi.fn(),
  listenForCurrentWindowActivation: vi.fn(), unlistenWindowActivation: vi.fn(),
}));

vi.mock("./window", () => ({
  currentWindowLabel: mocks.currentWindowLabel, getSettings: mocks.getSettings,
  completeOnboarding: mocks.completeOnboarding,
  listMonitors: mocks.listMonitors, setTrayLabels: mocks.setTrayLabels, updateSettings: vi.fn(),
  isTauriRuntime: mocks.isTauriRuntime, listenForLocaleChanges: mocks.listenForLocaleChanges,
  listenForEdgeView: mocks.listenForEdgeView, setNotchInteraction: vi.fn(),
  listenForSettingsChanges: mocks.listenForSettingsChanges,
  listenForDashboardCacheChanged: mocks.listenForDashboardCacheChanged,
  isCurrentWindowActive: mocks.isCurrentWindowActive,
  listenForCurrentWindowActivation: mocks.listenForCurrentWindowActivation,
  showNotchMenu: vi.fn(),
}));
vi.mock("./setup/useProviderSetup", () => ({
  useProviderSetup: () => mocks.providerSetupController(),
}));
vi.mock("./dashboard", async (importOriginal) => {
  const original = await importOriginal<typeof import("./dashboard")>();
  return { ...original, getDashboardSnapshot: mocks.getDashboardSnapshot };
});
vi.mock("@tauri-apps/plugin-autostart", () => ({ enable: vi.fn(), disable: vi.fn(), isEnabled: mocks.isEnabled }));

import App from "./App";
import { unavailableDashboardSnapshot } from "./dashboard";

describe("window routing", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentWindowLabel.mockReturnValue("main");
    mocks.isTauriRuntime.mockReturnValue(true);
    mocks.listenForLocaleChanges.mockResolvedValue(mocks.unlistenLocale);
    mocks.listenForEdgeView.mockResolvedValue(mocks.unlistenEdgeView);
    mocks.listenForSettingsChanges.mockResolvedValue(mocks.unlistenSettingsChanges);
    mocks.listenForDashboardCacheChanged.mockResolvedValue(mocks.unlistenDashboardCacheChanged);
    mocks.isCurrentWindowActive.mockResolvedValue(true);
    mocks.listenForCurrentWindowActivation.mockResolvedValue(mocks.unlistenWindowActivation);
    mocks.getSettings.mockResolvedValue({ placement: "right", monitor: null, locale: "en", alwaysShowOverFullscreen: false, onboardingCompleted: true, enabledProviders: ["claude", "codex", "github"] });
    mocks.completeOnboarding.mockResolvedValue({ placement: "right", monitor: null, locale: "en", alwaysShowOverFullscreen: false, onboardingCompleted: true, enabledProviders: ["claude", "codex", "github"] });
    mocks.providerSetupController.mockReturnValue({
      states: [], busyProvider: null, failureProvider: null, loadFailed: false,
      reload: vi.fn(), install: vi.fn(), login: vi.fn(),
    });
    mocks.listMonitors.mockResolvedValue([]);
    mocks.setTrayLabels.mockResolvedValue(undefined);
    mocks.getDashboardSnapshot.mockResolvedValue(unavailableDashboardSnapshot());
    mocks.isEnabled.mockResolvedValue(false);
  });

  afterEach(cleanup);

  it("renders the settings surface for the native settings label", async () => {
    render(<App windowLabel="settings" />);
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
  });

  it("renders the localized onboarding surface only for the native onboarding label", async () => {
    render(<App windowLabel="onboarding" />);
    expect(await screen.findByRole("heading", { name: "Choose what Dashy watches" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Settings" })).not.toBeInTheDocument();
    expect(screen.queryByTestId("notch-app")).not.toBeInTheDocument();
  });

  it("renders the compile-safe notch placeholder for other native labels", () => {
    render(<App windowLabel="main" />);
    expect(screen.getByTestId("notch-app")).toBeInTheDocument();
  });

  it("bootstraps the independent main WebView from persisted Rust locale", async () => {
    mocks.getSettings.mockResolvedValue({ placement: "right", monitor: null, locale: "he", alwaysShowOverFullscreen: false, onboardingCompleted: true, enabledProviders: ["claude", "codex", "github"] });
    render(<App windowLabel="main" />);

    await waitFor(() => expect(document.documentElement.lang).toBe("he"));
    expect(document.documentElement.dir).toBe("rtl");
  });

  it("applies validated locale events in main and removes its listener on cleanup", async () => {
    let localeHandler: ((locale: unknown) => void) | undefined;
    mocks.listenForLocaleChanges.mockImplementation(async (handler: (locale: unknown) => void) => {
      localeHandler = handler;
      return mocks.unlistenLocale;
    });
    const view = render(<App windowLabel="main" />);
    await waitFor(() => expect(localeHandler).toBeTypeOf("function"));

    localeHandler?.("ar");
    await waitFor(() => expect(document.documentElement.lang).toBe("ar"));
    expect(document.documentElement.dir).toBe("rtl");

    view.unmount();
    expect(mocks.unlistenLocale).toHaveBeenCalledTimes(1);
  });

  it("does not let a stale startup read overwrite a newer locale event", async () => {
    let resolveSettings!: (value: { placement: "right"; monitor: null; locale: "he"; alwaysShowOverFullscreen: false; onboardingCompleted: true; enabledProviders: ["claude", "codex", "github"] }) => void;
    mocks.getSettings.mockReturnValue(new Promise((resolve) => { resolveSettings = resolve; }));
    let localeHandler: ((locale: unknown) => void) | undefined;
    mocks.listenForLocaleChanges.mockImplementation(async (handler: (locale: unknown) => void) => {
      localeHandler = handler;
      return mocks.unlistenLocale;
    });
    render(<App windowLabel="main" />);
    await waitFor(() => expect(localeHandler).toBeTypeOf("function"));

    localeHandler?.("ja");
    resolveSettings({ placement: "right", monitor: null, locale: "he", alwaysShowOverFullscreen: false, onboardingCompleted: true, enabledProviders: ["claude", "codex", "github"] });

    await waitFor(() => expect(document.documentElement.lang).toBe("ja"));
    expect(document.documentElement.dir).toBe("ltr");
  });

  it("keeps browser fallback deterministic without native settings or listeners", async () => {
    mocks.isTauriRuntime.mockReturnValue(false);
    render(<App windowLabel="main" />);

    expect(await screen.findByTestId("notch-app")).toBeInTheDocument();
    expect(mocks.getSettings).not.toHaveBeenCalled();
    expect(mocks.listenForLocaleChanges).not.toHaveBeenCalled();
  });

  it("uses the deterministic main-label fallback supplied by the window boundary", async () => {
    mocks.isTauriRuntime.mockReturnValue(false);
    render(<App />);
    expect(mocks.currentWindowLabel).toHaveBeenCalledTimes(1);
    expect(await screen.findByTestId("notch-app")).toBeInTheDocument();
  });
});
