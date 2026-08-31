import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StrictMode } from "react";
import type { ProviderSetupState } from "./api";

const apiMocks = vi.hoisted(() => ({
  getProviderSetupStates: vi.fn(),
  installProvider: vi.fn(),
  loginProvider: vi.fn(),
}));

vi.mock("./api", async (importOriginal) => ({
  ...await importOriginal<typeof import("./api")>(),
  getProviderSetupStates: apiMocks.getProviderSetupStates,
  installProvider: apiMocks.installProvider,
  loginProvider: apiMocks.loginProvider,
}));

import { useProviderSetup } from "./useProviderSetup";

const codexState = (status: ProviderSetupState["status"]): ProviderSetupState => ({
  definition: {
    provider: "codex",
    publisher: "OpenAI",
    packageId: "OpenAI.Codex",
    installCommand: "winget install --id OpenAI.Codex",
    installUrl: "https://learn.chatgpt.com/docs/codex/cli",
    loginCommand: "codex login",
  },
  status,
  repairAction: status === "notInstalled" ? "install" : null,
});

function Probe({ activationRevision }: { activationRevision: number }) {
  const controller = useProviderSetup(activationRevision);
  return <>
    <output data-testid="state">{JSON.stringify({
      states: controller.states,
      busyProvider: controller.busyProvider,
      failureProvider: controller.failureProvider,
      loadFailed: controller.loadFailed,
    })}</output>
    <button type="button" onClick={() => { void controller.install("codex"); }}>install</button>
  </>;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((fulfill, fail) => {
    resolve = fulfill;
    reject = fail;
  });
  return { promise, resolve, reject };
}

describe("useProviderSetup activation", () => {
  beforeEach(() => {
    apiMocks.getProviderSetupStates.mockReset();
    apiMocks.installProvider.mockReset();
    apiMocks.loginProvider.mockReset();
    apiMocks.getProviderSetupStates.mockResolvedValue([codexState("connected")]);
    apiMocks.installProvider.mockResolvedValue(codexState("connected"));
    apiMocks.loginProvider.mockResolvedValue(codexState("connected"));
  });

  afterEach(cleanup);

  it("does not discover providers until the window has activated", async () => {
    const view = render(<Probe activationRevision={0} />);
    await Promise.resolve();
    expect(apiMocks.getProviderSetupStates).not.toHaveBeenCalled();

    view.rerender(<Probe activationRevision={1} />);
    await waitFor(() => expect(apiMocks.getProviderSetupStates).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId("state")).toHaveTextContent("connected");
  });

  it("remains live after React Strict Mode's development cleanup cycle", async () => {
    render(<StrictMode><Probe activationRevision={1} /></StrictMode>);

    await waitFor(() => expect(screen.getByTestId("state")).toHaveTextContent("connected"));
  });

  it("reloads discovery on each later show or focus activation", async () => {
    apiMocks.getProviderSetupStates
      .mockResolvedValueOnce([codexState("notInstalled")])
      .mockResolvedValueOnce([codexState("connected")]);
    const view = render(<Probe activationRevision={1} />);
    await waitFor(() => expect(screen.getByTestId("state")).toHaveTextContent("notInstalled"));

    view.rerender(<Probe activationRevision={2} />);

    await waitFor(() => expect(screen.getByTestId("state")).toHaveTextContent("connected"));
    expect(apiMocks.getProviderSetupStates).toHaveBeenCalledTimes(2);
  });

  it("keeps the newest activation result when an older discovery resolves late", async () => {
    const first = deferred<ProviderSetupState[]>();
    apiMocks.getProviderSetupStates
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce([codexState("connected")]);
    const view = render(<Probe activationRevision={1} />);
    await waitFor(() => expect(apiMocks.getProviderSetupStates).toHaveBeenCalledTimes(1));

    view.rerender(<Probe activationRevision={2} />);
    await waitFor(() => expect(apiMocks.getProviderSetupStates).toHaveBeenCalledTimes(2));
    await waitFor(() => expect(screen.getByTestId("state")).toHaveTextContent("connected"));
    first.resolve([codexState("notInstalled")]);

    await waitFor(() => expect(screen.getByTestId("state")).not.toHaveTextContent("notInstalled"));
  });

  it("reconciles provider states after an action rejects and keeps only sanitized failure state", async () => {
    apiMocks.getProviderSetupStates
      .mockResolvedValueOnce([codexState("notInstalled")])
      .mockResolvedValueOnce([codexState("connected")]);
    apiMocks.installProvider.mockRejectedValue(new Error("raw native setup failure"));
    render(<Probe activationRevision={1} />);
    await waitFor(() => expect(screen.getByTestId("state")).toHaveTextContent("notInstalled"));

    fireEvent.click(screen.getByRole("button", { name: "install" }));

    await waitFor(() => expect(screen.getByTestId("state")).toHaveTextContent("connected"));
    expect(screen.getByTestId("state")).toHaveTextContent('"failureProvider":"codex"');
    expect(apiMocks.getProviderSetupStates).toHaveBeenCalledTimes(2);
    expect(document.body.textContent).not.toContain("raw native setup failure");
  });
});
