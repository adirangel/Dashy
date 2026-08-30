import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mocks.invoke }));

import {
  beginNotchExit, completeNotchExit, getCurrentEdgeView, isDashboardCacheChangedEvent,
  isEdgeViewState, isExitToken,
} from "./window";

describe("strict edge-view validation", () => {
  beforeEach(() => mocks.invoke.mockReset());

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
});
