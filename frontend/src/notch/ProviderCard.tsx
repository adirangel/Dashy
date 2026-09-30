import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderId, ProviderStatus } from "../dashboard";
import { formatDateTime, formatTime, resolveLocale } from "../i18n";
import { ProviderGlyph } from "./ProviderGlyph";

export type ProviderViewStatus = ProviderStatus | "loading";

export function providerGuidanceKey(provider: ProviderId, status: ProviderViewStatus, errorKind?: string | null) {
  const suffix = provider === "github" ? "GitHub" : `${provider[0].toUpperCase()}${provider.slice(1)}`;
  if (status === "notInstalled") return `guidance.install${suffix}`;
  if (status === "notAuthenticated") return `guidance.signIn${suffix}`;
  if (status !== "unavailable" && status !== "stale") return null;
  // Only translate known categories. Never render raw CLI output or unknown errors.
  switch (errorKind) {
    case "missingExecutable": return `guidance.install${suffix}`;
    case "authentication": return `guidance.signIn${suffix}`;
    case "usageUnavailable": return "guidance.usageUnavailable";
    case "unsupportedOutput": return "guidance.unsupportedOutput";
    case "timeout": return "guidance.timeout";
    case "launch": return "guidance.launch";
    case "process": return "guidance.process";
    case "network": return "guidance.network";
    default: return "guidance.retryLater";
  }
}

export function statusTranslationKey(status: ProviderViewStatus) {
  return status === "notAuthenticated" ? "status.signInRequired" : `status.${status}`;
}

type ProviderCardProps = {
  provider: ProviderId;
  status: ProviderViewStatus;
  errorKind?: string | null;
  lastSuccessfulRefresh?: string | null;
  children?: ReactNode;
};

export function ProviderCard({ provider, status, errorKind, lastSuccessfulRefresh, children }: ProviderCardProps) {
  const { t, i18n } = useTranslation();
  const locale = resolveLocale(i18n.resolvedLanguage);
  const name = t(`providers.${provider}`);
  const guidanceKey = providerGuidanceKey(provider, status, errorKind);
  const showsData = status === "connected" || status === "stale";
  const statusText = status === "connected" ? null : t(statusTranslationKey(status));
  const guidance = guidanceKey ? t(guidanceKey, { provider: name }) : null;
  // The header line only confirms a healthy connection; every other state gets
  // the full explanation panel below, so no status text is repeated twice.
  const headerState = status === "connected" ? t("setup.connected") : null;

  return <article
    className={`provider-card provider-${provider} status-${status}`}
    data-status={status}
    tabIndex={status === "stale" ? 0 : undefined}
    dir={i18n.dir()}
    style={{ "--provider-accent": `var(--${provider})` } as React.CSSProperties}
  >
    <header className="provider-card__header">
      <span className="provider-card__disc" aria-hidden="true"><ProviderGlyph provider={provider} /></span>
      <div className="provider-card__title">
        <h2>{name}</h2>
        {headerState && <span className="provider-card__state">
          <i className="provider-card__state-dot" aria-hidden="true" />
          {headerState}
        </span>}
      </div>
      {showsData && lastSuccessfulRefresh && <time
        className="provider-card__updated"
        dateTime={lastSuccessfulRefresh}
        title={t("status.lastUpdated", { time: formatDateTime(lastSuccessfulRefresh, locale) })}
      >{formatTime(lastSuccessfulRefresh, locale)}</time>}
    </header>
    {showsData && children}
    {statusText && <aside className="provider-state" role={status === "loading" ? "status" : undefined}>
      <strong>{statusText}</strong>
      {status === "stale" && lastSuccessfulRefresh && <span>{t("status.lastUpdated", { time: formatDateTime(lastSuccessfulRefresh, locale) })}</span>}
      {guidance && <p>{guidance}</p>}
    </aside>}
  </article>;
}
