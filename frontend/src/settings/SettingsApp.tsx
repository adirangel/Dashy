import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { getDashboardSnapshot, type ProviderId } from "../dashboard";
import i18n, {
  SUPPORTED_LOCALES, languageName, resolveLocale, setLocale, type SupportedLocale,
} from "../i18n";
import { ProviderManager } from "../setup/ProviderManager";
import { useProviderSetup } from "../setup/useProviderSetup";
import { applyTrayLocale, translatedTrayLabels } from "../trayLabels";
import { useWindowActivationRevision } from "../useWindowActivation";
import {
  getSettings,
  emitLocaleChanged,
  listMonitors,
  listenForSettingsChanges,
  updateSettings,
  type AppSettings,
  type EdgePlacement,
  type MonitorInfo,
  type SettingsPatch,
} from "../window";

export { translatedTrayLabels };

function StartupCheckbox({
  state,
  disabled,
  onChange,
}: {
  state: boolean | null;
  disabled: boolean;
  onChange: () => void;
}) {
  const checkbox = useRef<HTMLInputElement>(null);
  useLayoutEffect(() => {
    if (checkbox.current) checkbox.current.indeterminate = state === null;
  }, [state]);

  return <input
    ref={checkbox}
    type="checkbox"
    checked={state === true}
    aria-checked={state === null ? "mixed" : state}
    disabled={disabled || state === null}
    onChange={onChange}
  />;
}

