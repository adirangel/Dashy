import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getDashboardSnapshot, refreshDashboardProvider } from "./dashboard";
import { useDashboardSnapshot } from "./useDashboardSnapshot";
import type { DashboardSnapshot } from "./dashboard";

const windowMocks = vi.hoisted(() => ({
  listenForDashboardCacheChanged: vi.fn(),
  unlistenDashboardCacheChanged: vi.fn(),
}));

vi.mock("./window", () => ({
  listenForDashboardCacheChanged: windowMocks.listenForDashboardCacheChanged,
}));

vi.mock("./dashboard", async (importOriginal) => ({
  ...await importOriginal<typeof import("./dashboard")>(),
  getDashboardSnapshot: vi.fn(),
  refreshDashboardProvider: vi.fn(),
}));

const getSnapshot = vi.mocked(getDashboardSnapshot);
const refreshProvider = vi.mocked(refreshDashboardProvider);

const snapshot: DashboardSnapshot = {
  github: {
    status: "connected",
    accountLogin: "fixture-user",
    contributionDays: [],
    currentStreakDays: 12,
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z",
    errorKind: null,
  },
  codex: {
    status: "connected",
    remainingPercent: 68,
    shortWindow: {
      labelKey: "short",
      remainingPercent: 68,
      resetsAt: null,
    },
    weeklyWindow: {
      labelKey: "weekly",
      remainingPercent: 72,
      resetsAt: null,
    },
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z",
    errorKind: null,
  },
  claude: {
    status: "connected",
    remainingPercent: 59,
    shortWindow: {
      labelKey: "short",
      remainingPercent: 83,
      resetsAt: null,
    },
    weeklyWindow: {
      labelKey: "weekly",
      remainingPercent: 59,
      resetsAt: null,
    },
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z",
    errorKind: null,
  },
  grok: {
    status: "connected",
    remainingPercent: 61,
    shortWindow: null,
    weeklyWindow: {
      labelKey: "monthly",
      remainingPercent: 61,
      resetsAt: null,
    },
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z",
    errorKind: null,
  },
  cursor: {
    status: "connected",
    subscriptionTier: "pro",
    accountEmail: "fixture@cursor.com",
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z",
    errorKind: null,
  },
  refreshedAt: "2026-08-29T09:00:00Z",
};

