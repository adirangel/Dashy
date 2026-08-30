import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderId, ProviderStatus } from "../dashboard";
import type { ProviderSetupDefinition } from "./api";
import type { ProviderSetupController } from "./useProviderSetup";

type ProviderManagerProps = {
  controller: ProviderSetupController;
  enabledProviders: ProviderId[];
  onEnabledChange: (providers: ProviderId[]) => void;
  selectionDisabled?: boolean;
};

type PendingAction = {
  provider: ProviderId;
  action: "install" | "login";
};

const providerOrder: ProviderId[] = ["claude", "codex", "github"];

function statusKey(status: ProviderStatus) {
  switch (status) {
    case "connected": return "setup.connected" as const;
    case "notInstalled": return "setup.notInstalled" as const;
    case "notAuthenticated": return "setup.signInRequired" as const;
    case "stale":
    case "unavailable": return "setup.needsAttention" as const;
  }
}

function Confirmation({
  definition,
  action,
  onCancel,
  onConfirm,
}: {
  definition: ProviderSetupDefinition;
  action: PendingAction["action"];
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const { t } = useTranslation();
  const labelId = `provider-setup-${definition.provider}-${action}-confirmation`;
  const confirmKey = action === "install" ? "setup.confirmInstall" : "setup.confirmLogin";

  return <div
    className="provider-setup-confirmation"
    role="group"
    aria-labelledby={labelId}
  >
    <p id={labelId}>{t(confirmKey)}</p>
    <p>{t(action === "install" ? "setup.installDisclosure" : "setup.loginDisclosure")}</p>
    <dl>
      {action === "install" && <>
        <dt>{t("setup.publisher")}</dt>
        <dd>{definition.publisher}</dd>
        <dt>{t("setup.packageId")}</dt>
        <dd>{definition.packageId}</dd>
      </>}
      <dt>{t("setup.command")}</dt>
      <dd><code>{action === "install" ? definition.installCommand : definition.loginCommand}</code></dd>
    </dl>
    <div className="provider-setup-confirmation-actions">
      <button
        className="provider-setup-confirmation-cancel"
        type="button"
        onClick={onCancel}
      >{t("setup.cancel")}</button>
      <button
        className="provider-setup-confirmation-primary"
        type="button"
        onClick={onConfirm}
      >{t(confirmKey)}</button>
    </div>
  </div>;
}

export function ProviderManager({
  controller,
  enabledProviders,
  onEnabledChange,
  selectionDisabled = false,
}: ProviderManagerProps) {
  const { t } = useTranslation();
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null);

  if (controller.states === null) {
    return <div className="provider-setup-grid">
      <div className="provider-setup-card" data-status={controller.loadFailed ? "unavailable" : "loading"}>
        {controller.loadFailed
          ? <>
            <p className="provider-setup-error" role="alert">{t("setup.actionFailure")}</p>
            <div className="provider-setup-actions">
              <button type="button" onClick={() => { void controller.reload(); }}>{t("setup.retry")}</button>
            </div>
          </>
          : <p className="provider-setup-loading" role="status">{t("setup.loading")}</p>}
      </div>
    </div>;
  }

  const statesByProvider = new Map(
    controller.states.map((state) => [state.definition.provider, state]),
  );

  return <div className="provider-setup-grid">
    {providerOrder.map((provider) => {
      const state = statesByProvider.get(provider);
      if (!state) return null;
      const name = t(`providers.${provider}`);
      const headingId = `provider-setup-${provider}-name`;
      const isBusy = controller.busyProvider === provider;
      const pending = pendingAction?.provider === provider ? pendingAction : null;
      const isEnabled = enabledProviders.includes(provider);

      const confirm = (action: PendingAction["action"]) => {
        setPendingAction(null);
        void controller[action](provider);
      };

      return <article
        aria-labelledby={headingId}
        aria-busy={isBusy || undefined}
        className="provider-setup-card"
        data-provider={provider}
        data-status={state.status}
        key={provider}
      >
        <header className="provider-setup-card-header">
          <h2 className="provider-setup-name" id={headingId}>{name}</h2>
          <span className="provider-setup-status">{t(statusKey(state.status))}</span>
        </header>

        <label className="provider-setup-selection">
          <input
            type="checkbox"
            checked={isEnabled}
            disabled={selectionDisabled}
            onChange={(event) => onEnabledChange(event.target.checked
              ? [...enabledProviders, provider]
              : enabledProviders.filter((enabled) => enabled !== provider))}
          />
          <span>{t("setup.useProvider", { provider: name })}</span>
        </label>

        <div className="provider-setup-actions">
          {state.status === "notInstalled" && <button
            type="button"
            disabled={isBusy}
            onClick={() => setPendingAction({ provider, action: "install" })}
          >{t("setup.install", { provider: name })}</button>}
          {state.status === "notAuthenticated" && <button
            type="button"
            disabled={isBusy}
            onClick={() => setPendingAction({ provider, action: "login" })}
          >{t("setup.connect", { provider: name })}</button>}
          {(state.status === "stale" || state.status === "unavailable") && <button
            type="button"
            disabled={isBusy}
            onClick={() => { void controller.reload(); }}
          >{t("setup.retry")}</button>}
        </div>

        {pending && <Confirmation
          definition={state.definition}
          action={pending.action}
          onCancel={() => setPendingAction(null)}
          onConfirm={() => confirm(pending.action)}
        />}

        {controller.failureProvider === provider && <>
          <p className="provider-setup-error" role="alert">{t("setup.actionFailure")}</p>
          <a
            className="provider-setup-manual-help"
            href={state.definition.installUrl}
            target="_blank"
            rel="noreferrer"
          >{t("setup.manualHelp")}</a>
        </>}
      </article>;
    })}
  </div>;
}
