import { invoke } from "@tauri-apps/api/core";

export type ProviderStatus =
  | "connected"
  | "stale"
  | "notInstalled"
  | "notAuthenticated"
  | "unavailable";

export type ProviderId = "github" | "codex" | "claude";

export type ContributionDay = {
  date: string;
  count: number;
  level: number;
};

export type GitHubSnapshot = {
  status: ProviderStatus;
  accountLogin: string | null;
  contributionDays: ContributionDay[] | null;
  currentStreakDays: number | null;
  lastSuccessfulRefresh: string | null;
  errorKind: string | null;
};

export type UsageWindowSnapshot = {
  labelKey: "short" | "weekly";
  remainingPercent: number;
  resetsAt: string | null;
};

export type UsageSnapshot = {
  status: ProviderStatus;
  remainingPercent: number | null;
  shortWindow: UsageWindowSnapshot | null;
  weeklyWindow: UsageWindowSnapshot | null;
  lastSuccessfulRefresh: string | null;
  errorKind: string | null;
};

export type DashboardSnapshot = {
  github: GitHubSnapshot;
  codex: UsageSnapshot;
  claude: UsageSnapshot;
  refreshedAt: string | null;
};

const unavailableUsageSnapshot = (): UsageSnapshot => ({
  status: "unavailable",
  remainingPercent: null,
  shortWindow: null,
  weeklyWindow: null,
  lastSuccessfulRefresh: null,
  errorKind: null,
});

export function unavailableDashboardSnapshot(): DashboardSnapshot {
  return {
    github: {
      status: "unavailable",
      accountLogin: null,
      contributionDays: null,
      currentStreakDays: null,
      lastSuccessfulRefresh: null,
      errorKind: null,
    },
    codex: unavailableUsageSnapshot(),
    claude: unavailableUsageSnapshot(),
    refreshedAt: null,
  };
}

export async function getDashboardSnapshot(force = false): Promise<DashboardSnapshot> {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) {
    return unavailableDashboardSnapshot();
  }

  return invoke<DashboardSnapshot>("get_dashboard_snapshot", { force });
}

export const refreshDashboardProvider = (provider: ProviderId) =>
  invoke<DashboardSnapshot>("refresh_dashboard_provider", { provider });
