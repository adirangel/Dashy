import "@testing-library/jest-dom/vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DashboardSnapshot, ProviderId } from "../dashboard";
import type { EdgeViewState } from "../window";

const mocks = vi.hoisted(() => ({
  getSettings: vi.fn(), getCurrentEdgeView: vi.fn(), isTauriRuntime: vi.fn(), listenForEdgeView: vi.fn(),
  beginNotchExit: vi.fn(), completeNotchExit: vi.fn(),
  createExitToken: vi.fn(),
  setNotchInteraction: vi.fn(), showNotchMenu: vi.fn(), unlisten: vi.fn(),
  refreshProvider: vi.fn(),
  dashboard: {
    snapshot: null as DashboardSnapshot | null,
    refreshing: false,
    refreshingProviders: new Set<ProviderId>() as ReadonlySet<ProviderId>,
    refreshFailures: new Set<ProviderId>() as ReadonlySet<ProviderId>,
  },
}));

vi.mock("../window", () => ({
  getSettings: mocks.getSettings, isTauriRuntime: mocks.isTauriRuntime,
  getCurrentEdgeView: mocks.getCurrentEdgeView,
  beginNotchExit: mocks.beginNotchExit, completeNotchExit: mocks.completeNotchExit,
  createExitToken: mocks.createExitToken,
  listenForEdgeView: mocks.listenForEdgeView,
  setNotchInteraction: mocks.setNotchInteraction, showNotchMenu: mocks.showNotchMenu,
}));
vi.mock("../useDashboardSnapshot", () => ({
  useDashboardSnapshot: () => ({ ...mocks.dashboard, refreshProvider: mocks.refreshProvider }),
}));

import { NotchApp } from "./NotchApp";

const usage = {
  status: "connected" as const, remainingPercent: 59,
  shortWindow: { labelKey: "short" as const, remainingPercent: 83, resetsAt: null },
  weeklyWindow: { labelKey: "weekly" as const, remainingPercent: 59, resetsAt: null },
  lastSuccessfulRefresh: "2026-08-29T09:00:00Z", errorKind: null,
};
const snapshot: DashboardSnapshot = {
  github: { status: "connected", accountLogin: "fixture-user", contributionDays: [], currentStreakDays: 4, lastSuccessfulRefresh: "2026-08-29T09:00:00Z", errorKind: null },
  codex: { ...usage, remainingPercent: 68 }, claude: usage, refreshedAt: "2026-08-29T09:00:00Z",
};
let edgeHandler: ((view: EdgeViewState) => void) | undefined;
let exitTokenSequence = 0;
async function emitEdgeView(view: EdgeViewState) {
  await act(async () => { edgeHandler?.(view); await Promise.resolve(); });
}
async function renderNativeNotch() {
  const view = render(<NotchApp snapshot={snapshot} />);
  await waitFor(() => expect(edgeHandler).toBeTypeOf("function"));
  return view;
}
function fireSurfaceAnimationEnd(element: Element) {
  fireEvent(element, new Event("webkitAnimationEnd", { bubbles: true }));
}

