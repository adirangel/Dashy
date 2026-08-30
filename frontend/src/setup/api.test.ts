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
