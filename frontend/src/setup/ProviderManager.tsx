import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { ProviderId, ProviderStatus } from "../dashboard";
import { ProviderGlyph } from "../notch/ProviderGlyph";
import type { ProviderSetupDefinition } from "./api";
import type { ProviderSetupController } from "./useProviderSetup";

type ProviderManagerProps = {
  controller: ProviderSetupController;
  enabledProviders: ProviderId[];
  onEnabledChange: (providers: ProviderId[]) => void;
  selectionDisabled?: boolean;
  actionsRequireSelection?: boolean;
};

type PendingAction = {
  provider: ProviderId;
  action: "install" | "login";
};

const providerOrder: ProviderId[] = ["claude", "codex", "github", "grok", "cursor"];

function statusKey(status: ProviderStatus) {
  switch (status) {
    case "connected": return "setup.connected" as const;
    case "notInstalled": return "setup.notInstalled" as const;
    case "notAuthenticated": return "setup.signInRequired" as const;
    // Stale means the last successful refresh produced real data; the connection is
    // fine even though the newest refresh attempt failed, so keep reporting Connected.
    case "stale": return "setup.connected" as const;
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
  // Manual-URL providers have no command to run: the confirmation only opens the
  // official install guide, so it discloses that instead of a winget command.
  const manualInstall = action === "install" && definition.installKind === "manualUrl";
  const confirmKey = manualInstall
    ? "setup.manualHelp"
    : action === "install" ? "setup.confirmInstall" : "setup.confirmLogin";
  const disclosureKey = manualInstall
    ? "setup.installManualDisclosure"
    : action === "install" ? "setup.installDisclosure" : "setup.loginDisclosure";

  return <div
    className="provider-setup-confirmation"
    role="group"
    aria-labelledby={labelId}
  >
    <p id={labelId}>{t(confirmKey)}</p>
    <p>{t(disclosureKey)}</p>
    {!manualInstall && <dl>
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
    </dl>}
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

// One row per provider: glyph, name, status line, an inline action only when
// something needs doing, and the switch that enables it in the rail. Used by
// both onboarding and Settings so they look identical.
export function ProviderManager({
  controller,
  enabledProviders,
  onEnabledChange,
  selectionDisabled = false,
  actionsRequireSelection = false,
}: ProviderManagerProps) {
  const { t } = useTranslation();
  const [pendingAction, setPendingAction] = useState<PendingAction | null>(null);
  const [restoreFocusProvider, setRestoreFocusProvider] = useState<ProviderId | null>(null);
  const [manualHelpFailureProvider, setManualHelpFailureProvider] = useState<ProviderId | null>(null);
  const selectionRefs = useRef<Partial<Record<ProviderId, HTMLInputElement | null>>>({});
  const rowRefs = useRef<Partial<Record<ProviderId, HTMLElement | null>>>({});

  useEffect(() => {
    if (!restoreFocusProvider || pendingAction !== null) return;
    const selection = selectionRefs.current[restoreFocusProvider];
    const target = selection && !selection.disabled
      ? selection
      : rowRefs.current[restoreFocusProvider];
    target?.focus();
    setRestoreFocusProvider(null);
  }, [pendingAction, restoreFocusProvider]);

  useEffect(() => {
    if (actionsRequireSelection && pendingAction
      && !enabledProviders.includes(pendingAction.provider)) {
      setPendingAction(null);
    }
  }, [actionsRequireSelection, enabledProviders, pendingAction]);

  useEffect(() => {
    if (actionsRequireSelection && manualHelpFailureProvider
      && !enabledProviders.includes(manualHelpFailureProvider)) {
      setManualHelpFailureProvider(null);
    }
  }, [actionsRequireSelection, enabledProviders, manualHelpFailureProvider]);

  if (controller.states === null) {
    return <div className="provider-setup-list">
      <div className="provider-setup-row" data-status={controller.loadFailed ? "unavailable" : "loading"}>
        {controller.loadFailed
          ? <>
            <p className="provider-setup-error" role="alert">{t("setup.actionFailure")}</p>
            <div className="provider-setup-row-actions">
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

  return <div className="provider-setup-list">
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
      const actionsAvailable = !actionsRequireSelection || isEnabled;
      const showFailure = actionsAvailable
        && (controller.failureProvider === provider || manualHelpFailureProvider === provider);

      const confirm = (action: PendingAction["action"], restoreKeyboardFocus: boolean) => {
        if (restoreKeyboardFocus) setRestoreFocusProvider(provider);
        setPendingAction(null);
        // Manual-URL installs never spawn a process: the confirmation opens the
        // official guide through the exact-URL opener allowlist.
        if (action === "install" && state.definition.installKind === "manualUrl") {
          setManualHelpFailureProvider(null);
          void openUrl(state.definition.installUrl)
            .catch(() => setManualHelpFailureProvider(provider));
          return;
        }
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
      const openManualHelp = () => {
        setManualHelpFailureProvider(null);
        void openUrl(state.definition.installUrl)
          .catch(() => setManualHelpFailureProvider(provider));
      };

      return <article
        ref={(element) => { rowRefs.current[provider] = element; }}
        aria-labelledby={headingId}
        aria-busy={isBusy || undefined}
        className="provider-setup-row"
        data-provider={provider}
        data-status={state.status}
        data-enabled={isEnabled}
        key={provider}
        tabIndex={-1}
      >
        <div className="provider-setup-row-main">
          <span className="provider-setup-disc"><ProviderGlyph provider={provider} /></span>
          <div className="provider-setup-row-text">
            <h3 className="provider-setup-name" id={headingId}>{name}</h3>
            <span
              className="provider-setup-status"
              role="status"
              aria-live="polite"
              aria-atomic="true"
            ><i className="provider-setup-status-dot" aria-hidden="true" />{busyStatus ?? t(statusKey(state.status))}</span>
          </div>
          <div className="provider-setup-row-actions">
            {actionsAvailable && state.repairAction === "install" && <button
              type="button"
              disabled={setupActionActive}
              onClick={() => beginAction("install")}
            >{t("setup.install", { provider: name })}</button>}
            {actionsAvailable && state.repairAction === "login" && <button
              type="button"
              disabled={setupActionActive}
              onClick={() => beginAction("login")}
            >{t("setup.connect", { provider: name })}</button>}
            {actionsAvailable && (state.status === "stale" || state.status === "unavailable")
              && state.repairAction === null && <button
              type="button"
              disabled={setupActionActive}
              onClick={() => {
                setManualHelpFailureProvider(null);
                void controller.reload();
              }}
            >{t("setup.retry")}</button>}
          </div>
          <input
            ref={(element) => { selectionRefs.current[provider] = element; }}
            className="settings-switch"
            type="checkbox"
            aria-label={t("setup.useProvider", { provider: name })}
            checked={isEnabled}
            disabled={selectionDisabled}
            onChange={(event) => onEnabledChange(event.target.checked
              ? [...enabledProviders, provider]
              : enabledProviders.filter((enabled) => enabled !== provider))}
          />
        </div>

        {actionsAvailable && pending && <Confirmation
          definition={state.definition}
          action={pending.action}
          disabled={setupActionActive}
          onCancel={cancel}
          onConfirm={(restoreKeyboardFocus) => confirm(pending.action, restoreKeyboardFocus)}
        />}

        {showFailure && <>
          <p className="provider-setup-error" role="alert">
            {t(manualHelpFailureProvider === provider
              ? "setup.manualHelpFailure"
              : "setup.actionFailure")}
          </p>
          <button
            className="provider-setup-manual-help"
            type="button"
            disabled={setupActionActive}
            onClick={openManualHelp}
          >{t("setup.manualHelp")}</button>
        </>}
      </article>;
    })}
  </div>;
}
