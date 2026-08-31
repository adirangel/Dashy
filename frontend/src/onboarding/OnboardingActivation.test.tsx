import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setLocale } from "../i18n";
import type { ProviderSetupState } from "../setup/api";
import type { AppSettings } from "../window";

const mocks = vi.hoisted(() => ({
  activationRevision: vi.fn(),
  getSettings: vi.fn(),
  completeOnboarding: vi.fn(),
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
].map(([provider, publisher, packageId, installUrl, loginCommand]) => ({
  definition: {
    provider: provider as "claude" | "codex" | "github",
    publisher,
    packageId,
    installCommand: `winget install --id ${packageId}`,
    installUrl,
    loginCommand,
  },
  status: "connected" as const,
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
});
