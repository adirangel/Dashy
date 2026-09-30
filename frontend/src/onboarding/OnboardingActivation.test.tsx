import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setLocale } from "../i18n";
import type { ProviderSetupState } from "../setup/api";
import type { AppSettings } from "../window";

const mocks = vi.hoisted(() => ({
  activationRevision: vi.fn(),
  getSettings: vi.fn(),
  completeOnboarding: vi.fn(),
  setTrayLabels: vi.fn(),
  emitLocaleChanged: vi.fn(),
  getProviderSetupStates: vi.fn(),
  installProvider: vi.fn(),
  loginProvider: vi.fn(),
}));

vi.mock("../useWindowActivation", () => ({
  useWindowActivationRevision: () => mocks.activationRevision(),
}));
vi.mock("../window", () => ({
  getSettings: mocks.getSettings,
  completeOnboarding: mocks.completeOnboarding,
  setTrayLabels: mocks.setTrayLabels,
  emitLocaleChanged: mocks.emitLocaleChanged,
}));
vi.mock("../setup/api", async (importOriginal) => ({
  ...await importOriginal<typeof import("../setup/api")>(),
  getProviderSetupStates: mocks.getProviderSetupStates,
  installProvider: mocks.installProvider,
  loginProvider: mocks.loginProvider,
}));

import { OnboardingApp } from "./OnboardingApp";

const baseSettings: AppSettings = {
  placement: "right",
  monitor: null,
  locale: "en",
  alwaysShowOverFullscreen: false,
  onboardingCompleted: false,
  enabledProviders: [],
};

