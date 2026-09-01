import { useTranslation } from "react-i18next";
import type { ProviderId, UsageSnapshot, UsageWindowSnapshot } from "../dashboard";
import { formatDateTime, formatNumber, resolveLocale } from "../i18n";
import { ProviderCard } from "./ProviderCard";

function UsageWindow({ window }: { window: UsageWindowSnapshot }) {
  const { t, i18n } = useTranslation();
  const locale = resolveLocale(i18n.resolvedLanguage);
  const label = window.labelKey === "short"
    ? t("usage.shortWindow")
    : window.labelKey === "monthly"
      ? t("usage.monthlyWindow")
      : t("usage.weeklyWindow");
  const reset = window.resetsAt ? formatDateTime(window.resetsAt, locale) : "";
  const value = Math.min(100, Math.max(0, window.remainingPercent));
  const formattedValue = formatNumber(value, locale);

  return <section className="usage-window" aria-label={label}>
    <div className="usage-window__row">
      <div className="usage-window__text">
        <h3>{label}</h3>
        {reset && <p>{t("usage.resets", { time: reset })}</p>}
      </div>
      <strong className="usage-window__value">
        <span className="visually-hidden">{t("usage.remaining", { value: formattedValue })}</span>
        <span aria-hidden="true">{formattedValue}<small>%</small></span>
      </strong>
    </div>
    <div className="usage-window__track" aria-hidden="true">
      <span style={{ inlineSize: `${value}%` }} />
    </div>
  </section>;
}

export function UsageProviderCard({
  provider,
  snapshot,
}: {
  provider: Exclude<ProviderId, "github" | "cursor">;
  snapshot: UsageSnapshot | null;
}) {
  const status = snapshot?.status ?? "loading";
  return <ProviderCard provider={provider} status={status} lastSuccessfulRefresh={snapshot?.lastSuccessfulRefresh}>
    <div className="usage-windows">
      {snapshot?.shortWindow && <UsageWindow window={snapshot.shortWindow} />}
      {snapshot?.weeklyWindow && <UsageWindow window={snapshot.weeklyWindow} />}
    </div>
  </ProviderCard>;
}
