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
  setTrayLabels: vi.fn(),
  emitLocaleChanged: vi.fn(),
  controller: vi.fn(),
}));

vi.mock("../window", () => ({
  getSettings: mocks.getSettings,
  completeOnboarding: mocks.completeOnboarding,
  setTrayLabels: mocks.setTrayLabels,
  emitLocaleChanged: mocks.emitLocaleChanged,
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

async function goToProviders() {
  fireEvent.click(await screen.findByRole("button", { name: "Continue" }));
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
    mocks.setTrayLabels.mockResolvedValue(undefined);
    mocks.emitLocaleChanged.mockResolvedValue(undefined);
    mocks.controller.mockReturnValue(controller());
  });

  afterEach(cleanup);

  it("opens on the language step, preselects the persisted locale, and switches live", async () => {
    mocks.getSettings.mockResolvedValue({ ...cleanSettings, locale: "fr" });

    render(<OnboardingApp />);

    const french = await screen.findByRole("radio", { name: "français" });
    await waitFor(() => expect(french).toBeChecked());
    expect(screen.queryByRole("checkbox", { name: /Dashy/ })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: "עברית" }));
    await waitFor(() => expect(document.documentElement.lang).toBe("he"));
    expect(document.documentElement.dir).toBe("rtl");
    expect(screen.getByRole("radio", { name: "עברית" })).toBeChecked();
  });

  it("round-trips between steps preserving locale and provider selections", async () => {
    mocks.controller.mockReturnValue(controller(states({
      claude: "connected",
      codex: "notInstalled",
      github: "connected",
    })));

    render(<OnboardingApp />);
    fireEvent.click(await screen.findByRole("radio", { name: "español" }));
    await waitFor(() => expect(document.documentElement.lang).toBe("es"));
    fireEvent.click(await screen.findByRole("button", { name: "Continuar" }));

    const claude = await screen.findByRole("checkbox", { name: "Usar Claude en Dashy" });
    expect(await screen.findByRole("heading", { level: 1 })).toHaveFocus();
    expect(claude).toBeChecked();
    fireEvent.click(claude);
    expect(claude).not.toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "Atrás" }));
    expect(await screen.findByRole("radio", { name: "español" })).toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "Continuar" }));
    expect(await screen.findByRole("checkbox", { name: "Usar Claude en Dashy" })).not.toBeChecked();
  });

  it("pushes localized tray labels before completing and reports the chosen locale", async () => {
    render(<OnboardingApp />);
    fireEvent.click(await screen.findByRole("radio", { name: "עברית" }));
    fireEvent.click(await screen.findByRole("button", { name: "המשך" }));
    fireEvent.click(await screen.findByRole("button", { name: "סיום ההגדרה" }));

    await waitFor(() => expect(mocks.completeOnboarding).toHaveBeenCalledExactlyOnceWith([], "he"));
    expect(mocks.setTrayLabels).toHaveBeenCalledTimes(1);
    expect(mocks.setTrayLabels.mock.calls[0][0].quit).toBe("צא מ־Dashy");
    expect(mocks.setTrayLabels.mock.invocationCallOrder[0])
      .toBeLessThan(mocks.completeOnboarding.mock.invocationCallOrder[0]);
    await waitFor(() => expect(mocks.emitLocaleChanged).toHaveBeenCalledExactlyOnceWith("he"));
  });

  it("finishes even when the tray label push fails", async () => {
    mocks.setTrayLabels.mockRejectedValue(new Error("tray offline"));

    render(<OnboardingApp />);
    await goToProviders();
    fireEvent.click(await screen.findByRole("button", { name: "Finish setup" }));

    await waitFor(() => expect(mocks.completeOnboarding).toHaveBeenCalledExactlyOnceWith([], "en"));
    expect(screen.queryByText(/tray offline/i)).not.toBeInTheDocument();
    expect(screen.getByText("", { selector: ".onboarding-footer-status" })).toBeInTheDocument();
  });

  it("preselects discovered connected providers but keeps every choice editable", async () => {
    mocks.controller.mockReturnValue(controller(states({
      claude: "connected",
      codex: "notInstalled",
      github: "connected",
    })));

    render(<OnboardingApp />);
    await goToProviders();

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
    await goToProviders();

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
    await goToProviders();

    expect(await screen.findByRole("checkbox", { name: "Use Claude in Dashy" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use Codex in Dashy" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use GitHub in Dashy" })).not.toBeChecked();
  });

  it("offers installation only for providers selected during setup", async () => {
    render(<OnboardingApp />);
    await goToProviders();

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
    await goToProviders();
    const finish = await screen.findByRole("button", { name: "Finish setup" });
    fireEvent.click(finish);
    fireEvent.click(finish);

    expect(finish).toBeDisabled();
    await waitFor(() => expect(mocks.completeOnboarding).toHaveBeenCalledExactlyOnceWith([], "en"));

    resolveCompletion({ ...cleanSettings, onboardingCompleted: true });
    await waitFor(() => expect(finish).not.toBeDisabled());
  });

  it("keeps the surface open with a localized message and never renders a raw save error", async () => {
    mocks.completeOnboarding.mockRejectedValue(new Error("secret filesystem path"));

    render(<OnboardingApp />);
    await goToProviders();
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
      await goToProviders();

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
