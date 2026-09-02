import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import { getProviderSetupStates, installProvider, loginProvider } from "./api";

const codex = {
  definition: {
    provider: "codex", publisher: "OpenAI", packageId: "OpenAI.Codex",
    installKind: "winget",
    installCommand: "winget install --id OpenAI.Codex --exact --source winget --interactive --accept-source-agreements --accept-package-agreements",
    installUrl: "https://learn.chatgpt.com/docs/codex/cli",
    loginCommand: "codex login",
  },
  status: "notInstalled",
  repairAction: "install",
};

const claudeOnMac = {
  definition: {
    provider: "claude", publisher: "Anthropic", packageId: "claude-code",
    installKind: "homebrew",
    installCommand: "brew install --cask claude-code",
    installUrl: "https://code.claude.com/docs/en/setup",
    loginCommand: "claude auth login --claudeai",
  },
  status: "notInstalled",
  repairAction: "install",
};

const cursor = {
  definition: {
    provider: "cursor", publisher: "Anysphere", packageId: null,
    installKind: "manualUrl",
    installCommand: null,
    installUrl: "https://cursor.com/docs/cli/installation",
    loginCommand: "cursor-agent login",
  },
  status: "notInstalled",
  repairAction: "install",
};

describe("provider setup IPC", () => {
  beforeEach(() => mocks.invoke.mockReset());

  it("accepts the exact setup contract", async () => {
    mocks.invoke.mockResolvedValue([codex, claudeOnMac, cursor]);
    await expect(getProviderSetupStates()).resolves.toEqual([codex, claudeOnMac, cursor]);
  });

  it.each([
    ["a homebrew kind without a package id", (definition: Record<string, unknown>) => { definition.packageId = null; }],
    ["a homebrew kind without an install command", (definition: Record<string, unknown>) => {
      definition.installCommand = null;
    }],
  ] as const)("rejects %s before it reaches the UI", async (_case, mutate) => {
    const definition = { ...claudeOnMac.definition } as Record<string, unknown>;
    mutate(definition);
    mocks.invoke.mockResolvedValue([{ ...claudeOnMac, definition }]);

    await expect(getProviderSetupStates()).rejects.toThrow(/invalid provider setup response/i);
  });

  it.each([
    ["a missing install kind", (definition: Record<string, unknown>) => { delete definition.installKind; }],
    ["an unknown install kind", (definition: Record<string, unknown>) => { definition.installKind = "script"; }],
    ["a winget kind without a package id", (definition: Record<string, unknown>) => { definition.packageId = null; }],
    ["a manual kind claiming a package id", (definition: Record<string, unknown>) => {
      definition.installKind = "manualUrl";
    }],
    ["a winget kind without an install command", (definition: Record<string, unknown>) => {
      definition.installCommand = null;
    }],
    ["a manual kind claiming an install command", (definition: Record<string, unknown>) => {
      definition.installKind = "manualUrl";
      definition.packageId = null;
    }],
  ] as const)("rejects %s before it reaches the UI", async (_case, mutate) => {
    const definition = { ...codex.definition } as Record<string, unknown>;
    mutate(definition);
    mocks.invoke.mockResolvedValue([{ ...codex, definition }]);

    await expect(getProviderSetupStates()).rejects.toThrow(/invalid provider setup response/i);
  });

  it("rejects extra native fields before they reach the UI", async () => {
    mocks.invoke.mockResolvedValue([{ ...codex, token: "secret" }]);
    await expect(getProviderSetupStates()).rejects.toThrow(/invalid provider setup response/i);
  });

  it.each([
    { ...codex, repairAction: undefined },
    { ...codex, repairAction: "repair" },
    { ...codex, repairAction: 1 },
  ])("rejects a missing or invalid repair action before it reaches the UI", async (payload) => {
    const response = { ...payload } as Record<string, unknown>;
    if (payload.repairAction === undefined) delete response.repairAction;
    mocks.invoke.mockResolvedValue([response]);

    await expect(getProviderSetupStates()).rejects.toThrow(/invalid provider setup response/i);
  });

  it("accepts an explicit null repair action", async () => {
    mocks.invoke.mockResolvedValue([{ ...codex, status: "connected", repairAction: null }]);

    await expect(getProviderSetupStates()).resolves.toEqual([
      { ...codex, status: "connected", repairAction: null },
    ]);
  });

  it("rejects a provider guide URL outside the native allowlist", async () => {
    mocks.invoke.mockResolvedValue([{
      ...codex,
      definition: { ...codex.definition, installUrl: "https://example.invalid/codex" },
    }]);

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

  it.each([
    ["install", installProvider],
    ["login", loginProvider],
  ] as const)("rejects a cross-provider %s response", async (_action, runAction) => {
    mocks.invoke.mockResolvedValue({
      ...codex,
      definition: { ...codex.definition, provider: "claude" },
      status: "connected",
    });

    await expect(runAction("codex")).rejects.toThrow(/invalid provider setup response/i);
  });
});
