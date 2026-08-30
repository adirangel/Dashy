import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
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
  disabled,
}: {
  definition: ProviderSetupDefinition;
  action: PendingAction["action"];
  onCancel: (restoreKeyboardFocus: boolean) => void;
  onConfirm: (restoreKeyboardFocus: boolean) => void;
  disabled: boolean;
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
        <dd><bdi
          className="provider-setup-technical-value"
          dir="ltr"
          style={{ unicodeBidi: "isolate" }}
        >{definition.packageId}</bdi></dd>
      </>}
      <dt>{t("setup.command")}</dt>
      <dd><code><bdi
        className="provider-setup-technical-value"
        dir="ltr"
        style={{ unicodeBidi: "isolate" }}
      >{action === "install" ? definition.installCommand : definition.loginCommand}</bdi></code></dd>
    </dl>
    <div className="provider-setup-confirmation-actions">
      <button
        className="provider-setup-confirmation-cancel"
        type="button"
        disabled={disabled}
        onClick={(event) => onCancel(
          event.detail === 0 && document.activeElement === event.currentTarget,
        )}
      >{t("setup.cancel")}</button>
      <button
        className="provider-setup-confirmation-primary"
        type="button"
        disabled={disabled}
        onClick={(event) => onConfirm(
          event.detail === 0 && document.activeElement === event.currentTarget,
        )}
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
  const [restoreFocusProvider, setRestoreFocusProvider] = useState<ProviderId | null>(null);
  const [manualHelpFailureProvider, setManualHelpFailureProvider] = useState<ProviderId | null>(null);
  const selectionRefs = useRef<Partial<Record<ProviderId, HTMLInputElement | null>>>({});
  const cardRefs = useRef<Partial<Record<ProviderId, HTMLElement | null>>>({});

  useEffect(() => {
    if (!restoreFocusProvider || pendingAction !== null) return;
    const selection = selectionRefs.current[restoreFocusProvider];
    const target = selection && !selection.disabled
      ? selection
      : cardRefs.current[restoreFocusProvider];
    target?.focus();
    setRestoreFocusProvider(null);
  }, [pendingAction, restoreFocusProvider]);

  if (controller.states === null) {
    return <div className="provider-setup-grid">
      <div className="provider-setup-card" data-status={controller.loadFailed ? "unavailable" : "loading"}>
        {controller.loadFailed
          ? <>
            <p className="provider-setup-error" role="alert">{t("setup.actionFailure")}</p>
            <div className="provider-setup-actions">
              <button
                type="button"
                disabled={controller.busyProvider !== null}
                onClick={() => { void controller.reload(); }}
              >{t("setup.retry")}</button>
            </div>
          </>
          : <p className="provider-setup-loading" role="status">{t("setup.loading")}</p>}
      </div>
    </div>;
  }

  const statesByProvider = new Map(
    controller.states.map((state) => [state.definition.provider, state]),
  );
  const setupActionActive = controller.busyProvider !== null;

  return <div className="provider-setup-grid">
    {providerOrder.map((provider) => {
      const state = statesByProvider.get(provider);
      if (!state) return null;
      const name = t(`providers.${provider}`);
      const headingId = `provider-setup-${provider}-name`;
      const isBusy = controller.busyProvider === provider;
      const busyStatus = isBusy && controller.busyAction
        ? t(controller.busyAction === "install" ? "setup.installing" : "setup.connecting")
        : null;
      const pending = pendingAction?.provider === provider ? pendingAction : null;
      const isEnabled = enabledProviders.includes(provider);

      const confirm = (action: PendingAction["action"], restoreKeyboardFocus: boolean) => {
        if (restoreKeyboardFocus) setRestoreFocusProvider(provider);
        setPendingAction(null);
        void controller[action](provider);
      };
      const beginAction = (action: PendingAction["action"]) => {
        setManualHelpFailureProvider(null);
        setPendingAction({ provider, action });
      };
      const cancel = (restoreKeyboardFocus: boolean) => {
        if (restoreKeyboardFocus) setRestoreFocusProvider(provider);
        setPendingAction(null);
      };

      return <article
        ref={(element) => { cardRefs.current[provider] = element; }}
        aria-labelledby={headingId}
        aria-busy={isBusy || undefined}
        className="provider-setup-card"
        data-provider={provider}
        data-status={state.status}
        key={provider}
        tabIndex={-1}
      >
        <header className="provider-setup-card-header">
          <h2 className="provider-setup-name" id={headingId}>{name}</h2>
          <span
            className="provider-setup-status"
            role="status"
            aria-live="polite"
            aria-atomic="true"
          >{busyStatus ?? t(statusKey(state.status))}</span>
        </header>

        <label className="provider-setup-selection">
          <input
            ref={(element) => { selectionRefs.current[provider] = element; }}
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
          {state.repairAction === "install" && <button
            type="button"
            disabled={setupActionActive}
            onClick={() => beginAction("install")}
          >{t("setup.install", { provider: name })}</button>}
          {state.repairAction === "login" && <button
            type="button"
            disabled={setupActionActive}
            onClick={() => beginAction("login")}
          >{t("setup.connect", { provider: name })}</button>}
          {(state.status === "stale" || state.status === "unavailable")
            && state.repairAction === null && <button
            type="button"
            disabled={setupActionActive}
            onClick={() => {
              setManualHelpFailureProvider(null);
              void controller.reload();
            }}
          >{t("setup.retry")}</button>}
        </div>

        {pending && <Confirmation
          definition={state.definition}
          action={pending.action}
          disabled={setupActionActive}
          onCancel={cancel}
          onConfirm={(restoreKeyboardFocus) => confirm(pending.action, restoreKeyboardFocus)}
        />}

        {controller.failureProvider === provider && <>
          <p className="provider-setup-error" role="alert">
            {t(manualHelpFailureProvider === provider
              ? "setup.manualHelpFailure"
              : "setup.actionFailure")}
          </p>
          <button
            className="provider-setup-manual-help"
            type="button"
            disabled={setupActionActive}
            onClick={() => {
              setManualHelpFailureProvider(null);
              void openUrl(state.definition.installUrl)
                .catch(() => setManualHelpFailureProvider(provider));
            }}
          >{t("setup.manualHelp")}</button>
        </>}
      </article>;
    })}
  </div>;
}
