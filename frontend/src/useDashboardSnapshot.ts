import { useCallback, useEffect, useRef, useState } from "react";
import {
  getDashboardSnapshot,
  refreshDashboardProvider,
  unavailableDashboardSnapshot,
  type DashboardSnapshot,
  type ProviderId,
} from "./dashboard";
import { listenForDashboardCacheChanged } from "./window";

const REFRESH_INTERVAL_MS = 300_000;
// A reveal re-probes failed providers at most this often so hovering the edge
// repeatedly never turns into a CLI process storm.
const REVEAL_REPROBE_COOLDOWN_MS = 60_000;
const PROVIDERS: ProviderId[] = ["github", "codex", "claude", "grok", "cursor"];

// "cache" reads through the backend cache, "force" bypasses it, and "reveal"
// picks one of the two from how old and how healthy the current snapshot is.
type RefreshMode = "cache" | "reveal" | "force";
const REFRESH_PRIORITY: Record<RefreshMode, number> = { cache: 0, reveal: 1, force: 2 };

function strongerMode(current: RefreshMode | null, next: RefreshMode): RefreshMode {
  return current !== null && REFRESH_PRIORITY[current] >= REFRESH_PRIORITY[next] ? current : next;
}

function hasFailedProvider(
  snapshot: DashboardSnapshot | null,
  providers: readonly ProviderId[],
): boolean {
  if (!snapshot) return true;
  return providers.some((provider) => snapshot[provider].status !== "connected");
}

type ProviderVersions = Record<ProviderId, number>;

const initialProviderVersions = (): ProviderVersions => ({
  github: 0,
  codex: 0,
  claude: 0,
  grok: 0,
  cursor: 0,
});

function monotonicRefreshedAt(
  current: string | null,
  incoming: string | null,
): string | null {
  const currentTime = current === null ? Number.NaN : Date.parse(current);
  const incomingTime = incoming === null ? Number.NaN : Date.parse(incoming);
  const currentIsValid = Number.isFinite(currentTime);
  const incomingIsValid = Number.isFinite(incomingTime);

  if (currentIsValid && incomingIsValid) {
    return incomingTime > currentTime ? incoming : current;
  }
  if (currentIsValid) return current;
  if (incomingIsValid) return incoming;
  return current ?? incoming;
}

function mergeSelectedProvider(
  current: DashboardSnapshot | null,
  incoming: DashboardSnapshot,
  provider: ProviderId,
): DashboardSnapshot {
  const base = current ?? unavailableDashboardSnapshot();
  const refreshedAt = monotonicRefreshedAt(base.refreshedAt, incoming.refreshedAt);

  // TypeScript cannot correlate the computed key with its field type, but the
  // runtime shape is exact: the value comes from the same field of `incoming`.
  return { ...base, [provider]: incoming[provider], refreshedAt } as DashboardSnapshot;
}

