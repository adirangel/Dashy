import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProviderId, ProviderStatus } from "../dashboard";
import { setLocale } from "../i18n";
import type { ProviderSetupDefinition, ProviderSetupState } from "./api";

const apiMocks = vi.hoisted(() => ({
  getProviderSetupStates: vi.fn(),
  installProvider: vi.fn(),
  loginProvider: vi.fn(),
  openUrl: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: apiMocks.openUrl }));

vi.mock("./api", async (importOriginal) => {
  const original = await importOriginal<typeof import("./api")>();
  return {
    ...original,
    getProviderSetupStates: apiMocks.getProviderSetupStates,
    installProvider: apiMocks.installProvider,
    loginProvider: apiMocks.loginProvider,
  };
});

import { ProviderManager } from "./ProviderManager";
import { useProviderSetup } from "./useProviderSetup";

const mocks = {
  install: vi.fn<(provider: ProviderId) => Promise<void>>().mockResolvedValue(undefined),
  login: vi.fn<(provider: ProviderId) => Promise<void>>().mockResolvedValue(undefined),
  reload: vi.fn<() => Promise<void>>().mockResolvedValue(undefined),
  onEnabledChange: vi.fn<(providers: ProviderId[]) => void>(),
};

const metadata: Record<ProviderId, Omit<ProviderSetupDefinition, "provider">> = {
  claude: { publisher: "Anthropic", packageId: "Anthropic.ClaudeCode", installKind: "winget", installCommand: "winget install --id Anthropic.ClaudeCode --exact --source winget --interactive --accept-source-agreements --accept-package-agreements", installUrl: "https://code.claude.com/docs/en/setup", loginCommand: "claude auth login --claudeai" },
  codex: { publisher: "OpenAI", packageId: "OpenAI.Codex", installKind: "winget", installCommand: "winget install --id OpenAI.Codex --exact --source winget --interactive --accept-source-agreements --accept-package-agreements", installUrl: "https://learn.chatgpt.com/docs/codex/cli", loginCommand: "codex login" },
  github: { publisher: "GitHub", packageId: "GitHub.cli", installKind: "winget", installCommand: "winget install --id GitHub.cli --exact --source winget --interactive --accept-source-agreements --accept-package-agreements", installUrl: "https://cli.github.com/", loginCommand: "gh auth login --web" },
  grok: { publisher: "xAI", packageId: "xAI.GrokBuild", installKind: "winget", installCommand: "winget install --id xAI.GrokBuild --exact --source winget --interactive --accept-source-agreements --accept-package-agreements", installUrl: "https://docs.x.ai/build/overview", loginCommand: "grok login" },
  cursor: { publisher: "Anysphere", packageId: null, installKind: "manualUrl", installCommand: null, installUrl: "https://cursor.com/docs/cli/installation", loginCommand: "cursor-agent login" },
};

function setupStates(
  status: Partial<Record<ProviderId, ProviderStatus>> = {},
  repairAction: Partial<Record<ProviderId, "install" | "login" | null>> = {},
): ProviderSetupState[] {
  return (["claude", "codex", "github", "grok", "cursor"] as ProviderId[]).map((provider) => ({
    definition: { provider, ...metadata[provider] },
    status: status[provider] ?? "connected",
    repairAction: repairAction[provider]
      ?? (status[provider] === "notInstalled"
        ? "install"
        : status[provider] === "notAuthenticated" ? "login" : null),
  }));
}

function renderManager(options: {
  claudeStatus?: ProviderStatus;
  codexStatus?: ProviderStatus;
  githubStatus?: ProviderStatus;
  enabledProviders?: ProviderId[];
  busyProvider?: ProviderId | null;
  busyAction?: "install" | "login" | null;
  failureProvider?: ProviderId | null;
  states?: ProviderSetupState[] | null;
  loadFailed?: boolean;
  actionsRequireSelection?: boolean;
} = {}) {
  const status = {
    claude: options.claudeStatus ?? "connected",
    codex: options.codexStatus ?? "connected",
    github: options.githubStatus ?? "connected",
    grok: "connected",
    cursor: "connected",
  } satisfies Record<ProviderId, ProviderStatus>;
  const states = options.states === undefined
    ? setupStates(status)
    : options.states;
  render(<ProviderManager
    controller={{
      states,
      busyProvider: options.busyProvider ?? null,
      busyAction: options.busyAction ?? null,
      failureProvider: options.failureProvider ?? null,
      loadFailed: options.loadFailed ?? false,
      install: mocks.install,
      login: mocks.login,
      reload: mocks.reload,
    }}
    enabledProviders={options.enabledProviders ?? ["claude", "codex", "github", "grok", "cursor"]}
    onEnabledChange={mocks.onEnabledChange}
    actionsRequireSelection={options.actionsRequireSelection}
  />);
}

