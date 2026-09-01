import { useTranslation } from "react-i18next";
import type { CursorSnapshot } from "../dashboard";
import { ProviderCard } from "./ProviderCard";

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
    <div className="github-summary cursor-summary">
      <div>
        <span>{t("cursor.plan")}</span>
        <strong data-testid="cursor-plan-value">{tier
          ?? <><b aria-hidden="true">—</b><small>{t("status.unavailable")}</small></>}</strong>
      </div>
      <div>
        <span>{t("cursor.account")}</span>
        <strong data-testid="cursor-account-value">{email
          ? <bdi dir="ltr">{email}</bdi>
          : <><b aria-hidden="true">—</b><small>{t("status.unavailable")}</small></>}</strong>
      </div>
    </div>
    <p className="cursor-usage-hint">{t("cursor.usageHint")}</p>
  </ProviderCard>;
}
