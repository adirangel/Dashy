import { useTranslation } from "react-i18next";
import type { CursorSnapshot } from "../dashboard";
import { ProviderCard } from "./ProviderCard";

function Unavailable({ label }: { label: string }) {
  return <><b aria-hidden="true">—</b><small>{label}</small></>;
}

// Cursor's CLI exposes no usage numbers (dashboard-only), so this card reports the
// account connection instead: plan tier, account, and where real usage lives.
export function CursorCard({ snapshot }: { snapshot: CursorSnapshot | null }) {
  const { t } = useTranslation();
  const status = snapshot?.status ?? "loading";
  const tier = snapshot?.subscriptionTier ?? null;
  const email = snapshot?.accountEmail ?? null;

  return <ProviderCard
    provider="cursor"
    status={status}
    lastSuccessfulRefresh={snapshot?.lastSuccessfulRefresh}
  >
    <div className="card-stats card-stats--account">
      <div className="card-stat">
        <strong className="card-stat__text card-stat__tier" data-testid="cursor-plan-value">{tier
          ?? <Unavailable label={t("status.unavailable")} />}</strong>
        <span>{t("cursor.plan")}</span>
      </div>
      <div className="card-stat">
        <strong className="card-stat__text card-stat__email" data-testid="cursor-account-value">{email
          ? <bdi dir="ltr">{email}</bdi>
          : <Unavailable label={t("status.unavailable")} />}</strong>
        <span>{t("cursor.account")}</span>
      </div>
    </div>
    <p className="cursor-usage-hint">{t("cursor.usageHint")}</p>
  </ProviderCard>;
}