describe("native notch interaction bridge", () => {
  beforeEach(() => {
    vi.clearAllMocks(); edgeHandler = undefined;
    exitTokenSequence = 0;
    mocks.isTauriRuntime.mockReturnValue(true);
    mocks.getSettings.mockResolvedValue({ placement: "right", monitor: null, locale: "en", alwaysShowOverFullscreen: false });
    mocks.getCurrentEdgeView.mockResolvedValue({ visibility: "hidden", placement: "right", provider: null });
    mocks.listenForEdgeView.mockImplementation(async (handler: (view: EdgeViewState) => void) => { edgeHandler = handler; return mocks.unlisten; });
    mocks.setNotchInteraction.mockResolvedValue(undefined);
    mocks.beginNotchExit.mockResolvedValue(true);
    mocks.completeNotchExit.mockResolvedValue(true);
    mocks.createExitToken.mockImplementation(() => `exit-test-${++exitTokenSequence}`);
    mocks.showNotchMenu.mockResolvedValue(undefined);
    mocks.refreshProvider.mockResolvedValue("success");
    mocks.dashboard.snapshot = snapshot;
    mocks.dashboard.refreshingProviders = new Set();
    mocks.dashboard.refreshFailures = new Set();
  });
  afterEach(cleanup);

  it("renders a backend rail view without mounting a provider card", async () => {
    await renderNativeNotch();
    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });
    expect(screen.getByRole("toolbar", { name: /provider status/i })).toBeInTheDocument();
    expect(screen.queryByRole("article")).not.toBeInTheDocument();
    expect(screen.getByTestId("notch-surface"))
      .toHaveAttribute("data-logical-size", "70x270");
    expect(screen.getByTestId("notch-surface")).not.toHaveClass("is-expanded");
  });

  it("never calls DOM focus for a passive hidden-to-rail proximity reveal", async () => {
    const focus = vi.spyOn(HTMLElement.prototype, "focus");
    await renderNativeNotch();
    focus.mockClear();

    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });
    await act(async () => { await Promise.resolve(); });

    expect(focus).not.toHaveBeenCalled();
    focus.mockRestore();
  });

  it("focuses the selected metric when the native window is explicitly focused", async () => {
    await renderNativeNotch();
    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });

    window.dispatchEvent(new Event("focus"));
    await waitFor(() => expect(screen.getByRole("button", { name: /Claude/i })).toHaveFocus());
  });

  it.each([
    ["right", "370x360"], ["left", "370x360"], ["top", "340x430"],
  ] as const)("publishes the expanded %s native/CSS geometry contract", async (placement, size) => {
    await renderNativeNotch();
    await emitEdgeView({ visibility: "card", placement, provider: "claude" });

    expect(screen.getByTestId("notch-surface")).toHaveClass("is-expanded");
    expect(screen.getByTestId("notch-surface")).toHaveAttribute("data-logical-size", size);
  });

  it.each([
    "connected", "loading", "unavailable", "notInstalled", "notAuthenticated", "stale",
  ] as const)("keeps the %s top rail inside the 270x70 one-line CSS contract", async (status) => {
    const statusSnapshot: DashboardSnapshot | null = status === "loading"
      ? null
      : {
        ...snapshot,
        claude: {
          ...snapshot.claude,
          status,
          remainingPercent: ["unavailable", "notInstalled", "notAuthenticated"].includes(status)
            ? null
            : snapshot.claude.remainingPercent,
        },
      };
    render(<NotchApp snapshot={statusSnapshot} />);
    await waitFor(() => expect(edgeHandler).toBeTypeOf("function"));
    await emitEdgeView({ visibility: "rail", placement: "top", provider: null });

    const surface = screen.getByTestId("notch-surface");
    expect(surface).toHaveClass("placement-top");
    expect(surface).not.toHaveClass("is-expanded");
    expect(surface).toHaveAttribute("data-logical-size", "270x70");
    const claude = screen.getByRole("button", { name: /Claude/i });
    expect(claude.querySelectorAll(":scope > .metric-value, :scope > .metric-status"))
      .toHaveLength(1);
  });

  it("sends safe-region and provider-selection inputs without hiding locally", async () => {
    await renderNativeNotch();
    await emitEdgeView({ visibility: "card", placement: "right", provider: "claude" });
    const surface = screen.getByTestId("notch-surface");
    fireEvent.pointerEnter(surface);
    fireEvent.mouseEnter(screen.getByRole("button", { name: /Codex/i }));
    fireEvent.pointerLeave(surface);
    await waitFor(() => expect(mocks.setNotchInteraction.mock.calls).toEqual([
      [{ kind: "enterSafeRegion" }], [{ kind: "selectProvider", provider: "codex" }],
      [{ kind: "leaveSafeRegion" }],
    ]));
    expect(screen.getByRole("article")).toBeInTheDocument();
  });

  it("pins and starts only the clicked provider's scoped refresh", async () => {
    await renderNativeNotch();
    await emitEdgeView({ visibility: "card", placement: "left", provider: "github" });
    mocks.setNotchInteraction.mockClear();
    fireEvent.click(screen.getByRole("button", { name: /GitHub/i }));
    await waitFor(() => expect(mocks.setNotchInteraction).toHaveBeenCalledExactlyOnceWith({ kind: "togglePin", provider: "github" }));
    expect(mocks.refreshProvider).toHaveBeenCalledExactlyOnceWith("github");
  });

  it("keeps pinned hover and focus authoritative, switches on click, then unpins on the same provider", async () => {
    await renderNativeNotch();
    await emitEdgeView({ visibility: "pinned", placement: "right", provider: "claude" });
    mocks.setNotchInteraction.mockClear();
    const codex = screen.getByRole("button", { name: /Codex/i });

    fireEvent.mouseEnter(codex);
    fireEvent.focus(codex);
    expect(mocks.setNotchInteraction).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", { name: "Claude" })).toBeInTheDocument();

    fireEvent.click(codex);
    await waitFor(() => expect(mocks.setNotchInteraction)
      .toHaveBeenCalledExactlyOnceWith({ kind: "togglePin", provider: "codex" }));
    expect(mocks.refreshProvider).toHaveBeenCalledExactlyOnceWith("codex");
    expect(screen.getByRole("heading", { name: "Claude" })).toBeInTheDocument();

    await emitEdgeView({ visibility: "pinned", placement: "right", provider: "codex" });
    expect(screen.getByRole("heading", { name: "Codex" })).toBeInTheDocument();
    mocks.setNotchInteraction.mockClear();
    mocks.refreshProvider.mockClear();
    fireEvent.click(screen.getByRole("button", { name: /Codex/i }));
    await waitFor(() => expect(mocks.setNotchInteraction)
      .toHaveBeenCalledExactlyOnceWith({ kind: "togglePin", provider: "codex" }));
    expect(mocks.refreshProvider).not.toHaveBeenCalled();
    await emitEdgeView({ visibility: "card", placement: "right", provider: "codex" });
    expect(screen.getByTestId("notch-app")).toHaveAttribute("data-visibility", "card");
  });

  it("subscribes before querying and recovers an edge view emitted before subscription", async () => {
    mocks.getCurrentEdgeView.mockResolvedValue({ visibility: "pinned", placement: "left", provider: "github" });
    await renderNativeNotch();

    await waitFor(() => expect(screen.getByRole("heading", { name: "GitHub" })).toBeInTheDocument());
    expect(mocks.listenForEdgeView.mock.invocationCallOrder[0])
      .toBeLessThan(mocks.getCurrentEdgeView.mock.invocationCallOrder[0]);
    expect(screen.getByTestId("notch-surface")).toHaveClass("placement-left");
  });

  it("never lets a late current-view query overwrite a newer edge event", async () => {
    let resolveCurrent!: (view: EdgeViewState) => void;
    mocks.getCurrentEdgeView.mockReturnValue(new Promise<EdgeViewState>((resolve) => { resolveCurrent = resolve; }));
    await renderNativeNotch();
    await emitEdgeView({ visibility: "card", placement: "right", provider: "codex" });
    resolveCurrent({ visibility: "rail", placement: "left", provider: null });
    await act(async () => { await Promise.resolve(); });

    expect(screen.getByRole("heading", { name: "Codex" })).toBeInTheDocument();
    expect(screen.getByTestId("notch-surface")).toHaveClass("placement-right");
  });

  it("starts the committed CSS exit only after token acceptance and acknowledges exactly once", async () => {
    let acceptExit!: (accepted: boolean) => void;
    mocks.beginNotchExit.mockReturnValue(new Promise<boolean>((resolve) => { acceptExit = resolve; }));
    await renderNativeNotch();
    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });
    await emitEdgeView({ visibility: "hidden", placement: "right", provider: null });
    const surface = screen.getByTestId("notch-surface");

    expect(surface).toHaveAttribute("aria-hidden", "true");
    expect(surface.querySelector(".notch-content")).toHaveAttribute("inert");
    expect(surface).toHaveClass("is-exit-pending");
    expect(surface).not.toHaveClass("is-exiting");
    const token = mocks.beginNotchExit.mock.calls[0]?.[0];
    expect(token).toEqual(expect.any(String));
    acceptExit(true);
    await waitFor(() => expect(surface).toHaveClass("is-exiting"));

    fireSurfaceAnimationEnd(surface.querySelector("button")!);
    expect(mocks.completeNotchExit).not.toHaveBeenCalled();
    fireSurfaceAnimationEnd(surface);
    await waitFor(() => expect(mocks.completeNotchExit).toHaveBeenCalledExactlyOnceWith(token));
    expect(screen.queryByTestId("notch-surface")).not.toBeInTheDocument();
    fireSurfaceAnimationEnd(surface);
    expect(mocks.completeNotchExit).toHaveBeenCalledTimes(1);
  });

  it("does not start a late accepted exit after a newer visible event", async () => {
    let acceptExit!: (accepted: boolean) => void;
    mocks.beginNotchExit.mockReturnValue(new Promise<boolean>((resolve) => { acceptExit = resolve; }));
    await renderNativeNotch();
    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });
    await emitEdgeView({ visibility: "hidden", placement: "right", provider: null });
    await emitEdgeView({ visibility: "card", placement: "right", provider: "codex" });
    acceptExit(true);
    await act(async () => { await Promise.resolve(); });

    expect(screen.getByTestId("notch-surface")).not.toHaveClass("is-exiting");
    expect(screen.getByRole("heading", { name: "Codex" })).toBeInTheDocument();
    expect(mocks.completeNotchExit).not.toHaveBeenCalled();
  });

  it("never relabels a late exit-A animation event as exit B", async () => {
    await renderNativeNotch();
    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });
    await emitEdgeView({ visibility: "hidden", placement: "right", provider: null });
    const exitA = await waitFor(() => {
      const element = screen.getByTestId("notch-surface");
      expect(element).toHaveClass("is-exiting");
      return element;
    });
    const tokenA = mocks.beginNotchExit.mock.calls[0][0];

    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });
    await emitEdgeView({ visibility: "hidden", placement: "right", provider: null });
    const exitB = await waitFor(() => {
      const element = screen.getByTestId("notch-surface");
      expect(element).toHaveClass("is-exiting");
      return element;
    });
    const tokenB = mocks.beginNotchExit.mock.calls[1][0];
    expect(tokenB).not.toBe(tokenA);

    fireSurfaceAnimationEnd(exitA);
    expect(mocks.completeNotchExit).not.toHaveBeenCalled();
    fireSurfaceAnimationEnd(exitB);
    await waitFor(() => expect(mocks.completeNotchExit).toHaveBeenCalledExactlyOnceWith(tokenB));
  });

  it("clears without animation when begin-exit is rejected", async () => {
    mocks.beginNotchExit.mockResolvedValue(false);
    await renderNativeNotch();
    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });
    await emitEdgeView({ visibility: "suppressed", placement: "right", provider: null });

    await waitFor(() => expect(screen.queryByTestId("notch-surface")).not.toBeInTheDocument());
    expect(mocks.completeNotchExit).not.toHaveBeenCalled();
  });

  it("keeps reduced-motion completion on the committed surface handler", async () => {
    vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
    await renderNativeNotch();
    await emitEdgeView({ visibility: "rail", placement: "top", provider: null });
    await emitEdgeView({ visibility: "hidden", placement: "top", provider: null });
    const surface = screen.getByTestId("notch-surface");
    await waitFor(() => expect(surface).toHaveClass("is-exiting"));
    const token = mocks.beginNotchExit.mock.calls[0][0];
    fireSurfaceAnimationEnd(surface);
    await waitFor(() => expect(mocks.completeNotchExit).toHaveBeenCalledExactlyOnceWith(token));
    vi.unstubAllGlobals();
  });

  it("does not acknowledge or apply an accepted exit after cleanup", async () => {
    let acceptExit!: (accepted: boolean) => void;
    mocks.beginNotchExit.mockReturnValue(new Promise<boolean>((resolve) => { acceptExit = resolve; }));
    const view = await renderNativeNotch();
    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });
    await emitEdgeView({ visibility: "suppressed", placement: "right", provider: null });
    const surface = screen.getByTestId("notch-surface");
    view.unmount();
    acceptExit(true);
    await act(async () => { await Promise.resolve(); });
    fireSurfaceAnimationEnd(surface);
    expect(mocks.completeNotchExit).not.toHaveBeenCalled();
  });

  it("keeps one card and rail container mounted while provider content changes", async () => {
    await renderNativeNotch();
    await emitEdgeView({ visibility: "card", placement: "right", provider: "claude" });
    const rail = screen.getByRole("toolbar");
    const slot = screen.getByTestId("provider-card-region");
    await emitEdgeView({ visibility: "card", placement: "right", provider: "codex" });
    expect(screen.getByRole("toolbar")).toBe(rail);
    expect(screen.getByTestId("provider-card-region")).toBe(slot);
    expect(screen.getByRole("heading", { name: "Codex" })).toBeInTheDocument();
  });

  it("restores selected metric focus after Escape changes an expanded card to the rail", async () => {
    const focus = vi.spyOn(HTMLElement.prototype, "focus");
    await renderNativeNotch();
    await emitEdgeView({ visibility: "card", placement: "right", provider: "codex" });
    const codex = screen.getByRole("button", { name: /Codex/i });
    focus.mockClear();

    fireEvent.keyDown(screen.getByTestId("notch-surface"), { key: "Escape" });
    expect(mocks.setNotchInteraction).toHaveBeenCalledWith({ kind: "escape" });
    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });

    await waitFor(() => expect(codex).toHaveFocus());
    expect(focus).toHaveBeenCalledTimes(1);
    focus.mockRestore();
  });

  it.each(["hidden", "suppressed"] as const)(
    "does not restore focus after a second Escape produces %s and a later passive reveal",
    async (hiddenVisibility) => {
    const focus = vi.spyOn(HTMLElement.prototype, "focus");
    await renderNativeNotch();
    await emitEdgeView({ visibility: "card", placement: "right", provider: "codex" });
    fireEvent.keyDown(screen.getByTestId("notch-surface"), { key: "Escape" });
    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });
    await waitFor(() => expect(screen.getByRole("button", { name: /Codex/i })).toHaveFocus());
    focus.mockClear();

    fireEvent.keyDown(screen.getByTestId("notch-surface"), { key: "Escape" });
    await emitEdgeView({ visibility: hiddenVisibility, placement: "right", provider: null });
    await emitEdgeView({ visibility: "rail", placement: "right", provider: null });
    await act(async () => { await Promise.resolve(); });

    expect(focus).not.toHaveBeenCalled();
    focus.mockRestore();
    },
  );

  it.each([
    ["right", "ArrowDown", "Codex"], ["left", "ArrowUp", "GitHub"],
    ["top", "ArrowRight", "Codex"], ["top", "ArrowLeft", "GitHub"],
  ] as const)("uses physical provider order for %s %s", async (placement, key, expected) => {
    await renderNativeNotch();
    await emitEdgeView({ visibility: "rail", placement, provider: null });
    screen.getByRole("button", { name: /Claude/i }).focus();
    fireEvent.keyDown(screen.getByTestId("notch-surface"), { key });
    expect(screen.getByRole("button", { name: new RegExp(expected, "i") })).toHaveFocus();
  });

  it("traps Tab inside pinned Dashy and opens the native menu on right click", async () => {
    await renderNativeNotch();
    await emitEdgeView({ visibility: "pinned", placement: "right", provider: "claude" });
    const buttons = screen.getAllByRole("button");
    const surface = screen.getByTestId("notch-surface");
    buttons.at(-1)!.focus(); fireEvent.keyDown(surface, { key: "Tab" });
    expect(buttons[0]).toHaveFocus();
    buttons[0].focus(); fireEvent.keyDown(surface, { key: "Tab", shiftKey: true });
    expect(buttons.at(-1)).toHaveFocus();
    fireEvent.contextMenu(surface);
    await waitFor(() => expect(mocks.showNotchMenu).toHaveBeenCalledTimes(1));
  });

  it("announces refresh failure politely, retains the card, and cleans up its listener", async () => {
    const view = await renderNativeNotch();
    await emitEdgeView({ visibility: "pinned", placement: "right", provider: "claude" });
    view.rerender(<NotchApp snapshot={{
      ...snapshot,
      claude: { ...snapshot.claude, status: "stale" },
    }} />);
    expect(screen.getByRole("status")).toHaveTextContent(/Claude.*Last known data/i);
    expect(screen.getByRole("heading", { name: "Claude" })).toBeInTheDocument();
    view.unmount();
    expect(mocks.unlisten).toHaveBeenCalledTimes(1);
  });

  it("announces only the currently retained provider failure in the live region", async () => {
    const view = await renderNativeNotch();
    await emitEdgeView({ visibility: "pinned", placement: "right", provider: "claude" });

    mocks.dashboard.refreshFailures = new Set(["claude"]);
    view.rerender(<NotchApp snapshot={snapshot} />);
    expect(screen.getByRole("status")).toHaveTextContent(/Claude.*Last known data/i);

    mocks.dashboard.refreshFailures = new Set();
    view.rerender(<NotchApp snapshot={snapshot} />);
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  });
});