function deferred<T>() {
  let resolve: (value: T) => void = () => undefined;
  let reject: (reason?: unknown) => void = () => undefined;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function withClaudeRemaining(remainingPercent: number, refreshedAt: string): DashboardSnapshot {
  return {
    ...snapshot,
    claude: { ...snapshot.claude, remainingPercent },
    refreshedAt,
  };
}

function withCodexRemaining(remainingPercent: number, refreshedAt: string): DashboardSnapshot {
  return {
    ...snapshot,
    codex: { ...snapshot.codex, remainingPercent },
    refreshedAt,
  };
}

afterEach(() => {
  vi.useRealTimers();
  vi.resetAllMocks();
});

describe("useDashboardSnapshot", () => {
  let cacheChanged: ((event: { revision: number }) => void) | undefined;

  beforeEach(() => {
    cacheChanged = undefined;
    windowMocks.listenForDashboardCacheChanged.mockImplementation(async (
      handler: (event: { revision: number }) => void,
    ) => {
      cacheChanged = handler;
      return windowMocks.unlistenDashboardCacheChanged;
    });
  });

  it("uses the cache on mount and forces a provider refresh after five minutes", async () => {
    vi.useFakeTimers();
    getSnapshot.mockResolvedValue(snapshot);

    const { unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });
    expect(getSnapshot).toHaveBeenCalledExactlyOnceWith(false);

    await act(async () => { await vi.advanceTimersByTimeAsync(300_000); });
    expect(getSnapshot).toHaveBeenCalledTimes(2);
    expect(getSnapshot).toHaveBeenLastCalledWith(true);

    unmount();
  });

  it("keeps the previous snapshot when a refresh is rejected", async () => {
    vi.useFakeTimers();
    getSnapshot.mockResolvedValueOnce(snapshot).mockRejectedValueOnce(new Error("offline"));

    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });
    expect(result.current.snapshot).toEqual(snapshot);

    await act(async () => { await vi.advanceTimersByTimeAsync(300_000); });
    expect(getSnapshot).toHaveBeenLastCalledWith(true);
    expect(result.current.snapshot).toEqual(snapshot);

    unmount();
  });

  it("does not start another refresh while one is in flight", async () => {
    vi.useFakeTimers();
    let resolveSnapshot: (value: DashboardSnapshot) => void = () => undefined;
    getSnapshot
      .mockReturnValueOnce(new Promise((resolve) => { resolveSnapshot = resolve; }))
      .mockResolvedValue(snapshot);

    const { unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });
    expect(getSnapshot).toHaveBeenCalledExactlyOnceWith(false);

    await act(async () => { await vi.advanceTimersByTimeAsync(300_000); });
    expect(getSnapshot).toHaveBeenCalledTimes(1);

    await act(async () => { resolveSnapshot(snapshot); await Promise.resolve(); });
    await act(async () => { await vi.advanceTimersByTimeAsync(300_000); });
    expect(getSnapshot).toHaveBeenCalledTimes(2);
    expect(getSnapshot).toHaveBeenLastCalledWith(true);

    unmount();
  });

  it("clears the periodic refresh interval on unmount", async () => {
    vi.useFakeTimers();
    getSnapshot.mockResolvedValue(snapshot);

    const { unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });
    unmount();

    await act(async () => { await vi.advanceTimersByTimeAsync(300_000); });
    expect(getSnapshot).toHaveBeenCalledExactlyOnceWith(false);
  });

  it("suppresses duplicate selected-provider calls while allowing different providers", async () => {
    getSnapshot.mockResolvedValue(snapshot);
    let resolveClaude: (value: DashboardSnapshot) => void = () => undefined;
    let resolveCodex: (value: DashboardSnapshot) => void = () => undefined;
    refreshProvider.mockImplementation((provider) => new Promise((resolve) => {
      if (provider === "claude") resolveClaude = resolve;
      if (provider === "codex") resolveCodex = resolve;
    }));
    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });

    await act(async () => {
      void result.current.refreshProvider("claude");
      void result.current.refreshProvider("claude");
      void result.current.refreshProvider("codex");
      await Promise.resolve();
    });

    expect(refreshProvider).toHaveBeenCalledTimes(2);
    expect(refreshProvider).toHaveBeenCalledWith("claude");
    expect(refreshProvider).toHaveBeenCalledWith("codex");
    expect(result.current.refreshingProviders).toEqual(new Set(["claude", "codex"]));

    await act(async () => { resolveCodex(snapshot); await Promise.resolve(); });
    expect(result.current.refreshingProviders).toEqual(new Set(["claude"]));
    await act(async () => { resolveClaude(snapshot); await Promise.resolve(); });
    expect(result.current.refreshingProviders).toEqual(new Set());
    unmount();
  });

  it("keeps the previous snapshot when a selected-provider refresh is rejected", async () => {
    getSnapshot.mockResolvedValue(snapshot);
    refreshProvider.mockRejectedValue(new Error("offline"));
    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });

    await act(async () => { await result.current.refreshProvider("github"); });

    expect(result.current.snapshot).toEqual(snapshot);
    expect(result.current.refreshingProviders).toEqual(new Set());
    expect(result.current.refreshFailures).toEqual(new Set(["github"]));
    unmount();
  });

  it("keeps a newer selected field when an older full response arrives later", async () => {
    const full = deferred<DashboardSnapshot>();
    const selected = deferred<DashboardSnapshot>();
    getSnapshot.mockReturnValue(full.promise);
    refreshProvider.mockReturnValue(selected.promise);
    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });

    await act(async () => {
      void result.current.refreshProvider("claude");
      await Promise.resolve();
    });
    selected.resolve(withClaudeRemaining(33, "2026-08-29T09:02:00Z"));
    await act(async () => { await selected.promise; });
    expect(result.current.snapshot?.claude.remainingPercent).toBe(33);

    full.resolve({ ...snapshot, refreshedAt: "2026-08-29T09:01:00Z" });
    await act(async () => { await full.promise; });

    expect(result.current.snapshot?.github).toEqual(snapshot.github);
    expect(result.current.snapshot?.codex).toEqual(snapshot.codex);
    expect(result.current.snapshot?.claude.remainingPercent).toBe(33);
    expect(result.current.snapshot?.refreshedAt).toBe("2026-08-29T09:02:00Z");
    unmount();
  });

  it("preserves both selected fields when different providers resolve out of order", async () => {
    getSnapshot.mockResolvedValue(snapshot);
    const claude = deferred<DashboardSnapshot>();
    const codex = deferred<DashboardSnapshot>();
    refreshProvider.mockImplementation((provider) => (
      provider === "claude" ? claude.promise : codex.promise
    ));
    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });

    await act(async () => {
      void result.current.refreshProvider("claude");
      void result.current.refreshProvider("codex");
      await Promise.resolve();
    });
    codex.resolve(withCodexRemaining(24, "2026-08-29T09:03:00Z"));
    await act(async () => { await codex.promise; });
    claude.resolve(withClaudeRemaining(31, "2026-08-29T09:02:00Z"));
    await act(async () => { await claude.promise; });

    expect(result.current.snapshot?.codex.remainingPercent).toBe(24);
    expect(result.current.snapshot?.claude.remainingPercent).toBe(31);
    expect(result.current.snapshot?.refreshedAt).toBe("2026-08-29T09:03:00Z");
    unmount();
  });

  it("ignores an older selected response after a newer full request supersedes it", async () => {
    vi.useFakeTimers();
    const full = deferred<DashboardSnapshot>();
    const selected = deferred<DashboardSnapshot>();
    getSnapshot.mockResolvedValueOnce(snapshot).mockReturnValueOnce(full.promise);
    refreshProvider.mockReturnValue(selected.promise);
    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });

    await act(async () => {
      void result.current.refreshProvider("claude");
      await Promise.resolve();
    });
    await act(async () => { await vi.advanceTimersByTimeAsync(300_000); });
    full.resolve(withClaudeRemaining(18, "2026-08-29T09:04:00Z"));
    await act(async () => { await full.promise; });
    selected.resolve(withClaudeRemaining(47, "2026-08-29T09:02:00Z"));
    await act(async () => { await selected.promise; });

    expect(result.current.snapshot?.claude.remainingPercent).toBe(18);
    expect(result.current.snapshot?.refreshedAt).toBe("2026-08-29T09:04:00Z");
    unmount();
  });

  it("does not publish a stale selected rejection after a newer full success", async () => {
    vi.useFakeTimers();
    const selected = deferred<DashboardSnapshot>();
    const newerFull = deferred<DashboardSnapshot>();
    getSnapshot.mockResolvedValueOnce(snapshot).mockReturnValueOnce(newerFull.promise);
    refreshProvider.mockReturnValue(selected.promise);
    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });

    await act(async () => {
      void result.current.refreshProvider("claude");
      await Promise.resolve();
    });
    await act(async () => { await vi.advanceTimersByTimeAsync(300_000); });
    newerFull.resolve(withClaudeRemaining(14, "2026-08-29T09:09:00Z"));
    await act(async () => { await newerFull.promise; });
    expect(result.current.snapshot?.claude.remainingPercent).toBe(14);
    expect(result.current.refreshFailures).toEqual(new Set());

    selected.reject(new Error("late selected failure"));
    await act(async () => {
      await selected.promise.catch(() => undefined);
      await Promise.resolve();
    });

    expect(result.current.snapshot?.claude.remainingPercent).toBe(14);
    expect(result.current.refreshFailures).toEqual(new Set());
    unmount();
  });

  it("retains a newer selected failure when an older full success cannot apply that provider", async () => {
    const olderFull = deferred<DashboardSnapshot>();
    const selected = deferred<DashboardSnapshot>();
    getSnapshot.mockReturnValue(olderFull.promise);
    refreshProvider.mockReturnValue(selected.promise);
    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await waitFor(() => expect(getSnapshot).toHaveBeenCalledExactlyOnceWith(false));

    await act(async () => {
      void result.current.refreshProvider("claude");
      await Promise.resolve();
    });
    selected.reject(new Error("current selected failure"));
    await act(async () => {
      await selected.promise.catch(() => undefined);
      await Promise.resolve();
    });
    expect(result.current.refreshFailures).toEqual(new Set(["claude"]));

    olderFull.resolve(withClaudeRemaining(18, "2026-08-29T09:10:00Z"));
    await act(async () => { await olderFull.promise; await Promise.resolve(); });

    expect(result.current.snapshot?.claude.remainingPercent).not.toBe(18);
    expect(result.current.refreshFailures).toEqual(new Set(["claude"]));
    unmount();
  });

  it("awaits listener registration before the initial cache query and folds an earlier event into that read", async () => {
    const registration = deferred<() => void>();
    windowMocks.listenForDashboardCacheChanged.mockImplementation((handler) => {
      cacheChanged = handler;
      return registration.promise;
    });
    getSnapshot.mockResolvedValue(snapshot);
    const { unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });

    expect(getSnapshot).not.toHaveBeenCalled();
    cacheChanged?.({ revision: 1 });
    registration.resolve(windowMocks.unlistenDashboardCacheChanged);
    await act(async () => { await registration.promise; await Promise.resolve(); });

    expect(windowMocks.listenForDashboardCacheChanged.mock.invocationCallOrder[0])
      .toBeLessThan(getSnapshot.mock.invocationCallOrder[0]);
    expect(getSnapshot).toHaveBeenCalledExactlyOnceWith(false);
    unmount();
  });

  it("coalesces cache-change bursts during initial and event reads into one retained follow-up per read", async () => {
    const initial = deferred<DashboardSnapshot>();
    const eventRead = deferred<DashboardSnapshot>();
    getSnapshot
      .mockReturnValueOnce(initial.promise)
      .mockReturnValueOnce(eventRead.promise)
      .mockResolvedValue(withClaudeRemaining(17, "2026-08-29T09:07:00Z"));
    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await waitFor(() => expect(getSnapshot).toHaveBeenCalledExactlyOnceWith(false));

    await act(async () => {
      cacheChanged?.({ revision: 2 });
      cacheChanged?.({ revision: 3 });
      cacheChanged?.({ revision: 4 });
      await Promise.resolve();
    });
    expect(getSnapshot).toHaveBeenCalledTimes(1);

    initial.resolve(snapshot);
    await act(async () => { await initial.promise; });
    await waitFor(() => expect(getSnapshot).toHaveBeenCalledTimes(2));
    expect(getSnapshot).toHaveBeenLastCalledWith(false);

    await act(async () => {
      cacheChanged?.({ revision: 5 });
      cacheChanged?.({ revision: 6 });
      await Promise.resolve();
    });
    expect(getSnapshot).toHaveBeenCalledTimes(2);

    eventRead.resolve(withClaudeRemaining(23, "2026-08-29T09:06:00Z"));
    await act(async () => { await eventRead.promise; });
    await waitFor(() => expect(getSnapshot).toHaveBeenCalledTimes(3));
    expect(getSnapshot).toHaveBeenLastCalledWith(false);
    await waitFor(() => expect(result.current.snapshot?.claude.remainingPercent).toBe(17));
    expect(refreshProvider).not.toHaveBeenCalled();
    unmount();
  });

  it("retains a cache event during a periodic forced read and follows it with a cache-only read", async () => {
    vi.useFakeTimers();
    const periodic = deferred<DashboardSnapshot>();
    getSnapshot
      .mockResolvedValueOnce(snapshot)
      .mockReturnValueOnce(periodic.promise)
      .mockResolvedValue(withClaudeRemaining(29, "2026-08-29T09:08:00Z"));
    const { unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });

    await act(async () => { await vi.advanceTimersByTimeAsync(300_000); });
    expect(getSnapshot).toHaveBeenLastCalledWith(true);
    await act(async () => { cacheChanged?.({ revision: 7 }); await Promise.resolve(); });
    expect(getSnapshot).toHaveBeenCalledTimes(2);

    periodic.resolve(withClaudeRemaining(31, "2026-08-29T09:07:00Z"));
    await act(async () => { await periodic.promise; await Promise.resolve(); });
    expect(getSnapshot).toHaveBeenCalledTimes(3);
    expect(getSnapshot).toHaveBeenLastCalledWith(false);
    unmount();
  });

  it("does not query after unmount while listener registration is still pending", async () => {
    const registration = deferred<() => void>();
    windowMocks.listenForDashboardCacheChanged.mockImplementation((handler) => {
      cacheChanged = handler;
      return registration.promise;
    });
    getSnapshot.mockResolvedValue(snapshot);
    const { unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); });
    unmount();

    registration.resolve(windowMocks.unlistenDashboardCacheChanged);
    await act(async () => { await registration.promise; await Promise.resolve(); });

    expect(windowMocks.unlistenDashboardCacheChanged).toHaveBeenCalledTimes(1);
    expect(getSnapshot).not.toHaveBeenCalled();
    cacheChanged?.({ revision: 8 });
    await act(async () => { await Promise.resolve(); });
    expect(getSnapshot).not.toHaveBeenCalled();
  });

  it("reloads externally refreshed cache without duplicate provider work and clears obsolete failures", async () => {
    getSnapshot.mockResolvedValue(snapshot);
    refreshProvider.mockRejectedValue(new Error("transport failed"));
    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    await act(async () => { await result.current.refreshProvider("github"); });
    expect(result.current.refreshFailures).toEqual(new Set(["github"]));

    const updated = withClaudeRemaining(21, "2026-08-29T09:05:00Z");
    getSnapshot.mockResolvedValueOnce(updated);
    await act(async () => { cacheChanged?.({ revision: 1 }); await Promise.resolve(); await Promise.resolve(); });

    expect(getSnapshot).toHaveBeenLastCalledWith(false);
    expect(refreshProvider).toHaveBeenCalledExactlyOnceWith("github");
    expect(result.current.snapshot?.claude.remainingPercent).toBe(21);
    expect(result.current.refreshFailures).toEqual(new Set());
    unmount();
  });

  it("protects a newer selected refresh from an older cache-change reload", async () => {
    getSnapshot.mockResolvedValueOnce(snapshot);
    const cacheReload = deferred<DashboardSnapshot>();
    const selected = deferred<DashboardSnapshot>();
    refreshProvider.mockReturnValue(selected.promise);
    const { result, unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });

    getSnapshot.mockReturnValueOnce(cacheReload.promise);
    await act(async () => { cacheChanged?.({ revision: 2 }); await Promise.resolve(); });
    await act(async () => { void result.current.refreshProvider("claude"); await Promise.resolve(); });
    selected.resolve(withClaudeRemaining(19, "2026-08-29T09:06:00Z"));
    await act(async () => { await selected.promise; });
    cacheReload.resolve(withClaudeRemaining(45, "2026-08-29T09:05:00Z"));
    await act(async () => { await cacheReload.promise; });

    expect(result.current.snapshot?.claude.remainingPercent).toBe(19);
    expect(result.current.snapshot?.refreshedAt).toBe("2026-08-29T09:06:00Z");
    unmount();
  });

  it("removes the cache-change listener and ignores late notifications on cleanup", async () => {
    getSnapshot.mockResolvedValue(snapshot);
    const { unmount } = renderHook(() => useDashboardSnapshot());
    await act(async () => { await Promise.resolve(); await Promise.resolve(); });
    const callsBeforeUnmount = getSnapshot.mock.calls.length;
    unmount();
    expect(windowMocks.unlistenDashboardCacheChanged).toHaveBeenCalledTimes(1);

    cacheChanged?.({ revision: 3 });
    await act(async () => { await Promise.resolve(); });
    expect(getSnapshot).toHaveBeenCalledTimes(callsBeforeUnmount);
  });
});
