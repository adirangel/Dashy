import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot } from "../dashboard";
import type { ProviderSetupState } from "../setup/api";
import type { AppSettings } from "../window";

const mocks = vi.hoisted(() => ({
  getSettings: vi.fn(), updateSettings: vi.fn(), listMonitors: vi.fn(), setTrayLabels: vi.fn(),
  getDashboardSnapshot: vi.fn(), enable: vi.fn(), disable: vi.fn(), isEnabled: vi.fn(),
  emitLocaleChanged: vi.fn(),
  listenForSettingsChanges: vi.fn(), unlistenSettingsChanges: vi.fn(),
  activationRevision: vi.fn(),
  getProviderSetupStates: vi.fn(), installProvider: vi.fn(), loginProvider: vi.fn(),
}));

vi.mock("../window", () => ({
  getSettings: mocks.getSettings, updateSettings: mocks.updateSettings,
  listMonitors: mocks.listMonitors, setTrayLabels: mocks.setTrayLabels,
  emitLocaleChanged: mocks.emitLocaleChanged,
  listenForSettingsChanges: mocks.listenForSettingsChanges,
}));
vi.mock("../useWindowActivation", () => ({
  useWindowActivationRevision: () => mocks.activationRevision(),
}));
vi.mock("../dashboard", async (importOriginal) => {
  const original = await importOriginal<typeof import("../dashboard")>();
  return { ...original, getDashboardSnapshot: mocks.getDashboardSnapshot };
});
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: mocks.enable, disable: mocks.disable, isEnabled: mocks.isEnabled,
}));
vi.mock("../setup/api", async (importOriginal) => {
  const original = await importOriginal<typeof import("../setup/api")>();
  return {
    ...original,
    getProviderSetupStates: mocks.getProviderSetupStates,
    installProvider: mocks.installProvider,
    loginProvider: mocks.loginProvider,
  };
});

import i18n, { localeResources, setLocale, SUPPORTED_LOCALES } from "../i18n";
import { SettingsApp, translatedTrayLabels } from "./SettingsApp";

const initialSettings: AppSettings = {
  placement: "right", monitor: null, locale: "en",
  alwaysShowOverFullscreen: false,
  onboardingCompleted: true,
  enabledProviders: ["claude", "codex", "github"],
};
const monitors = [
  { id: "display-1", name: "Studio display", x: 0, y: 0, width: 1920, height: 1040, primary: true },
  { id: "display-2", name: "Portrait display", x: 1920, y: 0, width: 1080, height: 1920, primary: false },
];
const providerSnapshot: DashboardSnapshot = {
  github: { status: "unavailable", accountLogin: null, contributionDays: null, currentStreakDays: null, lastSuccessfulRefresh: null, errorKind: "raw-secret-github-error" },
  codex: { status: "notAuthenticated", remainingPercent: null, shortWindow: null, weeklyWindow: null, lastSuccessfulRefresh: null, errorKind: "raw-secret-codex-error" },
  claude: { status: "notInstalled", remainingPercent: null, shortWindow: null, weeklyWindow: null, lastSuccessfulRefresh: null, errorKind: "raw-secret-claude-error" },
  grok: { status: "notAuthenticated", remainingPercent: null, shortWindow: null, weeklyWindow: null, lastSuccessfulRefresh: null, errorKind: "raw-secret-grok-error" },
  cursor: { status: "unavailable", subscriptionTier: null, accountEmail: null, lastSuccessfulRefresh: null, errorKind: "raw-secret-cursor-error" },
  refreshedAt: null,
};
const providerSetupStates: ProviderSetupState[] = [
  {
    definition: {
      provider: "claude", publisher: "Anthropic", packageId: "Anthropic.ClaudeCode",
      installKind: "winget",
      installCommand: "winget install --id Anthropic.ClaudeCode", installUrl: "https://code.claude.com/docs/en/setup",
      loginCommand: "claude auth login --claudeai",
    },
    status: "connected",
    repairAction: null,
  },
  {
    definition: {
      provider: "codex", publisher: "OpenAI", packageId: "OpenAI.Codex",
      installKind: "winget",
      installCommand: "winget install --id OpenAI.Codex", installUrl: "https://learn.chatgpt.com/docs/codex/cli",
      loginCommand: "codex login",
    },
    status: "connected",
    repairAction: null,
  },
  {
    definition: {
      provider: "github", publisher: "GitHub", packageId: "GitHub.cli",
      installKind: "winget",
      installCommand: "winget install --id GitHub.cli", installUrl: "https://cli.github.com/",
      loginCommand: "gh auth login --web",
    },
    status: "connected",
    repairAction: null,
  },
  {
    definition: {
      provider: "grok", publisher: "xAI", packageId: "xAI.GrokBuild",
      installKind: "winget",
      installCommand: "winget install --id xAI.GrokBuild", installUrl: "https://docs.x.ai/build/overview",
      loginCommand: "grok login",
    },
    status: "connected",
    repairAction: null,
  },
  {
    definition: {
      provider: "cursor", publisher: "Anysphere", packageId: null,
      installKind: "manualUrl",
      installCommand: null,
      installUrl: "https://cursor.com/docs/cli/installation",
      loginCommand: "cursor-agent login",
    },
    status: "connected",
    repairAction: null,
  },
];

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((fulfill) => { resolve = fulfill; });
  return { promise, resolve };
}