const providerStates: ProviderSetupState[] = [
  ["claude", "Anthropic", "Anthropic.ClaudeCode", "https://code.claude.com/docs/en/setup", "claude auth login --claudeai"],
  ["codex", "OpenAI", "OpenAI.Codex", "https://learn.chatgpt.com/docs/codex/cli", "codex login"],
  ["github", "GitHub", "GitHub.cli", "https://cli.github.com/", "gh auth login --web"],
  ["grok", "xAI", "xAI.GrokBuild", "https://docs.x.ai/build/overview", "grok login"],
  ["cursor", "Anysphere", null, "https://cursor.com/docs/cli/installation", "cursor-agent login"],
].map(([provider, publisher, packageId, installUrl, loginCommand]) => ({
  definition: {
    provider: provider as "claude" | "codex" | "github" | "grok" | "cursor",
    publisher: publisher as string,
    packageId: packageId as string | null,
    installKind: packageId === null ? "manualUrl" as const : "winget" as const,
    installCommand: packageId === null ? null : `winget install --id ${packageId}`,
    installUrl: installUrl as string,
    loginCommand: loginCommand as string,
  },
  status: "connected" as const,
  errorKind: null,
  repairAction: null,
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((fulfill) => { resolve = fulfill; });
  return { promise, resolve };
}

describe("Onboarding activation", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await setLocale("en");
    mocks.activationRevision.mockReturnValue(0);
    mocks.getSettings.mockResolvedValue(baseSettings);
    mocks.getProviderSetupStates.mockResolvedValue(providerStates);
    mocks.installProvider.mockResolvedValue(providerStates[0]);
    mocks.loginProvider.mockResolvedValue(providerStates[0]);
    mocks.completeOnboarding.mockResolvedValue({ ...baseSettings, onboardingCompleted: true });
    mocks.setTrayLabels.mockResolvedValue(undefined);
    mocks.emitLocaleChanged.mockResolvedValue(undefined);
  });

  afterEach(async () => {
    cleanup();
    await setLocale("en");
  });

  it("performs zero settings and provider IPC while hidden, including Strict Mode", async () => {
    render(<StrictMode><OnboardingApp /></StrictMode>);
    await act(async () => { await Promise.resolve(); });

    expect(mocks.getSettings).not.toHaveBeenCalled();
    expect(mocks.getProviderSetupStates).not.toHaveBeenCalled();
  });

  it("loads on every activation and prevents an older settings response from winning", async () => {
    const staleSettings = deferred<AppSettings>();
    mocks.activationRevision.mockReturnValue(1);
    mocks.getSettings
      .mockReturnValueOnce(staleSettings.promise)
      .mockResolvedValueOnce({ ...baseSettings, locale: "ja", enabledProviders: ["codex"] });
    const view = render(<OnboardingApp />);
    await waitFor(() => expect(mocks.getSettings).toHaveBeenCalledTimes(1));

    mocks.activationRevision.mockReturnValue(2);
    view.rerender(<OnboardingApp />);
    await waitFor(() => expect(document.documentElement.lang).toBe("ja"));
    fireEvent.click(screen.getByRole("button", { name: "続行" }));
    const codexCard = await screen.findByRole("article", { name: "Codex" });
    expect(within(codexCard).getByRole("checkbox")).toBeChecked();
    expect(document.documentElement.lang).toBe("ja");

    staleSettings.resolve({ ...baseSettings, locale: "he", enabledProviders: ["claude"] });
    await act(async () => { await Promise.resolve(); });
    expect(document.documentElement.lang).toBe("ja");
    const claudeCard = screen.getByRole("article", { name: "Claude" });
    expect(within(claudeCard).getByRole("checkbox")).not.toBeChecked();
    expect(mocks.getSettings).toHaveBeenCalledTimes(2);
    expect(mocks.getProviderSetupStates).toHaveBeenCalledTimes(2);
  });

  it("keeps a locale the user picked over a persisted one arriving on a later activation", async () => {
    mocks.activationRevision.mockReturnValue(1);
    const view = render(<OnboardingApp />);
    await waitFor(() => expect(mocks.getSettings).toHaveBeenCalledTimes(1));

    fireEvent.click(await screen.findByRole("radio", { name: "עברית" }));
    await waitFor(() => expect(document.documentElement.lang).toBe("he"));

    mocks.getSettings.mockResolvedValueOnce({ ...baseSettings, locale: "en" });
    mocks.activationRevision.mockReturnValue(2);
    view.rerender(<OnboardingApp />);
    await waitFor(() => expect(mocks.getSettings).toHaveBeenCalledTimes(2));
    await act(async () => { await Promise.resolve(); });

    expect(document.documentElement.lang).toBe("he");
    expect(document.documentElement.dir).toBe("rtl");
    expect(screen.getByRole("radio", { name: "עברית" })).toBeChecked();
  });

  it("requires confirmed installation and login, recovers from cancellation, then permits completion", async () => {
    mocks.activationRevision.mockReturnValue(1);
    mocks.getSettings.mockResolvedValue({ ...baseSettings, enabledProviders: ["codex"] });
    const missing: ProviderSetupState = {
      ...providerStates[1], status: "notInstalled", errorKind: "missingExecutable", repairAction: "install",
    };
    const signedOut: ProviderSetupState = {
      ...providerStates[1], status: "notAuthenticated", errorKind: "authentication", repairAction: "login",
    };
    mocks.getProviderSetupStates.mockResolvedValue([missing]);
    mocks.installProvider.mockRejectedValueOnce(new Error("cancelled fixture install"))
      .mockResolvedValueOnce(signedOut);
    mocks.loginProvider.mockResolvedValue(providerStates[1]);
    render(<OnboardingApp />);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    const finish = await screen.findByRole("button", { name: "Finish setup" });
    expect(finish).toBeDisabled();
    expect(mocks.installProvider).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Install Codex" }));
    expect(mocks.installProvider).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(mocks.installProvider).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Install Codex" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm installation" }));
    expect(await screen.findByText("Provider setup needs attention.")).toBeInTheDocument();
    expect(finish).toBeDisabled();
    expect(mocks.completeOnboarding).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Install Codex" }));
    fireEvent.click(screen.getByRole("button", { name: "Confirm installation" }));
    const connect = await screen.findByRole("button", { name: "Connect Codex" });
    expect(finish).toBeDisabled();
    fireEvent.click(connect);
    expect(mocks.loginProvider).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Open official login" }));
    await waitFor(() => expect(finish).toBeEnabled());
    fireEvent.click(finish);
    await waitFor(() => expect(mocks.completeOnboarding)
      .toHaveBeenCalledExactlyOnceWith(["codex"], "en", false));
    expect(mocks.installProvider).toHaveBeenCalledTimes(2);
    expect(mocks.loginProvider).toHaveBeenCalledExactlyOnceWith("codex");
  });

  it("reloads and blocks a stale frontend success when native completion verification fails", async () => {
    mocks.activationRevision.mockReturnValue(1);
    mocks.getSettings.mockResolvedValue({ ...baseSettings, enabledProviders: ["codex"] });
    mocks.getProviderSetupStates.mockResolvedValueOnce([providerStates[1]])
      .mockResolvedValueOnce([{
        ...providerStates[1], status: "stale", errorKind: "authentication", repairAction: "login",
      }]);
    mocks.completeOnboarding.mockRejectedValueOnce("provider_setup_incomplete");
    render(<OnboardingApp />);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    const finish = await screen.findByRole("button", { name: "Finish setup" });
    fireEvent.click(finish);
    expect(await screen.findByText(/A selected provider could not be verified/)).toBeInTheDocument();
    await waitFor(() => expect(finish).toBeDisabled());
    expect(screen.getByRole("button", { name: "Connect Codex" })).toBeEnabled();
    expect(screen.queryByText("provider_setup_incomplete")).not.toBeInTheDocument();

    mocks.loginProvider.mockResolvedValue(providerStates[1]);
    fireEvent.click(screen.getByRole("button", { name: "Connect Codex" }));
    fireEvent.click(screen.getByRole("button", { name: "Open official login" }));
    await waitFor(() => expect(finish).toBeEnabled());
    fireEvent.click(finish);
    await waitFor(() => expect(mocks.completeOnboarding).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(mocks.emitLocaleChanged).toHaveBeenCalledExactlyOnceWith("en"));
  });

  it("restores deferred selections on reopening and allows completion after a verified repair", async () => {
    mocks.activationRevision.mockReturnValue(1);
    const deferredSettings = { ...baseSettings, enabledProviders: ["codex" as const] };
    mocks.getSettings.mockResolvedValue(deferredSettings);
    mocks.getProviderSetupStates.mockResolvedValue([{
      ...providerStates[1], status: "unavailable", errorKind: "network", repairAction: null,
    }]);
    mocks.completeOnboarding.mockResolvedValue(deferredSettings);
    const view = render(<OnboardingApp />);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    fireEvent.click(await screen.findByRole("button", { name: "Save and finish later" }));
    await waitFor(() => expect(mocks.completeOnboarding)
      .toHaveBeenCalledExactlyOnceWith(["codex"], "en", true));
    await waitFor(() => expect(mocks.emitLocaleChanged).toHaveBeenCalled());
    view.unmount();

    mocks.getProviderSetupStates.mockResolvedValue([providerStates[1]]);
    mocks.completeOnboarding.mockResolvedValue({ ...deferredSettings, onboardingCompleted: true });
    render(<OnboardingApp />);
    fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("checkbox", { name: "Use Codex in Dashy" })).toBeChecked();
    const finish = screen.getByRole("button", { name: "Finish setup" });
    expect(finish).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Save and finish later" })).not.toBeInTheDocument();
    fireEvent.click(finish);
    await waitFor(() => expect(mocks.completeOnboarding)
      .toHaveBeenLastCalledWith(["codex"], "en", false));
  });
});
