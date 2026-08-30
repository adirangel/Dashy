import { useTranslation } from "react-i18next";
import type { DashboardSnapshot, ProviderId, ProviderStatus } from "../dashboard";
import { formatNumber, resolveLocale } from "../i18n";
import type { EdgePlacement } from "../window";
import { localIsoDate } from "./heatmap";
import { ProgressRing } from "./ProgressRing";
import { ProviderGlyph } from "./ProviderGlyph";
import { statusTranslationKey, type ProviderViewStatus } from "./ProviderCard";

const PROVIDERS: ProviderId[] = ["claude", "codex", "github"];

type MetricRailProps = {
  placement: EdgePlacement;
  snapshot: DashboardSnapshot | null;
  selectedProvider: ProviderId;
  onSelect: (provider: ProviderId) => void;
  onFocusSelect?: (provider: ProviderId) => void;
  onActivate?: (provider: ProviderId) => void;
  refreshingProviders?: ReadonlySet<ProviderId>;
  now?: Date;
};

function viewStatus(snapshot: DashboardSnapshot | null, provider: ProviderId): ProviderViewStatus {
  return snapshot?.[provider].status ?? "loading";
}

function statusValue(status: ProviderViewStatus, t: (key: string) => string) {
  return status === "connected" ? null : t(statusTranslationKey(status));
}

export function MetricRail({
  placement,
  snapshot,
  selectedProvider,
  onSelect,
  onFocusSelect,
  onActivate,
  refreshingProviders = new Set(),
  now = new Date(),
}: MetricRailProps) {
  const { t, i18n } = useTranslation();
  const locale = resolveLocale(i18n.resolvedLanguage);
  const orientation = placement === "top" ? "horizontal" : "vertical";

  return <nav className="metric-rail" aria-label={t("settings.providerStatus")} aria-orientation={orientation} role="toolbar">
    {PROVIDERS.map((provider) => {
      const status = viewStatus(snapshot, provider);
      const entry = snapshot?.[provider];
      const isGitHub = provider === "github";
      const statusText = statusValue(status, t);
      const usageValue = !isGitHub && entry && "remainingPercent" in entry ? entry.remainingPercent : null;
      const hasUsage = usageValue !== null;
      const streak = isGitHub && entry && "currentStreakDays" in entry ? entry.currentStreakDays : null;
      const today = isGitHub && entry && "contributionDays" in entry
        ? entry.contributionDays?.find((day) => day.date === localIsoDate(now))
        : undefined;
      const ringValue = isGitHub
        ? today ? Math.min(100, Math.max(0, today.level * 25)) : null
        : usageValue;
      const compactValue = isGitHub
        ? streak === null ? "—" : new Intl.NumberFormat(locale, { style: "unit", unit: "day", unitDisplay: "narrow" }).format(streak)
        : hasUsage ? `${formatNumber(usageValue, locale)}%` : "—";
      const name = t(`providers.${provider}`);
      const githubState = isGitHub
        ? `${streak === null ? t("status.unavailable") : t("github.streakDays", { count: formatNumber(streak, locale) })}; ${t("github.today")}: ${today ? t("github.contributions", { count: formatNumber(today.count, locale) }) : t("status.unavailable")}`
        : null;
      const accessibleState = isGitHub
        ? statusText ? `${statusText}; ${githubState}` : githubState
        : statusText ?? (hasUsage ? t("usage.remaining", { value: formatNumber(usageValue, locale) }) : t("status.unavailable"));

      return <button
        type="button"
        key={provider}
        data-provider={provider}
        className={`metric-button ${selectedProvider === provider ? "is-selected" : ""} status-${status}`}
        aria-label={`${name}: ${accessibleState}`}
        aria-pressed={selectedProvider === provider}
        aria-busy={refreshingProviders.has(provider)}
        onMouseEnter={() => onSelect(provider)}
        onFocus={() => (onFocusSelect ?? onSelect)(provider)}
        onClick={() => (onActivate ?? onSelect)(provider)}
        style={{ "--provider-accent": `var(--${provider})` } as React.CSSProperties}
      >
        <ProgressRing
          value={ringValue}
          semantic={isGitHub ? "activity" : "progress"}
          label={`${name}: ${accessibleState}`}
          className={refreshingProviders.has(provider) ? "is-refreshing" : ""}
        >
          <ProviderGlyph provider={provider} />
        </ProgressRing>
        {statusText
          ? <span className="metric-status">{statusText}</span>
          : <span className="metric-value">{compactValue}</span>}
      </button>;
    })}
  </nav>;
}