describe("SettingsApp", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await setLocale("en");
    mocks.getSettings.mockResolvedValue(initialSettings);
    mocks.listMonitors.mockResolvedValue(monitors);
    mocks.isEnabled.mockResolvedValue(false);
    mocks.getDashboardSnapshot.mockResolvedValue(providerSnapshot);
    mocks.getProviderSetupStates.mockResolvedValue(providerSetupStates);
    mocks.installProvider.mockResolvedValue(providerSetupStates[0]);
    mocks.loginProvider.mockResolvedValue(providerSetupStates[0]);
    mocks.updateSettings.mockImplementation(async (patch: Record<string, unknown>) => ({ ...initialSettings, ...patch }));
    mocks.setTrayLabels.mockResolvedValue(undefined);
    mocks.emitLocaleChanged.mockResolvedValue(undefined);
    mocks.listenForSettingsChanges.mockResolvedValue(mocks.unlistenSettingsChanges);
    mocks.activationRevision.mockReturnValue(1);
    mocks.enable.mockResolvedValue(undefined);
    mocks.disable.mockResolvedValue(undefined);
  });

  afterEach(async () => {
    cleanup();
    vi.restoreAllMocks();
    await setLocale("en");
  });

  it("builds all native tray labels from existing locale keys in every locale", () => {
    for (const locale of SUPPORTED_LOCALES) {
      const labels = translatedTrayLabels(locale);
      const messages = localeResources[locale].translation;
      expect(labels).toEqual({
        show: messages.menu.show,
        refreshAll: messages.menu.refreshAll,
        placement: messages.menu.placement,
        right: messages.settings.right,
        left: messages.settings.left,
        top: messages.settings.top,
        monitor: messages.menu.monitor,
        primaryMonitor: messages.menu.primaryMonitor,
        unavailable: messages.status.unavailable,
        settings: messages.menu.settings,
        quit: messages.menu.quit,
      });
    }
  });

  it("loads persisted values, discovered monitors, and all eight languages", async () => {
    render(<SettingsApp />);
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Placement" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Right" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Left" })).not.toBeChecked();
    expect(screen.getByLabelText("Monitor")).toHaveValue("");
    expect(screen.getByLabelText("Language")).toHaveValue("en");
    expect(screen.getByLabelText("Always show over fullscreen apps")).not.toBeChecked();
    expect(screen.getByLabelText("Launch at startup")).not.toBeChecked();
    expect(screen.getByRole("option", { name: "Portrait display" })).toBeInTheDocument();
    expect(screen.getByLabelText("Language").querySelectorAll("option")).toHaveLength(8);
  });

  it("offers repair actions without exposing raw native errors", async () => {
    mocks.getProviderSetupStates.mockRejectedValue(new Error("token=raw-secret"));
    render(<SettingsApp />);

    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(await screen.findByText("Provider setup needs attention.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("raw-secret");
  });

  it("keeps every hidden-window data source dormant until activation", async () => {
    mocks.activationRevision.mockReturnValue(0);
    render(<SettingsApp />);
    await waitFor(() => expect(mocks.listenForSettingsChanges).toHaveBeenCalledTimes(1));

    expect(mocks.getSettings).not.toHaveBeenCalled();
    expect(mocks.listMonitors).not.toHaveBeenCalled();
    expect(mocks.isEnabled).not.toHaveBeenCalled();
    expect(mocks.getProviderSetupStates).not.toHaveBeenCalled();
  });

  it("reloads persisted settings on focus before exposing editable controls", async () => {
    const focusedSettings = deferred<typeof initialSettings>();
    mocks.getSettings
      .mockResolvedValueOnce(initialSettings)
      .mockReturnValueOnce(focusedSettings.promise);
    const view = render(<SettingsApp />);
    await screen.findByRole("heading", { name: "Settings" });

    mocks.activationRevision.mockReturnValue(2);
    view.rerender(<SettingsApp />);

    expect(screen.queryByRole("radiogroup", { name: "Placement" })).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Loading");
    focusedSettings.resolve({ ...initialSettings, enabledProviders: ["codex"] });
    expect(await screen.findByRole("radio", { name: "Right" })).toBeEnabled();
    expect(screen.getByRole("checkbox", { name: "Use Claude in Dashy" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use Codex in Dashy" })).toBeChecked();
  });

  it("applies settings events, ignores an older pending read, and cleans up its listener", async () => {
    const staleRead = deferred<typeof initialSettings>();
    mocks.getSettings.mockReturnValue(staleRead.promise);
    let settingsHandler: ((settings: typeof initialSettings) => void) | undefined;
    mocks.listenForSettingsChanges.mockImplementation(async (handler) => {
      settingsHandler = handler;
      return mocks.unlistenSettingsChanges;
    });
    const view = render(<SettingsApp />);
    await waitFor(() => expect(settingsHandler).toBeTypeOf("function"));

    act(() => settingsHandler?.({ ...initialSettings, enabledProviders: ["codex"] }));
    expect(await screen.findByRole("checkbox", { name: "Use Codex in Dashy" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use Claude in Dashy" })).not.toBeChecked();

    staleRead.resolve(initialSettings);
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Use Claude in Dashy" }))
      .not.toBeChecked());
    view.unmount();
    expect(mocks.unlistenSettingsChanges).toHaveBeenCalledTimes(1);
  });

  it("restores the newest event locale when an older locale change completes last", async () => {
    const olderLocale = deferred<void>();
    const newerLocale = deferred<void>();
    const languageChange = vi.spyOn(i18n, "changeLanguage")
      .mockImplementationOnce(async () => {
        await olderLocale.promise;
        return i18n.t;
      })
      .mockImplementationOnce(async () => {
        await newerLocale.promise;
        return i18n.t;
      })
      .mockImplementation(async () => i18n.t);
    mocks.getSettings.mockResolvedValue({ ...initialSettings, locale: "he" });
    let settingsHandler: ((settings: typeof initialSettings) => void) | undefined;
    mocks.listenForSettingsChanges.mockImplementation(async (handler) => {
      settingsHandler = handler;
      return mocks.unlistenSettingsChanges;
    });
    render(<SettingsApp />);
    await waitFor(() => expect(languageChange).toHaveBeenCalledWith("he"));

    act(() => settingsHandler?.({ ...initialSettings, locale: "ja", enabledProviders: ["codex"] }));
    await waitFor(() => expect(languageChange).toHaveBeenCalledWith("ja"));
    newerLocale.resolve();
    await waitFor(() => expect(document.documentElement.lang).toBe("ja"));

    olderLocale.resolve();
    await waitFor(() => expect(languageChange).toHaveBeenCalledTimes(3));
    expect(languageChange).toHaveBeenLastCalledWith("ja");
    expect(document.documentElement.lang).toBe("ja");
  });

  it("uses the latest persisted provider set on first open after onboarding completes", async () => {
    mocks.activationRevision.mockReturnValue(0);
    let settingsHandler: ((settings: typeof initialSettings) => void) | undefined;
    mocks.listenForSettingsChanges.mockImplementation(async (handler) => {
      settingsHandler = handler;
      return mocks.unlistenSettingsChanges;
    });
    const view = render(<SettingsApp />);
    await waitFor(() => expect(settingsHandler).toBeTypeOf("function"));
    act(() => settingsHandler?.({ ...initialSettings, enabledProviders: ["claude"] }));

    mocks.getSettings.mockResolvedValue({ ...initialSettings, enabledProviders: ["codex"] });
    mocks.activationRevision.mockReturnValue(1);
    view.rerender(<SettingsApp />);
    const github = await screen.findByRole("checkbox", { name: "Use GitHub in Dashy" });
    expect(screen.getByRole("checkbox", { name: "Use Claude in Dashy" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use Codex in Dashy" })).toBeChecked();

    fireEvent.click(github);
    await waitFor(() => expect(mocks.updateSettings)
      .toHaveBeenCalledWith({ enabledProviders: ["codex", "github"] }));
  });

  it("persists an independently enabled provider set from Settings", async () => {
    render(<SettingsApp />);

    fireEvent.click(await screen.findByRole("checkbox", { name: "Use GitHub in Dashy" }));

    await waitFor(() => expect(mocks.updateSettings)
      .toHaveBeenCalledWith({ enabledProviders: ["claude", "codex"] }));
  });

  it("keeps the last confirmed provider set when persistence fails", async () => {
    mocks.updateSettings.mockRejectedValue(new Error("token=raw-settings-secret"));
    render(<SettingsApp />);

    const github = await screen.findByRole("checkbox", { name: "Use GitHub in Dashy" });
    fireEvent.click(github);

    await waitFor(() => expect(mocks.updateSettings)
      .toHaveBeenCalledWith({ enabledProviders: ["claude", "codex"] }));
    expect(github).toBeChecked();
    expect(document.body.textContent).not.toContain("raw-settings-secret");
  });

  it("prevents a second provider selection from racing an unconfirmed save", async () => {
    const firstSave = deferred<typeof initialSettings>();
    mocks.updateSettings.mockReturnValueOnce(firstSave.promise);
    render(<SettingsApp />);

    const github = await screen.findByRole("checkbox", { name: "Use GitHub in Dashy" });
    const codex = screen.getByRole("checkbox", { name: "Use Codex in Dashy" });
    fireEvent.click(github);
    await waitFor(() => expect(mocks.updateSettings).toHaveBeenCalledTimes(1));

    expect(github).toBeDisabled();
    expect(codex).toBeDisabled();
    fireEvent.click(codex);
    expect(mocks.updateSettings).toHaveBeenCalledTimes(1);

    firstSave.resolve({ ...initialSettings, enabledProviders: ["claude", "codex"] });
    await waitFor(() => expect(github).not.toBeChecked());
    expect(codex).toBeEnabled();
    expect(mocks.updateSettings).toHaveBeenCalledTimes(1);
  });

  it("persists placement, monitor, language, and fullscreen choices", async () => {
    render(<SettingsApp />);
    await screen.findByRole("heading", { name: "Settings" });
    for (const [value, label] of [["left", "Left"], ["top", "Top"], ["right", "Right"]] as const) {
      fireEvent.click(screen.getByRole("radio", { name: label }));
      await waitFor(() => expect(mocks.updateSettings).toHaveBeenCalledWith({ placement: value }));
      await waitFor(() => expect(screen.getByRole("radio", { name: label })).toBeChecked());
    }
    fireEvent.change(screen.getByLabelText("Monitor"), { target: { value: "display-2" } });
    await waitFor(() => expect(mocks.updateSettings).toHaveBeenCalledWith({ monitor: {
      id: "display-2", name: "Portrait display",
      lastWorkArea: { x: 1920, y: 0, width: 1080, height: 1920 },
    } }));
    fireEvent.change(screen.getByLabelText("Language"), { target: { value: "he" } });
    await waitFor(() => expect(mocks.updateSettings).toHaveBeenCalledWith({ locale: "he" }));
    await waitFor(() => expect(document.documentElement.dir).toBe("rtl"));
    expect(mocks.setTrayLabels).toHaveBeenCalled();
    expect(mocks.emitLocaleChanged).toHaveBeenCalledWith("he");
    fireEvent.click(screen.getByLabelText("הצג תמיד מעל יישומים במסך מלא"));
    await waitFor(() => expect(mocks.updateSettings).toHaveBeenCalledWith({ alwaysShowOverFullscreen: true }));
  });

  it("does not emit a locale event when Rust rejects the language update", async () => {
    mocks.updateSettings.mockRejectedValue(new Error("settings store failed"));
    render(<SettingsApp />);
    await screen.findByRole("heading", { name: "Settings" });

    fireEvent.change(screen.getByLabelText("Language"), { target: { value: "fr" } });

    await waitFor(() => expect(mocks.updateSettings).toHaveBeenCalledWith({ locale: "fr" }));
    expect(mocks.emitLocaleChanged).not.toHaveBeenCalled();
    expect(document.body.textContent).not.toContain("settings store failed");
  });

  it("renders confirmed Rust settings when monitor discovery fails", async () => {
    mocks.listMonitors.mockRejectedValue(new Error("raw monitor details"));
    render(<SettingsApp />);

    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Placement" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Right" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Left" })).not.toBeChecked();
    expect(screen.getByText("Try Dashy again later.", { selector: ".settings-message" }))
      .toHaveAttribute("role", "status");
    expect(document.body.textContent).not.toContain("raw monitor details");
  });

  it("renders confirmed Rust settings and disables uncertain startup state when its read fails", async () => {
    mocks.isEnabled.mockRejectedValue(new Error("raw registry details"));
    render(<SettingsApp />);

    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByRole("radiogroup", { name: "Placement" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Right" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Left" })).not.toBeChecked();
    const startup = screen.getByLabelText("Launch at startup");
    expect(startup).toBeDisabled();
    expect(startup).toBePartiallyChecked();
    expect(startup).toHaveAttribute("aria-checked", "mixed");
    expect(screen.getByText("Try Dashy again later.", { selector: ".settings-message" }))
      .toHaveAttribute("role", "status");
    expect(document.body.textContent).not.toContain("raw registry details");
  });

  it("renders a confirmed disabled startup preference as unchecked rather than unknown", async () => {
    mocks.isEnabled.mockResolvedValue(false);
    render(<SettingsApp />);

    const startup = await screen.findByLabelText("Launch at startup");
    await waitFor(() => expect(startup).toBeEnabled());
    expect(startup).not.toBeChecked();
    expect(startup).not.toBePartiallyChecked();
    expect(startup).toHaveAttribute("aria-checked", "false");
  });

  it("renders a confirmed enabled startup preference as checked rather than unknown", async () => {
    mocks.isEnabled.mockResolvedValue(true);
    render(<SettingsApp />);

    const startup = await screen.findByLabelText("Launch at startup");
    await waitFor(() => expect(startup).toBeEnabled());
    expect(startup).toBeChecked();
    expect(startup).not.toBePartiallyChecked();
    expect(startup).toHaveAttribute("aria-checked", "true");
  });

  it("enables startup only after the plugin confirms the new state", async () => {
    mocks.isEnabled.mockResolvedValueOnce(false).mockResolvedValueOnce(true);
    render(<SettingsApp />);
    const startup = await screen.findByLabelText("Launch at startup");
    fireEvent.click(startup);
    await waitFor(() => expect(mocks.enable).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(startup).toBeChecked());
  });

  it("disables startup only after the plugin confirms the new state", async () => {
    mocks.isEnabled.mockResolvedValueOnce(true).mockResolvedValueOnce(false);
    render(<SettingsApp />);
    const startup = await screen.findByLabelText("Launch at startup");
    await waitFor(() => expect(startup).toBeChecked());
    fireEvent.click(startup);
    await waitFor(() => expect(mocks.disable).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(startup).not.toBeChecked());
  });

  it("keeps the last confirmed startup state when the plugin operation fails", async () => {
    mocks.enable.mockRejectedValue(new Error("startup registry unavailable"));
    render(<SettingsApp />);
    const startup = await screen.findByLabelText("Launch at startup");
    fireEvent.click(startup);
    await waitFor(() => expect(mocks.enable).toHaveBeenCalledTimes(1));
    expect(startup).not.toBeChecked();
    expect(screen.queryByText("startup registry unavailable")).not.toBeInTheDocument();
  });

  it("keeps Refresh all as a separate enabled-dashboard data action", async () => {
    render(<SettingsApp />);
    await screen.findByRole("heading", { name: "Settings" });
    fireEvent.click(screen.getByRole("button", { name: "Refresh all" }));
    await waitFor(() => expect(mocks.getDashboardSnapshot).toHaveBeenCalledWith(true));
  });

  it("owns a keyboard-focusable scroll surface that contains the bottom refresh and provider content", async () => {
    mocks.getSettings.mockResolvedValue({ ...initialSettings, locale: "ru" });
    render(<SettingsApp />);

    const scrollSurface = await screen.findByTestId("settings-scroll-surface");
    const refresh = await screen.findByRole("button", { name: "Обновить всё" });
    expect(scrollSurface).toHaveAttribute("tabindex", "0");
    expect(scrollSurface).toHaveAttribute("data-scroll-owner", "settings");
    expect(scrollSurface).toContainElement(refresh);
    expect(scrollSurface).toContainElement(screen.getByText("GitHub"));
    expect(refresh).toBeEnabled();
  });
});