export function SettingsApp() {
  const { t } = useTranslation();
  const activationRevision = useWindowActivationRevision();
  const providerSetup = useProviderSetup(activationRevision);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [settingsReadyRevision, setSettingsReadyRevision] = useState(0);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [startup, setStartup] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const providerSaveInFlight = useRef(false);
  const settingsRequest = useRef(0);
  const latestLocale = useRef<{ request: number; locale: SupportedLocale } | null>(null);
  const activationRevisionRef = useRef(activationRevision);
  const mounted = useRef(true);
  activationRevisionRef.current = activationRevision;

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      settingsRequest.current += 1;
    };
  }, []);

  const restoreLatestLocale = useCallback(async () => {
    let target = latestLocale.current;
    while (mounted.current && target) {
      try {
        await setLocale(target.locale);
      } catch {
        return;
      }
      const current = latestLocale.current;
      if (!current || current.request === target.request) return;
      target = current;
    }
  }, []);

  const applyConfirmedSettings = useCallback(async (
    loadedSettings: AppSettings,
    request: number,
    readyRevision: number,
  ) => {
    const locale = resolveLocale(loadedSettings.locale);
    if (!mounted.current || request !== settingsRequest.current) return;
    latestLocale.current = { request, locale };
    try {
      await setLocale(locale);
    } catch {
      if (mounted.current && request === settingsRequest.current) {
        setMessage(i18n.t("guidance.retryLater", { provider: "Dashy" }));
      }
      return;
    }
    if (!mounted.current) return;
    if (request !== settingsRequest.current) {
      await restoreLatestLocale();
      return;
    }
    if (!mounted.current || request !== settingsRequest.current) return;
    setSettings({ ...loadedSettings, locale });
    setSettingsReadyRevision(readyRevision);
    await applyTrayLocale(locale).catch(() => {
      if (mounted.current && request === settingsRequest.current) {
        setMessage(i18n.t("guidance.retryLater", { provider: "Dashy" }));
      }
    });
  }, [restoreLatestLocale]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void listenForSettingsChanges((loadedSettings) => {
      if (!active) return;
      const request = ++settingsRequest.current;
      void applyConfirmedSettings(loadedSettings, request, activationRevisionRef.current);
    }).then((stopListening) => {
      if (!active) {
        stopListening();
        return;
      }
      unlisten = stopListening;
    }).catch(() => undefined);
    return () => {
      active = false;
      unlisten?.();
    };
  }, [applyConfirmedSettings]);

  useEffect(() => {
    if (activationRevision <= 0) return;
    let active = true;
    const request = ++settingsRequest.current;
    const showOptionalFailure = () => {
      if (active) setMessage(i18n.t("guidance.retryLater", { provider: "Dashy" }));
    };
    void getSettings()
      .then((loadedSettings) => applyConfirmedSettings(
        loadedSettings,
        request,
        activationRevision,
      ))
      .catch(() => {
        if (active && request === settingsRequest.current) showOptionalFailure();
      });
    void listMonitors()
      .then((loadedMonitors) => { if (active) setMonitors(loadedMonitors); })
      .catch(showOptionalFailure);
    void isEnabled()
      .then((startupEnabled) => {
        if (!active) return;
        setStartup(startupEnabled);
      })
      .catch(showOptionalFailure);
    return () => { active = false; };
  }, [activationRevision, applyConfirmedSettings]);

  const save = useCallback(async (patch: SettingsPatch) => {
    setBusy(true);
    setMessage("");
    try {
      const saved = await updateSettings(patch);
      setSettings(saved);
      return saved;
    } catch {
      setMessage(t("guidance.retryLater", { provider: "Dashy" }));
      return null;
    } finally {
      setBusy(false);
    }
  }, [t]);

  const selectedMonitor = settings?.monitor?.id ?? "";
  const monitorOptions = useMemo(() => {
    if (!settings?.monitor || monitors.some((monitor) => monitor.id === settings.monitor?.id)) return monitors;
    return [{
      id: settings.monitor.id, name: settings.monitor.name,
      ...settings.monitor.lastWorkArea, primary: false,
    }, ...monitors];
  }, [monitors, settings]);

  if (activationRevision <= 0
    || settings === null
    || settingsReadyRevision !== activationRevision) {
    return <main
      className="settings-app"
      data-testid="settings-scroll-surface"
      data-scroll-owner="settings"
      tabIndex={0}
    ><p role="status">{message || t("status.loading")}</p></main>;
  }

  const savePlacement = (placement: EdgePlacement) => { void save({ placement }); };
  const saveMonitor = (id: string) => {
    const monitor = monitors.find((candidate) => candidate.id === id);
    void save({ monitor: monitor ? {
      id: monitor.id, name: monitor.name,
      lastWorkArea: { x: monitor.x, y: monitor.y, width: monitor.width, height: monitor.height },
    } : null });
  };
  const saveLanguage = async (locale: SupportedLocale) => {
    const saved = await save({ locale });
    if (!saved) return;
    const confirmed = await setLocale(saved.locale);
    await applyTrayLocale(confirmed).catch(() => setMessage(i18n.t("guidance.retryLater", { provider: "Dashy" })));
    await emitLocaleChanged(confirmed).catch(() => setMessage(i18n.t("guidance.retryLater", { provider: "Dashy" })));
  };
  const toggleStartup = async () => {
    if (startup === null) return;
    const previous = startup;
    setBusy(true);
    setMessage("");
    try {
      if (previous) await disable(); else await enable();
      setStartup(await isEnabled());
    } catch {
      setStartup(previous);
      setMessage(t("guidance.retryLater", { provider: "Dashy" }));
    } finally {
      setBusy(false);
    }
  };
  const saveProviders = async (enabledProviders: ProviderId[]) => {
    if (providerSaveInFlight.current) return;
    providerSaveInFlight.current = true;
    try {
      const saved = await save({ enabledProviders });
      if (saved) setSettings(saved);
    } finally {
      providerSaveInFlight.current = false;
    }
  };
  const refreshAll = async () => {
    setBusy(true);
    setMessage("");
    try { await getDashboardSnapshot(true); }
    catch { setMessage(t("guidance.retryLater", { provider: "Dashy" })); }
    finally { setBusy(false); }
  };

  return <main
    className="settings-app"
    data-testid="settings-scroll-surface"
    data-scroll-owner="settings"
    tabIndex={0}
  >
    <header><p>Dashy</p><h1>{t("settings.title")}</h1></header>
    <section className="settings-panel" aria-busy={busy}>
      <label>{t("settings.placement")}
        <select value={settings.placement} disabled={busy} onChange={(event) => savePlacement(event.target.value as EdgePlacement)}>
          <option value="right">{t("settings.right")}</option><option value="left">{t("settings.left")}</option><option value="top">{t("settings.top")}</option>
        </select>
      </label>
      <label>{t("settings.monitor")}
        <select value={selectedMonitor} disabled={busy} onChange={(event) => saveMonitor(event.target.value)}>
          <option value="">{t("menu.primaryMonitor")}</option>
          {monitorOptions.map((monitor) => <option key={monitor.id} value={monitor.id}>{monitor.name}</option>)}
        </select>
      </label>
      <label>{t("settings.language")}
        <select value={settings.locale} disabled={busy} onChange={(event) => { void saveLanguage(resolveLocale(event.target.value)); }}>
          {SUPPORTED_LOCALES.map((locale) => <option key={locale} value={locale}>{languageName(locale)}</option>)}
        </select>
      </label>
      <label className="settings-toggle"><input type="checkbox" checked={settings.alwaysShowOverFullscreen} disabled={busy} onChange={(event) => { void save({ alwaysShowOverFullscreen: event.target.checked }); }} />{t("settings.fullscreen")}</label>
      <label className="settings-toggle"><StartupCheckbox state={startup} disabled={busy} onChange={() => { void toggleStartup(); }} />{t("settings.startup")}</label>
    </section>
    <section className="provider-settings" aria-labelledby="provider-status-title">
      <div className="provider-settings-header">
        <h2 id="provider-status-title">{t("settings.providerStatus")}</h2>
        <button type="button" disabled={busy} onClick={() => { void refreshAll(); }}>{t("actions.refreshAll")}</button>
      </div>
      <ProviderManager
        controller={providerSetup}
        enabledProviders={settings.enabledProviders}
        onEnabledChange={(enabledProviders) => { void saveProviders(enabledProviders); }}
        selectionDisabled={busy}
      />
    </section>
    <p className="settings-message" role="status" aria-live="polite">{message}</p>
  </main>;
}
