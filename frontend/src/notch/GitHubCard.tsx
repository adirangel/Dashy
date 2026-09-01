import { useTranslation } from "react-i18next";
import type { GitHubSnapshot } from "../dashboard";
import { formatNumber, resolveLocale } from "../i18n";
import { formatContributionDate, heatmapMonthLabels, localIsoDate, positionContributionDays } from "./heatmap";
import { ProviderCard } from "./ProviderCard";

function Unavailable({ label }: { label: string }) {
  return <><b aria-hidden="true">—</b><small>{label}</small></>;
}

export function GitHubCard({ snapshot, now = new Date() }: { snapshot: GitHubSnapshot | null; now?: Date }) {
  const { t, i18n } = useTranslation();
  const status = snapshot?.status ?? "loading";
  const days = snapshot?.contributionDays?.slice(-84) ?? [];
  const positioned = positionContributionDays(days);
  const today = days.find((day) => day.date === localIsoDate(now));
  const locale = resolveLocale(i18n.resolvedLanguage);
  const streak = snapshot?.currentStreakDays;
  const formattedStreak = streak === null || streak === undefined ? null : formatNumber(streak, locale);
  const formattedToday = today ? formatNumber(today.count, locale) : null;

  return <ProviderCard provider="github" status={status} lastSuccessfulRefresh={snapshot?.lastSuccessfulRefresh}>
    <div className="card-stats">
      <div className="card-stat">
        <strong data-testid="github-streak-value">{formattedStreak ?? <Unavailable label={t("status.unavailable")} />}</strong>
        <span>{t("github.streakUnit")}</span>
      </div>
      <div className="card-stat">
        <strong data-testid="github-today-value">{formattedToday ?? <Unavailable label={t("status.unavailable")} />}</strong>
        <span>{t("github.todayUnit")}</span>
      </div>
    </div>
    <div className="contribution-heatmap" dir="ltr">
      <div className="heatmap-months" aria-hidden="true">
        {heatmapMonthLabels(days, locale).map(({ label, column }) => <span key={`${label}-${column}`} style={{ gridColumn: column }}>{label}</span>)}
      </div>
      <div className="heatmap-grid" role="list" aria-label={t("github.heatmapLabel")}>
        {positioned.map(({ day, style }) => <i
          key={day.date}
          role="listitem"
          data-level={Math.min(4, Math.max(0, day.level))}
          style={style}
          aria-label={`${t("github.contributions", { count: formatNumber(day.count, locale) })} — ${formatContributionDate(day.date, locale)}`}
        />)}
      </div>
    </div>
  </ProviderCard>;
}
