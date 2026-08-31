import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderId, ProviderStatus } from "../dashboard";
import { setLocale } from "../i18n";
import type { ProviderSetupDefinition, ProviderSetupState } from "../setup/api";
import type { AppSettings } from "../window";
import "../onboarding.css";

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
vi.mock("../useWindowActivation", () => ({ useWindowActivationRevision: () => 1 }));

import { OnboardingApp } from "./OnboardingApp";

const metadata: Record<ProviderId, Omit<ProviderSetupDefinition, "provider">> = {
  claude: {
    publisher: "Anthropic",
    packageId: "Anthropic.ClaudeCode",
    installCommand: "winget install --id Anthropic.ClaudeCode --exact --source winget --interactive --accept-source-agreements --accept-package-agreements",
    installUrl: "https://code.claude.com/docs/en/setup",
    loginCommand: "claude auth login --claudeai",
  },
  codex: {
    publisher: "OpenAI",
    packageId: "OpenAI.Codex",
    installCommand: "winget install --id OpenAI.Codex --exact --source winget --interactive --accept-source-agreements --accept-package-agreements",
    installUrl: "https://learn.chatgpt.com/docs/codex/cli",
    loginCommand: "codex login",
  },
  github: {
    publisher: "GitHub",
    packageId: "GitHub.cli",
    installCommand: "winget install --id GitHub.cli --exact --source winget --interactive --accept-source-agreements --accept-package-agreements",
    installUrl: "https://cli.github.com/",
    loginCommand: "gh auth login --web",
  },
};

const cleanSettings: AppSettings = {
  placement: "right",
  monitor: null,
  locale: "en",
  alwaysShowOverFullscreen: false,
  onboardingCompleted: false,
  enabledProviders: [],
};

function states(status: Record<ProviderId, ProviderStatus>): ProviderSetupState[] {
  return (["claude", "codex", "github"] as ProviderId[]).map((provider) => ({
    definition: { provider, ...metadata[provider] },
    status: status[provider],
    repairAction: status[provider] === "notInstalled"
      ? "install"
      : status[provider] === "notAuthenticated" ? "login" : null,
  }));
}

function controller(providerStates = states({
  claude: "notInstalled",
  codex: "notInstalled",
  github: "notInstalled",
})) {
  return {
    states: providerStates,
    busyProvider: null,
    busyAction: null,
    failureProvider: null,
    loadFailed: false,
    reload: vi.fn(),
    install: vi.fn(),
    login: vi.fn(),
  };
}