function HookProbe() {
  const controller = useProviderSetup();
  return <>
    <output data-testid="hook-state">{JSON.stringify({
      states: controller.states,
      busyProvider: controller.busyProvider,
      busyAction: controller.busyAction,
      failureProvider: controller.failureProvider,
      loadFailed: controller.loadFailed,
    })}</output>
    <button type="button" onClick={() => { void controller.install("codex"); }}>hook install</button>
    <button type="button" onClick={() => { void controller.login("claude"); }}>hook login</button>
    <button type="button" onClick={() => { void controller.reload(); }}>hook reload</button>
  </>;
}

function ProviderManagerHarness() {
  const controller = useProviderSetup();
  return <ProviderManager
    controller={controller}
    enabledProviders={["claude", "codex", "github"]}
    onEnabledChange={mocks.onEnabledChange}
  />;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((fulfill) => { resolve = fulfill; });
  return { promise, resolve };
}

describe("ProviderManager", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await setLocale("en");
    mocks.install.mockResolvedValue(undefined);
    mocks.login.mockResolvedValue(undefined);
    mocks.reload.mockResolvedValue(undefined);
    apiMocks.getProviderSetupStates.mockResolvedValue(setupStates());
    apiMocks.installProvider.mockResolvedValue(setupStates({ codex: "connected" })[1]);
    apiMocks.loginProvider.mockResolvedValue(setupStates({ claude: "connected" })[0]);
    apiMocks.openUrl.mockResolvedValue(undefined);
  });

  afterEach(async () => {
    cleanup();
    await setLocale("en");
  });

  it("shows the publisher and exact command before installation is confirmed", async () => {
    renderManager({ codexStatus: "notInstalled" });
    fireEvent.click(screen.getByRole("button", { name: "Install Codex" }));
    expect(screen.getByText("OpenAI.Codex")).toBeInTheDocument();
    expect(screen.getByText(/winget install --id OpenAI\.Codex/)).toBeInTheDocument();
    expect(mocks.install).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm installation" }));
    expect(mocks.install).toHaveBeenCalledExactlyOnceWith("codex");
  });

  it.each(["he", "ar"] as const)(
    "isolates exact package and command values from %s directionality",
    async (locale) => {
      await setLocale(locale);
      renderManager({ codexStatus: "notInstalled" });
      fireEvent.click(within(screen.getByRole("article", { name: "Codex" }))
        .getByRole("button"));

      for (const value of [
        screen.getByText("OpenAI.Codex"),
        screen.getByText(/winget install --id OpenAI\.Codex/),
      ]) {
        expect(value.tagName).toBe("BDI");
        expect(value).toHaveAttribute("dir", "ltr");
        expect(value).toHaveClass("provider-setup-technical-value");
        expect(value).toHaveStyle({ unicodeBidi: "isolate" });
      }
    },
  );

  it("requires a separate confirmation before login", async () => {
    renderManager({ claudeStatus: "notAuthenticated" });
    fireEvent.click(screen.getByRole("button", { name: "Connect Claude" }));
    expect(screen.getByText("claude auth login --claudeai")).toBeInTheDocument();
    expect(mocks.login).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Open official login" }));
    expect(mocks.login).toHaveBeenCalledExactlyOnceWith("claude");
  });

  it("lets every provider be enabled or skipped independently while preserving selection order", async () => {
    renderManager({ enabledProviders: ["codex"] });
    expect(screen.getByRole("checkbox", { name: "Use Codex in Dashy" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use Claude in Dashy" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Use GitHub in Dashy" })).not.toBeChecked();
    fireEvent.click(screen.getByRole("checkbox", { name: "Use GitHub in Dashy" }));
    expect(mocks.onEnabledChange).toHaveBeenLastCalledWith(["codex", "github"]);

    cleanup();
    renderManager({ enabledProviders: ["github", "codex"] });
    fireEvent.click(screen.getByRole("checkbox", { name: "Use GitHub in Dashy" }));
    expect(mocks.onEnabledChange).toHaveBeenLastCalledWith(["codex"]);
  });

  it("keeps only one labelled inline confirmation open and lets it be cancelled", () => {
    renderManager({ claudeStatus: "notAuthenticated", codexStatus: "notInstalled" });
    fireEvent.click(screen.getByRole("button", { name: "Install Codex" }));
    expect(screen.getByRole("group", { name: "Confirm installation" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Connect Claude" }));
    expect(screen.queryByRole("group", { name: "Confirm installation" })).not.toBeInTheDocument();
    expect(screen.getAllByRole("group")).toHaveLength(1);
    expect(screen.getByRole("group", { name: "Open official login" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("group")).not.toBeInTheDocument();
    expect(mocks.install).not.toHaveBeenCalled();
    expect(mocks.login).not.toHaveBeenCalled();
  });

  it("closes confirmation when setup deselects its provider", () => {
    const manager = {
      states: setupStates({ codex: "notInstalled" }),
      busyProvider: null,
      busyAction: null,
      failureProvider: null,
      loadFailed: false,
      install: mocks.install,
      login: mocks.login,
      reload: mocks.reload,
    };
    const view = render(<ProviderManager
      controller={manager}
      enabledProviders={["codex"]}
      onEnabledChange={mocks.onEnabledChange}
      actionsRequireSelection
    />);

    fireEvent.click(screen.getByRole("button", { name: "Install Codex" }));
    expect(screen.getByRole("group", { name: "Confirm installation" })).toBeInTheDocument();

    view.rerender(<ProviderManager
      controller={manager}
      enabledProviders={[]}
      onEnabledChange={mocks.onEnabledChange}
      actionsRequireSelection
    />);

    expect(screen.queryByRole("group", { name: "Confirm installation" })).not.toBeInTheDocument();
    expect(mocks.install).not.toHaveBeenCalled();
  });

  it("hides stale setup failures for deselected providers without changing settings mode", async () => {
    apiMocks.openUrl.mockRejectedValue(new Error("old opener failure"));
    const controller = {
      states: setupStates({ codex: "notInstalled" }),
      busyProvider: null,
      busyAction: null,
      failureProvider: "codex" as ProviderId,
      loadFailed: false,
      install: mocks.install,
      login: mocks.login,
      reload: mocks.reload,
    };
    const view = render(<ProviderManager
      controller={controller}
      enabledProviders={["codex"]}
      onEnabledChange={mocks.onEnabledChange}
      actionsRequireSelection
    />);
    const codexCard = screen.getByRole("article", { name: "Codex" });

    fireEvent.click(within(codexCard).getByRole("button", {
      name: "Open official installation guide",
    }));
    expect(await within(codexCard).findByText(
      "Dashy could not open the official installation guide.",
    )).toHaveAttribute("role", "alert");

    view.rerender(<ProviderManager
      controller={controller}
      enabledProviders={[]}
      onEnabledChange={mocks.onEnabledChange}
      actionsRequireSelection
    />);

    await waitFor(() => expect(within(codexCard).queryByRole("alert")).not.toBeInTheDocument());
    expect(within(codexCard).queryByRole("button", { name: "Install Codex" }))
      .not.toBeInTheDocument();
    expect(within(codexCard).queryByRole("button", {
      name: "Open official installation guide",
    })).not.toBeInTheDocument();

    view.rerender(<ProviderManager
      controller={controller}
      enabledProviders={[]}
      onEnabledChange={mocks.onEnabledChange}
    />);

    expect(within(codexCard).getByRole("alert"))
      .toHaveTextContent("Provider setup needs attention.");
    expect(within(codexCard).getByRole("button", { name: "Install Codex" }))
      .toBeEnabled();
    expect(within(codexCard).getByRole("button", {
      name: "Open official installation guide",
    })).toBeEnabled();
  });

  it("restores same-provider focus after keyboard cancel but preserves focus after pointer cancel", async () => {
    renderManager({ codexStatus: "notInstalled" });
    const action = screen.getByRole("button", { name: "Install Codex" });
    fireEvent.click(action);
    const keyboardCancel = screen.getByRole("button", { name: "Cancel" });
    keyboardCancel.focus();
    fireEvent.click(keyboardCancel, { detail: 0 });
    await waitFor(() => expect(screen.getByRole("checkbox", { name: "Use Codex in Dashy" }))
      .toHaveFocus());

    fireEvent.click(action);
    const retained = screen.getByRole("checkbox", { name: "Use GitHub in Dashy" });
    retained.focus();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }), { detail: 1 });
    expect(retained).toHaveFocus();
  });

  it("marks only the active provider card busy and disables setup actions across every card", () => {
    renderManager({
      codexStatus: "notInstalled", githubStatus: "stale",
      busyProvider: "codex", busyAction: "install",
    });
    const codexCard = screen.getByRole("article", { name: "Codex" });
    const githubCard = screen.getByRole("article", { name: "GitHub" });
    expect(codexCard).toHaveAttribute("aria-busy", "true");
    expect(githubCard).not.toHaveAttribute("aria-busy");
    expect(within(codexCard).getByRole("button", { name: "Install Codex" })).toBeDisabled();
    expect(within(githubCard).getByRole("button", { name: "Retry" })).toBeDisabled();
  });

  it("disables confirmation actions while any setup operation is active", () => {
    const providerStates = setupStates({
      claude: "notAuthenticated",
      codex: "notInstalled",
    });
    const controller = {
      states: providerStates,
      busyProvider: null as ProviderId | null,
      busyAction: null as "install" | "login" | null,
      failureProvider: null,
      loadFailed: false,
      install: mocks.install,
      login: mocks.login,
      reload: mocks.reload,
    };
    const view = render(<ProviderManager
      controller={controller}
      enabledProviders={["claude", "codex", "github"]}
      onEnabledChange={mocks.onEnabledChange}
    />);
    const claudeCard = screen.getByRole("article", { name: "Claude" });
    fireEvent.click(within(claudeCard).getByRole("button", { name: "Connect Claude" }));
    expect(within(claudeCard).getByRole("group")).toBeInTheDocument();

    view.rerender(<ProviderManager
      controller={{ ...controller, busyProvider: "codex", busyAction: "install" }}
      enabledProviders={["claude", "codex", "github"]}
      onEnabledChange={mocks.onEnabledChange}
    />);

    const confirmation = within(screen.getByRole("article", { name: "Claude" })).getByRole("group");
    expect(within(confirmation).getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(within(confirmation).getByRole("button", { name: "Open official login" })).toBeDisabled();
  });

  it.each([
    {
      action: "install" as const,
      provider: "codex" as const,
      initialStatus: "notInstalled" as const,
      actionLabel: "Install Codex",
      confirmationLabel: "Confirm installation",
      busyLabel: "Installing",
    },
    {
      action: "login" as const,
      provider: "claude" as const,
      initialStatus: "notAuthenticated" as const,
      actionLabel: "Connect Claude",
      confirmationLabel: "Open official login",
      busyLabel: "Connecting",
    },
  ])("announces the localized $action status and restores keyboard focus", async ({
    action, provider, initialStatus, actionLabel, confirmationLabel, busyLabel,
  }) => {
    const operation = deferred<ProviderSetupState>();
    apiMocks.getProviderSetupStates.mockResolvedValue(setupStates({ [provider]: initialStatus }));
    apiMocks[`${action}Provider`].mockReturnValue(operation.promise);
    render(<ProviderManagerHarness />);

    const actionButton = await screen.findByRole("button", { name: actionLabel });
    const card = screen.getByRole("article", { name: provider === "codex" ? "Codex" : "Claude" });
    const liveStatus = within(card).getByRole("status");
    fireEvent.click(actionButton);
    const confirmation = screen.getByRole("button", { name: confirmationLabel });
    confirmation.focus();
    expect(confirmation).toHaveFocus();
    fireEvent.keyDown(confirmation, { key: "Enter" });
    fireEvent.click(confirmation, { detail: 0 });

    expect(await within(card).findByRole("status")).toBe(liveStatus);
    expect(liveStatus).toHaveTextContent(busyLabel);
    expect(liveStatus).toHaveAttribute("aria-live", "polite");
    await waitFor(() => expect(within(card).getByRole("checkbox")).toHaveFocus());
    expect(document.body).not.toHaveFocus();

    operation.resolve(setupStates({ [provider]: "connected" })
      .find((state) => state.definition.provider === provider)!);
    await waitFor(() => expect(card).not.toHaveAttribute("aria-busy"));
    expect(within(card).getByRole("status")).toBe(liveStatus);
    expect(liveStatus).toHaveTextContent("Connected");
  });

  it("does not move focus after pointer confirmation", async () => {
    const operation = deferred<ProviderSetupState>();
    apiMocks.getProviderSetupStates.mockResolvedValue(setupStates({ codex: "notInstalled" }));
    apiMocks.installProvider.mockReturnValue(operation.promise);
    render(<ProviderManagerHarness />);

    fireEvent.click(await screen.findByRole("button", { name: "Install Codex" }));
    const retainedFocus = screen.getByRole("checkbox", { name: "Use GitHub in Dashy" });
    retainedFocus.focus();
    fireEvent.click(screen.getByRole("button", { name: "Confirm installation" }), { detail: 1 });

    expect(retainedFocus).toHaveFocus();
    operation.resolve(setupStates({ codex: "connected" })[1]);
    await waitFor(() => expect(apiMocks.installProvider).toHaveBeenCalledTimes(1));
  });

  it("opens the validated official manual-help URL through the native opener", async () => {
    renderManager({ codexStatus: "notInstalled", failureProvider: "codex" });
    const codexCard = screen.getByRole("article", { name: "Codex" });
    expect(within(codexCard).getByRole("alert")).toHaveTextContent("Provider setup needs attention.");
    fireEvent.click(within(codexCard).getByRole("button", {
      name: "Open official installation guide",
    }));
    await waitFor(() => expect(apiMocks.openUrl)
      .toHaveBeenCalledExactlyOnceWith("https://learn.chatgpt.com/docs/codex/cli"));
    expect(document.body.textContent).not.toContain("raw-secret");
  });

  it("renders a localized sanitized failure when the native guide opener rejects", async () => {
    apiMocks.openUrl.mockRejectedValue(new Error("shell raw-secret"));
    renderManager({ codexStatus: "notInstalled", failureProvider: "codex" });

    fireEvent.click(screen.getByRole("button", { name: "Open official installation guide" }));

    expect(await screen.findByText("Dashy could not open the official installation guide."))
      .toHaveAttribute("role", "alert");
    expect(document.body.textContent).not.toContain("shell raw-secret");
  });

  it("clears an old guide-opener failure when a new setup confirmation begins", async () => {
    apiMocks.openUrl.mockRejectedValue(new Error("old opener failure"));
    renderManager({ codexStatus: "notInstalled", failureProvider: "codex" });
    fireEvent.click(screen.getByRole("button", { name: "Open official installation guide" }));
    expect(await screen.findByText("Dashy could not open the official installation guide."))
      .toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Install Codex" }));

    expect(screen.queryByText("Dashy could not open the official installation guide."))
      .not.toBeInTheDocument();
    expect(screen.getByText("Provider setup needs attention.")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Confirm installation" })).toBeInTheDocument();
  });

  it.each([
    ["install", "Install GitHub"],
    ["login", "Connect GitHub"],
  ] as const)("offers a stale provider's explicit %s repair action", (repairAction, label) => {
    renderManager({
      states: setupStates({ github: "stale" }, { github: repairAction }),
    });

    const githubCard = screen.getByRole("article", { name: "GitHub" });
    expect(within(githubCard).getByRole("button", { name: label })).toBeEnabled();
    expect(within(githubCard).queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
  });

  it("reports a stale provider as connected while keeping its retry affordance", () => {
    renderManager({ githubStatus: "stale" });

    const githubCard = screen.getByRole("article", { name: "GitHub" });
    expect(githubCard).toHaveAttribute("data-status", "stale");
    expect(within(githubCard).getByRole("status")).toHaveTextContent("Connected");
    expect(within(githubCard).getByRole("button", { name: "Retry" })).toBeEnabled();
  });

  it("reports a stale provider with an authentication repair as connected with a login action", () => {
    renderManager({ states: setupStates({ github: "stale" }, { github: "login" }) });

    const githubCard = screen.getByRole("article", { name: "GitHub" });
    expect(within(githubCard).getByRole("status")).toHaveTextContent("Connected");
    expect(within(githubCard).getByRole("button", { name: "Connect GitHub" })).toBeEnabled();
  });

  it("confirms a manual-URL install by opening the official guide without any command", () => {
    apiMocks.openUrl.mockResolvedValue(undefined);
    renderManager({ states: setupStates({ cursor: "notInstalled" }, { cursor: "install" }) });

    const cursorCard = screen.getByRole("article", { name: "Cursor" });
    fireEvent.click(within(cursorCard).getByRole("button", { name: "Install Cursor" }));

    const confirmation = within(cursorCard).getByRole("group", {
      name: "Open official installation guide",
    });
    expect(within(confirmation).getByText("Dashy will open the official install guide in your browser.")).toBeInTheDocument();
    expect(within(confirmation).queryByText(/winget/)).not.toBeInTheDocument();
    expect(within(confirmation).queryByText("Package")).not.toBeInTheDocument();

    fireEvent.click(within(confirmation).getByRole("button", {
      name: "Open official installation guide",
    }), { detail: 1 });

    expect(apiMocks.openUrl).toHaveBeenCalledExactlyOnceWith("https://cursor.com/docs/cli/installation");
    expect(mocks.install).not.toHaveBeenCalled();
  });

  it("surfaces a manual-help failure when the official guide cannot open", async () => {
    apiMocks.openUrl.mockRejectedValue(new Error("opener offline"));
    renderManager({ states: setupStates({ cursor: "notInstalled" }, { cursor: "install" }) });

    const cursorCard = screen.getByRole("article", { name: "Cursor" });
    fireEvent.click(within(cursorCard).getByRole("button", { name: "Install Cursor" }));
    fireEvent.click(within(cursorCard).getByRole("button", {
      name: "Open official installation guide",
    }), { detail: 1 });

    expect(await within(cursorCard).findByRole("alert"))
      .toHaveTextContent("Dashy could not open the official installation guide.");
    expect(document.body.textContent).not.toContain("opener offline");
  });

  it.each(["stale", "unavailable"] as const)(
    "retries a %s provider without opening login consent",
    (providerStatus) => {
      renderManager({ githubStatus: providerStatus });
      fireEvent.click(within(screen.getByRole("article", { name: "GitHub" }))
        .getByRole("button", { name: "Retry" }));
      expect(mocks.reload).toHaveBeenCalledTimes(1);
      expect(screen.queryByRole("group")).not.toBeInTheDocument();
      expect(mocks.login).not.toHaveBeenCalled();
    },
  );

  it("renders a sanitized, retryable load failure before definitions are available", () => {
    renderManager({ states: null, loadFailed: true });
    expect(screen.getByRole("alert")).toHaveTextContent("Provider setup needs attention.");
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(mocks.reload).toHaveBeenCalledTimes(1);
    expect(document.body.textContent).not.toContain("raw rejected setup details");
  });

  it("renders stable provider order and associates each card, status, selection, and confirmation", () => {
    renderManager({ states: setupStates({ claude: "notAuthenticated" }).reverse() });
    expect(screen.getAllByRole("article").map((card) =>
      within(card).getByRole("heading").textContent
    )).toEqual(["Claude", "Codex", "GitHub", "Grok", "Cursor"]);
    const claudeCard = screen.getByRole("article", { name: "Claude" });
    expect(claudeCard).toHaveAttribute("data-provider", "claude");
    expect(claudeCard).toHaveAttribute("data-status", "notAuthenticated");
    expect(within(claudeCard).getByText("Sign in required")).toHaveClass("provider-setup-status");
    expect(within(claudeCard).getByText("Use Claude in Dashy").closest("label"))
      .toHaveClass("provider-setup-selection");

    fireEvent.click(within(claudeCard).getByRole("button", { name: "Connect Claude" }));
    const confirmation = within(claudeCard).getByRole("group", { name: "Open official login" });
    expect(confirmation).toHaveClass("provider-setup-confirmation");
    expect(within(confirmation).getByRole("button", { name: "Cancel" }))
      .toHaveClass("provider-setup-confirmation-cancel");
    expect(within(confirmation).getByRole("button", { name: "Open official login" }))
      .toHaveClass("provider-setup-confirmation-primary");
  });
});

describe("useProviderSetup", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    await setLocale("en");
    apiMocks.getProviderSetupStates.mockResolvedValue(setupStates());
    apiMocks.installProvider.mockResolvedValue(setupStates({ codex: "connected" })[1]);
    apiMocks.loginProvider.mockResolvedValue(setupStates({ claude: "connected" })[0]);
  });

  afterEach(async () => {
    cleanup();
    await setLocale("en");
  });

  it("loads provider states on mount and replaces an action result by provider ID", async () => {
    apiMocks.getProviderSetupStates.mockResolvedValue(setupStates({ codex: "notInstalled" }));
    render(<HookProbe />);
    await waitFor(() => expect(screen.getByTestId("hook-state")).toHaveTextContent("notInstalled"));

    fireEvent.click(screen.getByRole("button", { name: "hook install" }));
    await waitFor(() => expect(apiMocks.installProvider).toHaveBeenCalledExactlyOnceWith("codex"));
    await waitFor(() => expect(screen.getByTestId("hook-state")).not.toHaveTextContent("notInstalled"));
    expect(screen.getByTestId("hook-state")).toHaveTextContent('"busyProvider":null');
  });

  it("tracks only the provider ID when an action rejects and clears busy state", async () => {
    apiMocks.installProvider.mockRejectedValue(new Error("raw-secret-install-error"));
    render(<HookProbe />);
    await waitFor(() => expect(screen.getByTestId("hook-state")).toHaveTextContent("connected"));

    fireEvent.click(screen.getByRole("button", { name: "hook install" }));
    await waitFor(() => expect(screen.getByTestId("hook-state")).toHaveTextContent('"failureProvider":"codex"'));
    expect(screen.getByTestId("hook-state")).toHaveTextContent('"busyProvider":null');
    expect(document.body.textContent).not.toContain("raw-secret-install-error");
  });

  it("serializes provider actions until the active promise settles", async () => {
    const install = deferred<ProviderSetupState>();
    const login = deferred<ProviderSetupState>();
    apiMocks.installProvider.mockReturnValue(install.promise);
    apiMocks.loginProvider.mockReturnValue(login.promise);
    render(<HookProbe />);
    await waitFor(() => expect(screen.getByTestId("hook-state")).toHaveTextContent("connected"));

    fireEvent.click(screen.getByRole("button", { name: "hook install" }));
    await waitFor(() => expect(screen.getByTestId("hook-state"))
      .toHaveTextContent('"busyProvider":"codex"'));
    fireEvent.click(screen.getByRole("button", { name: "hook login" }));

    expect(apiMocks.loginProvider).not.toHaveBeenCalled();
    expect(screen.getByTestId("hook-state")).toHaveTextContent('"busyProvider":"codex"');

    install.resolve(setupStates({ codex: "connected" })[1]);
    await waitFor(() => expect(screen.getByTestId("hook-state"))
      .toHaveTextContent('"busyProvider":null'));

    fireEvent.click(screen.getByRole("button", { name: "hook login" }));
    await waitFor(() => expect(apiMocks.loginProvider).toHaveBeenCalledExactlyOnceWith("claude"));
    expect(screen.getByTestId("hook-state")).toHaveTextContent('"busyProvider":"claude"');

    login.resolve(setupStates({ claude: "connected" })[0]);
    await waitFor(() => expect(screen.getByTestId("hook-state"))
      .toHaveTextContent('"busyProvider":null'));
  });

  it("reports a sanitized load failure and clears it after a successful retry", async () => {
    apiMocks.getProviderSetupStates
      .mockRejectedValueOnce(new Error("raw rejected setup details"))
      .mockResolvedValueOnce(setupStates());
    render(<HookProbe />);
    await waitFor(() => expect(screen.getByTestId("hook-state")).toHaveTextContent('"loadFailed":true'));
    expect(document.body.textContent).not.toContain("raw rejected setup details");

    fireEvent.click(screen.getByRole("button", { name: "hook reload" }));
    await waitFor(() => expect(screen.getByTestId("hook-state")).toHaveTextContent('"loadFailed":false'));
    expect(apiMocks.getProviderSetupStates).toHaveBeenCalledTimes(2);
  });
});
