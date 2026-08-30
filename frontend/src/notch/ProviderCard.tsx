import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderId, ProviderStatus } from "../dashboard";
import { formatDateTime, resolveLocale } from "../i18n";

export type ProviderViewStatus = ProviderStatus | "loading";

function providerGuidanceKey(provider: ProviderId, status: ProviderViewStatus) {
  const suffix = provider === "github" ? "GitHub" : `${provider[0].toUpperCase()}${provider.slice(1)}`;
  if (status === "notInstalled") return `guidance.install${suffix}`;
  if (status === "notAuthenticated") return `guidance.signIn${suffix}`;
  return null;
}

export function statusTranslationKey(status: ProviderViewStatus) {
  return status === "notAuthenticated" ? "status.signInRequired" : `status.${status}`;
}

type ProviderCardProps = {
  provider: ProviderId;
  status: ProviderViewStatus;
  lastSuccessfulRefresh?: string | null;
  children?: ReactNode;
};

export function ProviderCard({ provider, status, lastSuccessfulRefresh, children }: ProviderCardProps) {
  const { t, i18n } = useTranslation();
  const locale = resolveLocale(i18n.resolvedLanguage);
  const name = t(`providers.${provider}`);
  const guidanceKey = providerGuidanceKey(provider, status);
  const showsData = status === "connected" || status === "stale";
  const statusText = status === "connected" ? null : t(statusTranslationKey(status));
  const guidance = guidanceKey
    ? t(guidanceKey)
    : status === "unavailable" || status === "stale"
      ? t("guidance.retryLater", { provider: name })
      : null;

  return <article
    className={`provider-card provider-${provider} status-${status}`}
    data-status={status}
    dir={i18n.dir()}
    style={{ "--provider-accent": `var(--${provider})` } as React.CSSProperties}
  >
    <header className="provider-card__header">
      <p className="provider-card__eyebrow">Dashy / {name}</p>
      <h2>{name}</h2>
    </header>
    {showsData && children}
    {statusText && <aside className="provider-state" role={status === "loading" ? "status" : undefined}>
      <strong>{statusText}</strong>
      {status === "stale" && lastSuccessfulRefresh && <span>{t("status.lastUpdated", { time: formatDateTime(lastSuccessfulRefresh, locale) })}</span>}
      {guidance && <p>{guidance}</p>}
    </aside>}
  </article>;
}
