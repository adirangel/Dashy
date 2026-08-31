import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  listen: vi.fn(),
  currentWindow: {
    label: "main",
    isVisible: vi.fn(),
    isFocused: vi.fn(),
    onFocusChanged: vi.fn(),
  },
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mocks.listen, emitTo: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => mocks.currentWindow }));

import {
  beginNotchExit, completeNotchExit, getCurrentEdgeView, isDashboardCacheChangedEvent,
  isCurrentWindowActive, isEdgeViewState, isExitToken, listenForCurrentWindowActivation,
  listenForSettingsChanges, openSettings,
} from "./window";

describe("strict edge-view validation", () => {
  beforeEach(() => {
    mocks.invoke.mockReset();
    mocks.listen.mockReset();
    mocks.currentWindow.isVisible.mockReset();
    mocks.currentWindow.isFocused.mockReset();
    mocks.currentWindow.onFocusChanged.mockReset();
  });

  afterEach(() => delete (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);

  it("queries the typed current-view native command with the exact wire name", async () => {
    const view = { visibility: "card" as const, placement: "right" as const, provider: "claude" as const };
    mocks.invoke.mockResolvedValue(view);
    await expect(getCurrentEdgeView()).resolves.toEqual(view);
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("get_current_edge_view");
  });

  it("rejects a malformed current-view command response at the native boundary", async () => {
    mocks.invoke.mockResolvedValue({ visibility: "card", placement: "right", provider: "claude", cursor: null });
    await expect(getCurrentEdgeView()).rejects.toThrow(/invalid edge view/i);
  });

  it("uses the exact strict begin/complete exit command wire", async () => {
    mocks.invoke.mockResolvedValue(true);
    await expect(beginNotchExit("exit-a1")).resolves.toBe(true);
    await expect(completeNotchExit("exit-a1")).resolves.toBe(true);
    expect(mocks.invoke.mock.calls).toEqual([
      ["begin_notch_exit", { request: { token: "exit-a1" } }],
      ["complete_notch_exit", { request: { token: "exit-a1" } }],
    ]);
  });

  it.each([
    "", "x".repeat(33), "with space", "slash/token", "UPPER", 4, null,
  ])("rejects an invalid exit token %# before IPC", async (token) => {
    expect(isExitToken(token)).toBe(false);
    await expect(beginNotchExit(token as string)).rejects.toThrow(/invalid exit token/i);
    await expect(completeNotchExit(token as string)).rejects.toThrow(/invalid exit token/i);
    expect(mocks.invoke).not.toHaveBeenCalled();
  });

  it("accepts an exact coherent payload", () => {
    expect(isEdgeViewState({ visibility: "pinned", placement: "left", provider: "codex" })).toBe(true);
    expect(isEdgeViewState({ visibility: "rail", placement: "top", provider: null })).toBe(true);
  });

  it.each([
    { visibility: "card", placement: "right", provider: "claude", cursor: { x: 1, y: 2 } },
    { visibility: "card", placement: "right", provider: "claude", extra: true },
    { visibility: "visible", placement: "right", provider: "claude" },
    { visibility: "card", placement: "bottom", provider: "claude" },
    { visibility: "card", placement: "right", provider: "gemini" },
    { visibility: "card", placement: "right" },
    { visibility: "card", placement: "right", provider: null },
    { visibility: "pinned", placement: "right", provider: null },
    { visibility: "hidden", placement: "right", provider: "claude" },
    { visibility: "rail", placement: "right", provider: "codex" },
    null,
  ])("rejects invalid, unknown, missing, or incoherent payload %#", (payload) => {
    expect(isEdgeViewState(payload)).toBe(false);
  });

  it("accepts only the bounded cache-change notification contract", () => {
    expect(isDashboardCacheChangedEvent({ revision: 1 })).toBe(true);
    expect(isDashboardCacheChangedEvent({ revision: 0xffff_ffff })).toBe(true);
    for (const invalid of [
      null, {}, { revision: 0 }, { revision: -1 }, { revision: 1.5 },
      { revision: Number.MAX_SAFE_INTEGER }, { revision: 1, provider: "claude" },
      { revision: "1" },
    ]) expect(isDashboardCacheChangedEvent(invalid)).toBe(false);
  });

  it("opens Settings through the dedicated native command", async () => {
    mocks.invoke.mockResolvedValue(undefined);
    await expect(openSettings()).resolves.toBeUndefined();
    expect(mocks.invoke).toHaveBeenCalledExactlyOnceWith("open_settings");
  });

  it("forwards the complete native settings event and returns its unlisten function", async () => {
    const unlisten = vi.fn();
    let nativeHandler: ((event: { payload: unknown }) => void) | undefined;
    mocks.listen.mockImplementation(async (_event, handler) => {
      nativeHandler = handler;
      return unlisten;
    });
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    const handler = vi.fn();
    const settings = {
      placement: "right" as const,
      monitor: null,
      locale: "en" as const,
      alwaysShowOverFullscreen: false,
      onboardingCompleted: true,
      enabledProviders: ["codex" as const],
    };

    await expect(listenForSettingsChanges(handler)).resolves.toBe(unlisten);
    nativeHandler?.({ payload: settings });
    expect(handler).toHaveBeenCalledExactlyOnceWith(settings);
  });

  it("treats the browser fallback as active without touching native window APIs", async () => {
    await expect(isCurrentWindowActive()).resolves.toBe(true);
    await expect(listenForCurrentWindowActivation(vi.fn())).resolves.toBeTypeOf("function");
    expect(mocks.currentWindow.isVisible).not.toHaveBeenCalled();
    expect(mocks.currentWindow.onFocusChanged).not.toHaveBeenCalled();
  });

  it.each([
    [true, false, true],
    [false, true, true],
    [false, false, false],
  ])("reports current native visibility=%s focus=%s as active=%s", async (
    visible, focused, expected,
  ) => {
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    mocks.currentWindow.isVisible.mockResolvedValue(visible);
    mocks.currentWindow.isFocused.mockResolvedValue(focused);

    await expect(isCurrentWindowActive()).resolves.toBe(expected);
  });

  it("uses a successful visibility signal when the focus query is unavailable", async () => {
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    mocks.currentWindow.isVisible.mockResolvedValue(true);
    mocks.currentWindow.isFocused.mockRejectedValue(new Error("focus permission unavailable"));

    await expect(isCurrentWindowActive()).resolves.toBe(true);
  });

  it("forwards only focus gains as activation and preserves listener cleanup", async () => {
    (window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    const unlisten = vi.fn();
    let focusHandler: ((event: { payload: boolean }) => void) | undefined;
    mocks.currentWindow.onFocusChanged.mockImplementation(async (handler) => {
      focusHandler = handler;
      return unlisten;
    });
    const handler = vi.fn();

    await expect(listenForCurrentWindowActivation(handler)).resolves.toBe(unlisten);
    focusHandler?.({ payload: false });
    focusHandler?.({ payload: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });
});
