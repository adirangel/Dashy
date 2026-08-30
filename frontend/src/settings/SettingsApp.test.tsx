import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot } from "../dashboard";

const mocks = vi.hoisted(() => ({
  getSettings: vi.fn(), updateSettings: vi.fn(), listMonitors: vi.fn(), setTrayLabels: vi.fn(),
  getDashboardSnapshot: vi.fn(), enable: vi.fn(), disable: vi.fn(), isEnabled: vi.fn(),
  emitLocaleChanged: vi.fn(),
}));

vi.mock("../window", () => ({
  getSettings: mocks.getSettings, updateSettings: mocks.updateSettings,
  listMonitors: mocks.listMonitors, setTrayLabels: mocks.setTrayLabels,
  emitLocaleChanged: mocks.emitLocaleChanged,
}));
vi.mock("../dashboard", async (importOriginal) => {
  const original = await importOriginal<typeof import("../dashboard")>();
  return { ...original, getDashboardSnapshot: mocks.getDashboardSnapshot };
});
vi.mock("@tauri-apps/plugin-autostart", () => ({
  enable: mocks.enable, disable: mocks.disable, isEnabled: mocks.isEnabled,
}));

import { localeResources, setLocale, SUPPORTED_LOCALES } from "../i18n";
import { SettingsApp, translatedTrayLabels } from "./SettingsApp";

const initialSettings = {
  placement: "right" as const, monitor: null, locale: "en" as const,
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
  refreshedAt: null,
};

describe("SettingsApp", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await setLocale("en");
    mocks.getSettings.mockResolvedValue(initialSettings);
    mocks.listMonitors.mockResolvedValue(monitors);
    mocks.isEnabled.mockResolvedValue(false);
    mocks.getDashboardSnapshot.mockResolvedValue(providerSnapshot);
    mocks.updateSettings.mockImplementation(async (patch: Record<string, unknown>) => ({ ...initialSettings, ...patch }));
    mocks.setTrayLabels.mockResolvedValue(undefined);
    mocks.emitLocaleChanged.mockResolvedValue(undefined);
    mocks.enable.mockResolvedValue(undefined);
    mocks.disable.mockResolvedValue(undefined);
  });

  afterEach(async () => { cleanup(); await setLocale("en"); });

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
    expect(screen.getByLabelText("Placement")).toHaveValue("right");
    expect(screen.getByLabelText("Monitor")).toHaveValue("");
    expect(screen.getByLabelText("Language")).toHaveValue("en");
    expect(screen.getByLabelText("Always show over fullscreen apps")).not.toBeChecked();
    expect(screen.getByLabelText("Launch at startup")).not.toBeChecked();
    expect(screen.getByRole("option", { name: "Portrait display" })).toBeInTheDocument();
    expect(screen.getByLabelText("Language").querySelectorAll("option")).toHaveLength(8);
  });

  it("keeps settings usable when the initial provider refresh fails", async () => {
    mocks.getDashboardSnapshot.mockRejectedValue(new Error("provider process failed"));
    render(<SettingsApp />);

    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByLabelText("Placement")).toHaveValue("right");
    expect(document.body.textContent).not.toContain("provider process failed");
  });

  it("persists placement, monitor, language, and fullscreen choices", async () => {
    render(<SettingsApp />);
    await screen.findByRole("heading", { name: "Settings" });
    const placement = screen.getByLabelText("Placement");
    for (const value of ["left", "top", "right"]) {
      fireEvent.change(placement, { target: { value } });
      await waitFor(() => expect(mocks.updateSettings).toHaveBeenCalledWith({ placement: value }));
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
    expect(screen.getByLabelText("Placement")).toHaveValue("right");
    expect(screen.getByRole("status")).toHaveTextContent("Try Dashy again later.");
    expect(document.body.textContent).not.toContain("raw monitor details");
  });

  it("renders confirmed Rust settings and disables uncertain startup state when its read fails", async () => {
    mocks.isEnabled.mockRejectedValue(new Error("raw registry details"));
    render(<SettingsApp />);

    expect(await screen.findByRole("heading", { name: "Settings" })).toBeInTheDocument();
    expect(screen.getByLabelText("Placement")).toHaveValue("right");
    const startup = screen.getByLabelText("Launch at startup");
    expect(startup).toBeDisabled();
    expect(startup).toBePartiallyChecked();
    expect(startup).toHaveAttribute("aria-checked", "mixed");
    expect(screen.getByRole("status")).toHaveTextContent("Try Dashy again later.");
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

  it("forces a full refresh and renders provider-specific sanitized guidance", async () => {
    render(<SettingsApp />);
    expect(await screen.findByText("Install the Claude CLI, then reopen Dashy.")).toBeInTheDocument();
    expect(screen.getByText("Sign in to Codex, then retry.")).toBeInTheDocument();
    expect(screen.getByText("Try GitHub again later.")).toBeInTheDocument();
    expect(document.body.textContent).not.toContain("raw-secret");
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
