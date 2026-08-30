import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  isTauriRuntime: vi.fn(),
  isCurrentWindowActive: vi.fn(),
  listenForCurrentWindowActivation: vi.fn(),
  unlisten: vi.fn(),
}));

vi.mock("./window", () => ({
  isTauriRuntime: mocks.isTauriRuntime,
  isCurrentWindowActive: mocks.isCurrentWindowActive,
  listenForCurrentWindowActivation: mocks.listenForCurrentWindowActivation,
}));

import { useWindowActivationRevision } from "./useWindowActivation";

function Probe() {
  const revision = useWindowActivationRevision();
  return <output data-testid="revision">{revision}</output>;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((fulfill) => { resolve = fulfill; });
  return { promise, resolve };
}

describe("useWindowActivationRevision", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.isTauriRuntime.mockReturnValue(true);
    mocks.isCurrentWindowActive.mockResolvedValue(false);
    mocks.listenForCurrentWindowActivation.mockResolvedValue(mocks.unlisten);
  });

  afterEach(cleanup);

  it("activates browser tests immediately without native listeners", () => {
    mocks.isTauriRuntime.mockReturnValue(false);
    render(<Probe />);

    expect(screen.getByTestId("revision")).toHaveTextContent("1");
    expect(mocks.listenForCurrentWindowActivation).not.toHaveBeenCalled();
    expect(mocks.isCurrentWindowActive).not.toHaveBeenCalled();
  });

  it("keeps a hidden native window inactive until it gains focus", async () => {
    let activate: (() => void) | undefined;
    mocks.listenForCurrentWindowActivation.mockImplementation(async (handler) => {
      activate = handler;
      return mocks.unlisten;
    });
    const view = render(<Probe />);

    await waitFor(() => expect(mocks.isCurrentWindowActive).toHaveBeenCalledTimes(1));
    expect(screen.getByTestId("revision")).toHaveTextContent("0");

    act(() => activate?.());
    expect(screen.getByTestId("revision")).toHaveTextContent("1");
    view.unmount();
    expect(mocks.unlisten).toHaveBeenCalledTimes(1);
  });

  it("activates an already-visible native window after the listener is installed", async () => {
    const listener = deferred<() => void>();
    mocks.listenForCurrentWindowActivation.mockReturnValue(listener.promise);
    mocks.isCurrentWindowActive.mockResolvedValue(true);
    render(<Probe />);

    expect(mocks.isCurrentWindowActive).not.toHaveBeenCalled();
    listener.resolve(mocks.unlisten);

    await waitFor(() => expect(screen.getByTestId("revision")).toHaveTextContent("1"));
    expect(mocks.isCurrentWindowActive).toHaveBeenCalledTimes(1);
  });

  it("does not double-activate when focus wins the initial visibility race", async () => {
    const visibility = deferred<boolean>();
    let activate: (() => void) | undefined;
    mocks.listenForCurrentWindowActivation.mockImplementation(async (handler) => {
      activate = handler;
      return mocks.unlisten;
    });
    mocks.isCurrentWindowActive.mockReturnValue(visibility.promise);
    render(<Probe />);
    await waitFor(() => expect(mocks.isCurrentWindowActive).toHaveBeenCalledTimes(1));

    act(() => activate?.());
    visibility.resolve(true);

    await waitFor(() => expect(screen.getByTestId("revision")).toHaveTextContent("1"));
  });

  it("unlistens when registration completes after unmount and skips the initial query", async () => {
    const listener = deferred<() => void>();
    mocks.listenForCurrentWindowActivation.mockReturnValue(listener.promise);
    const view = render(<Probe />);
    view.unmount();

    listener.resolve(mocks.unlisten);
    await waitFor(() => expect(mocks.unlisten).toHaveBeenCalledTimes(1));
    expect(mocks.isCurrentWindowActive).not.toHaveBeenCalled();
  });
});