export function useDashboardSnapshot() {
  const [snapshot, setSnapshot] = useState<DashboardSnapshot | null>(null);
  const [refreshing, setRefreshing] = useState(true);
  const [refreshingProviders, setRefreshingProviders] = useState<ReadonlySet<ProviderId>>(
    () => new Set(),
  );
  const [refreshFailures, setRefreshFailures] = useState<ReadonlySet<ProviderId>>(
    () => new Set(),
  );
  const mounted = useRef(true);
  const providerRefreshes = useRef(new Set<ProviderId>());
  const providerVersions = useRef<ProviderVersions>(initialProviderVersions());
  const snapshotRef = useRef<DashboardSnapshot | null>(null);
  snapshotRef.current = snapshot;
  const lastCompletedRefreshAt = useRef<number | null>(null);
  const lastRevealReprobeAt = useRef(Number.NEGATIVE_INFINITY);
  const revealProviders = useRef<readonly ProviderId[]>(PROVIDERS);
  const revealRef = useRef<(() => void) | null>(null);

  const refreshProvider = useCallback(async (provider: ProviderId) => {
    if (providerRefreshes.current.has(provider)) return;

    providerRefreshes.current.add(provider);
    const requestVersion = ++providerVersions.current[provider];
    if (mounted.current) {
      setRefreshingProviders(new Set(providerRefreshes.current));
      setRefreshFailures((current) => {
        if (!current.has(provider)) return current;
        const next = new Set(current);
        next.delete(provider);
        return next;
      });
    }

    try {
      const nextSnapshot = await refreshDashboardProvider(provider);
      if (mounted.current && providerVersions.current[provider] === requestVersion) {
        setSnapshot((current) => mergeSelectedProvider(current, nextSnapshot, provider));
        setRefreshFailures((current) => {
          if (!current.has(provider)) return current;
          const next = new Set(current);
          next.delete(provider);
          return next;
        });
      }
    } catch {
      // Keep the last verified snapshot visible until a later refresh succeeds.
      if (mounted.current && providerVersions.current[provider] === requestVersion) {
        setRefreshFailures((current) => new Set(current).add(provider));
      }
    } finally {
      providerRefreshes.current.delete(provider);
      if (mounted.current) {
        setRefreshingProviders(new Set(providerRefreshes.current));
      }
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    let active = true;
    let unlisten: (() => void) | undefined;
    let intervalId: number | undefined;
    let inFlight = false;
    let pending: RefreshMode | null = null;
    let listenerReady = false;

    const revealNeedsForce = () => {
      const now = Date.now();
      const completed = lastCompletedRefreshAt.current;
      if (completed === null || now - completed >= REFRESH_INTERVAL_MS) return true;
      if (now - lastRevealReprobeAt.current < REVEAL_REPROBE_COOLDOWN_MS) return false;
      return hasFailedProvider(snapshotRef.current, revealProviders.current);
    };

    const refresh = async (mode: RefreshMode) => {
      if (!active) return;
      if (inFlight) {
        // The in-flight read publishes its own result; only a cache change
        // must be re-read afterwards, since it may postdate that read.
        if (mode === "cache") pending = strongerMode(pending, "cache");
        return;
      }
      const force = mode === "force" || (mode === "reveal" && revealNeedsForce());
      if (mode === "reveal" && force) lastRevealReprobeAt.current = Date.now();

      inFlight = true;
      const requestVersions = Object.fromEntries(
        PROVIDERS.map((provider) => [provider, ++providerVersions.current[provider]]),
      ) as ProviderVersions;
      if (mounted.current) setRefreshing(true);

      try {
        const nextSnapshot = await getDashboardSnapshot(force);
        lastCompletedRefreshAt.current = Date.now();
        if (mounted.current) {
          setSnapshot((current) => {
            const applied = PROVIDERS.filter(
              (provider) => providerVersions.current[provider] === requestVersions[provider],
            );
            if (applied.length === 0) return current;

            const base = current ?? unavailableDashboardSnapshot();
            const next: DashboardSnapshot = {
              ...base,
              refreshedAt: monotonicRefreshedAt(base.refreshedAt, nextSnapshot.refreshedAt),
            };
            for (const provider of applied) {
              // Same computed-key cast as mergeSelectedProvider; shapes match by field.
              (next as Record<ProviderId, unknown>)[provider] = nextSnapshot[provider];
            }
            return next;
          });
          setRefreshFailures((current) => {
            let next: Set<ProviderId> | null = null;
            for (const provider of PROVIDERS) {
              if (
                current.has(provider)
                && providerVersions.current[provider] === requestVersions[provider]
              ) {
                next ??= new Set(current);
                next.delete(provider);
              }
            }
            return next ?? current;
          });
        }
      } catch {
        // Keep the last verified snapshot visible until a later refresh succeeds.
      } finally {
        inFlight = false;
        if (!active) return;
        const followUp = pending;
        pending = null;
        if (followUp !== null) {
          void refresh(followUp);
        } else {
          setRefreshing(false);
        }
      }
    };

    const onCacheChanged = () => {
      if (!active) return;
      if (!listenerReady) {
        pending = strongerMode(pending, "cache");
        return;
      }
      void refresh("cache");
    };

    const onReveal = () => {
      if (!active) return;
      if (!listenerReady) {
        pending = strongerMode(pending, "reveal");
        return;
      }
      void refresh("reveal");
    };
    revealRef.current = onReveal;

    // The notch window is hidden nearly all the time, so its timers may be
    // throttled or paused by the webview and by system sleep. Whenever the page
    // becomes visible again, revalidate instead of trusting the interval.
    const onDocumentVisible = () => {
      if (document.visibilityState === "visible") onReveal();
    };
    document.addEventListener("visibilitychange", onDocumentVisible);

    const startRefreshLoop = () => {
      if (!active) return;
      listenerReady = true;
      const initial = pending;
      pending = null;
      void refresh(initial === "reveal" ? "reveal" : "cache");
      intervalId = window.setInterval(() => { void refresh("force"); }, REFRESH_INTERVAL_MS);
    };

    void listenForDashboardCacheChanged(onCacheChanged).then((stop) => {
      if (!active) {
        stop();
        return;
      }
      unlisten = stop;
      startRefreshLoop();
    }).catch(startRefreshLoop);

    return () => {
      active = false;
      mounted.current = false;
      revealRef.current = null;
      document.removeEventListener("visibilitychange", onDocumentVisible);
      unlisten?.();
      if (intervalId !== undefined) window.clearInterval(intervalId);
    };
  }, []);

  // Called when the notch surface becomes visible. Reads through the cache when
  // the snapshot is fresh and healthy; otherwise re-probes the given providers.
  const revalidate = useCallback((providers: readonly ProviderId[]) => {
    revealProviders.current = providers;
    revealRef.current?.();
  }, []);

  return { snapshot, refreshing, refreshProvider, refreshingProviders, refreshFailures, revalidate };
}
