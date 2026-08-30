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
const PROVIDERS: ProviderId[] = ["github", "codex", "claude"];

type ProviderVersions = Record<ProviderId, number>;

const initialProviderVersions = (): ProviderVersions => ({
  github: 0,
  codex: 0,
  claude: 0,
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

  switch (provider) {
    case "github":
      return { ...base, github: incoming.github, refreshedAt };
    case "codex":
      return { ...base, codex: incoming.codex, refreshedAt };
    case "claude":
      return { ...base, claude: incoming.claude, refreshedAt };
  }
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
    let cacheDirty = false;
    let listenerReady = false;

    const refresh = async (force: boolean) => {
      if (!active || inFlight) return;

      inFlight = true;
      const requestVersions: ProviderVersions = {
        github: ++providerVersions.current.github,
        codex: ++providerVersions.current.codex,
        claude: ++providerVersions.current.claude,
      };
      if (mounted.current) setRefreshing(true);

      try {
        const nextSnapshot = await getDashboardSnapshot(force);
        if (mounted.current) {
          setSnapshot((current) => {
            const currentVersions = providerVersions.current;
            const applyGitHub = currentVersions.github === requestVersions.github;
            const applyCodex = currentVersions.codex === requestVersions.codex;
            const applyClaude = currentVersions.claude === requestVersions.claude;
            if (!applyGitHub && !applyCodex && !applyClaude) return current;

            const base = current ?? unavailableDashboardSnapshot();
            return {
              github: applyGitHub ? nextSnapshot.github : base.github,
              codex: applyCodex ? nextSnapshot.codex : base.codex,
              claude: applyClaude ? nextSnapshot.claude : base.claude,
              refreshedAt: monotonicRefreshedAt(base.refreshedAt, nextSnapshot.refreshedAt),
            };
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
        if (cacheDirty) {
          cacheDirty = false;
          void refresh(false);
        } else {
          setRefreshing(false);
        }
      }
    };

    const onCacheChanged = () => {
      if (!active) return;
      if (!listenerReady || inFlight) {
        cacheDirty = true;
        return;
      }
      void refresh(false);
    };

    const startRefreshLoop = () => {
      if (!active) return;
      listenerReady = true;
      cacheDirty = false;
      void refresh(false);
      intervalId = window.setInterval(() => { void refresh(true); }, REFRESH_INTERVAL_MS);
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
      unlisten?.();
      if (intervalId !== undefined) window.clearInterval(intervalId);
    };
  }, []);

  return { snapshot, refreshing, refreshProvider, refreshingProviders, refreshFailures };
}
