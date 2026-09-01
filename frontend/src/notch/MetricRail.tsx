import { useTranslation } from "react-i18next";
import type { DashboardSnapshot, ProviderId } from "../dashboard";
import { formatNumber, resolveLocale } from "../i18n";
import type { EdgePlacement } from "../window";
import { localIsoDate } from "./heatmap";
import { ProgressRing } from "./ProgressRing";
import { ProviderGlyph } from "./ProviderGlyph";
import { statusTranslationKey, type ProviderViewStatus } from "./ProviderCard";

type MetricRailProps = {
  providers: ProviderId[];
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

// Three tile shapes: usage rings show a remaining percentage, the activity ring
// shows today's GitHub level with a streak, and account tiles have no metric at
// all — a neutral ring with the plan tier as the compact value.
type MetricKind = "usage" | "activity" | "account";
const METRIC_KIND: Record<ProviderId, MetricKind> = {
  claude: "usage",
  codex: "usage",
  grok: "usage",
  github: "activity",
  cursor: "account",
};

export function MetricRail({
  providers,
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
    {providers.map((provider) => {
      const status = viewStatus(snapshot, provider);
      const entry = snapshot?.[provider];
      const kind = METRIC_KIND[provider];
      const statusText = statusValue(status, t);
      const usageValue = kind === "usage" && entry && "remainingPercent" in entry
        ? entry.remainingPercent
        : null;
      const hasUsage = usageValue !== null;
      const streak = kind === "activity" && entry && "currentStreakDays" in entry
        ? entry.currentStreakDays
        : null;
      const today = kind === "activity" && entry && "contributionDays" in entry
        ? entry.contributionDays?.find((day) => day.date === localIsoDate(now))
        : undefined;
      const tier = kind === "account" && entry && "subscriptionTier" in entry
        ? entry.subscriptionTier
        : null;
      const ringValue = kind === "activity"
        ? today ? Math.min(100, Math.max(0, today.level * 25)) : null
        : usageValue;
      const compactValue = kind === "activity"
        ? streak === null ? "—" : new Intl.NumberFormat(locale, { style: "unit", unit: "day", unitDisplay: "narrow" }).format(streak)
        : kind === "account"
          ? tier ?? "—"
          : hasUsage ? `${formatNumber(usageValue, locale)}%` : "—";
      const name = t(`providers.${provider}`);
      const githubState = kind === "activity"
        ? `${streak === null ? t("status.unavailable") : t("github.streakDays", { count: formatNumber(streak, locale) })}; ${t("github.today")}: ${today ? t("github.contributions", { count: formatNumber(today.count, locale) }) : t("status.unavailable")}`
        : null;
      const accessibleState = kind === "activity"
        ? statusText ? `${statusText}; ${githubState}` : githubState
        : kind === "account"
          // A connected account without a reported tier is still connected, not
          // unavailable — the tile just has no plan name to read out.
          ? statusText ?? (tier ? `${t("cursor.plan")}: ${tier}` : t("setup.connected"))
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
          semantic={kind === "usage" ? "progress" : "activity"}
          label={`${name}: ${accessibleState}`}
          className={refreshingProviders.has(provider) ? "is-refreshing" : ""}
        >
          <ProviderGlyph provider={provider} />
        </ProgressRing>
        {statusText
          ? <span className="metric-status">{statusText}</span>
          : <span className={`metric-value${kind === "account" ? " metric-value--text" : ""}`}>
            {compactValue}
          </span>}
      </button>;
    })}
  </nav>;
}