describe("OnboardingApp", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await setLocale("en");
    mocks.getSettings.mockResolvedValue(cleanSettings);
    mocks.completeOnboarding.mockResolvedValue({
      ...cleanSettings,
      onboardingCompleted: true,
    });
    mocks.controller.mockReturnValue(controller());
  });

  afterEach(cleanup);

  it("preselects discovered connected providers but keeps every choice editable", async () => {
    mocks.controller.mockReturnValue(controller(states({
      claude: "connected",
      codex: "notInstalled",
      github: "connected",
    })));

    render(<OnboardingApp />);

    const claude = await screen.findByRole("checkbox", { name: "Use Claude in Dashy" });
    const codex = screen.getByRole("checkbox", { name: "Use Codex in Dashy" });
    const github = screen.getByRole("checkbox", { name: "Use GitHub in Dashy" });
    expect(claude).toBeChecked();
    expect(codex).not.toBeChecked();
    expect(github).toBeChecked();

    fireEvent.click(claude);
    fireEvent.click(codex);
    expect(claude).not.toBeChecked();
    expect(codex).toBeChecked();
  });

  it("preserves a saved explicit provider selection over discovery", async () => {
    mocks.getSettings.mockResolvedValue({
      ...cleanSettings,
      enabledProviders: ["codex"],
    });
    mocks.controller.mockReturnValue(controller(states({
      claude: "connected",
      codex: "notInstalled",
      github: "connected",
    })));

    render(<OnboardingApp />);

    expect(await screen.findByRole("checkbox", { name: "Use Claude in Dashy" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use Codex in Dashy" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use GitHub in Dashy" })).not.toBeChecked();
  });

  it("preserves an explicitly empty legacy selection during the one-time setup review", async () => {
    mocks.getSettings.mockResolvedValue({
      ...cleanSettings,
      onboardingCompleted: true,
      enabledProviders: [],
    });
    mocks.controller.mockReturnValue(controller(states({
      claude: "connected",
      codex: "connected",
      github: "connected",
    })));

    render(<OnboardingApp />);

    expect(await screen.findByRole("checkbox", { name: "Use Claude in Dashy" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use Codex in Dashy" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use GitHub in Dashy" })).not.toBeChecked();
  });

  it("offers installation only for providers selected during setup", async () => {
    render(<OnboardingApp />);

    const claude = await screen.findByRole("checkbox", { name: "Use Claude in Dashy" });
    expect(screen.queryByRole("button", { name: "Install Claude" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install Codex" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install GitHub" })).not.toBeInTheDocument();

    fireEvent.click(claude);

    expect(screen.getByRole("button", { name: "Install Claude" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Install Codex" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install GitHub" })).not.toBeInTheDocument();
  });

  it("can finish with no providers and persists the exact empty selection once", async () => {
    let resolveCompletion!: (settings: AppSettings) => void;
    mocks.completeOnboarding.mockReturnValue(new Promise((resolve) => {
      resolveCompletion = resolve;
    }));

    render(<OnboardingApp />);
    const finish = await screen.findByRole("button", { name: "Finish setup" });
    fireEvent.click(finish);
    fireEvent.click(finish);

    expect(finish).toBeDisabled();
    expect(mocks.completeOnboarding).toHaveBeenCalledExactlyOnceWith([]);

    resolveCompletion({ ...cleanSettings, onboardingCompleted: true });
    await waitFor(() => expect(finish).not.toBeDisabled());
  });

  it("keeps the surface open with a localized message and never renders a raw save error", async () => {
    mocks.completeOnboarding.mockRejectedValue(new Error("secret filesystem path"));

    render(<OnboardingApp />);
    fireEvent.click(await screen.findByRole("button", { name: "Finish setup" }));

    expect(await screen.findByText(
      "Dashy could not save your provider selection.",
      { selector: ".onboarding-footer-status" },
    )).toHaveAttribute("role", "status");
    expect(screen.queryByText(/secret filesystem path/i)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Finish setup" })).toBeEnabled();
  });

  it("keeps expanded consent and Finish reachable in the same 520 by 560 scroll owner", async () => {
    const previousWidth = window.innerWidth;
    const previousHeight = window.innerHeight;
    Object.defineProperty(window, "innerWidth", { configurable: true, value: 520 });
    Object.defineProperty(window, "innerHeight", { configurable: true, value: 560 });
    try {
      render(<OnboardingApp />);

      const surface = await screen.findByTestId("onboarding-scroll-surface");
      const finish = await screen.findByRole("button", { name: "Finish setup" });
      const consent = screen.getByRole("checkbox", { name: "Use GitHub in Dashy" });
      fireEvent.click(consent);
      fireEvent.click(screen.getByRole("button", { name: "Install GitHub" }));
      const confirmation = screen.getByRole("group", { name: "Confirm installation" });
      const style = getComputedStyle(surface);

      expect(surface).toHaveAttribute("tabindex", "0");
      expect(surface).toHaveAttribute("data-scroll-owner", "onboarding");
      expect(surface).toContainElement(consent);
      expect(surface).toContainElement(confirmation);
      expect(surface).toContainElement(finish);
      expect(style.height).toBe("100vh");
      expect(style.minHeight).toBe("0px");
      expect(style.overflowY).toBe("auto");
    } finally {
      Object.defineProperty(window, "innerWidth", { configurable: true, value: previousWidth });
      Object.defineProperty(window, "innerHeight", { configurable: true, value: previousHeight });
    }
  });
});
