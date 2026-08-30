import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { getDashboardSnapshot, type ProviderId } from "../dashboard";
import i18n, { SUPPORTED_LOCALES, resolveLocale, setLocale, type SupportedLocale } from "../i18n";
import { ProviderManager } from "../setup/ProviderManager";
import { useProviderSetup } from "../setup/useProviderSetup";
import {
  getSettings,
  emitLocaleChanged,
  listMonitors,
  setTrayLabels,
  updateSettings,
  type AppSettings,
  type EdgePlacement,
  type MonitorInfo,
  type SettingsPatch,
  type TrayLabels,
} from "../window";

export function translatedTrayLabels(locale: SupportedLocale): TrayLabels {
  const translate = i18n.getFixedT(locale);
  return {
    show: translate("menu.show"), refreshAll: translate("menu.refreshAll"),
    placement: translate("menu.placement"), right: translate("settings.right"),
    left: translate("settings.left"), top: translate("settings.top"),
    monitor: translate("menu.monitor"), primaryMonitor: translate("menu.primaryMonitor"),
    unavailable: translate("status.unavailable"), settings: translate("menu.settings"),
    quit: translate("menu.quit"),
  };
}

async function applyTrayLocale(locale: SupportedLocale) {
  await setTrayLabels(translatedTrayLabels(locale));
}

function languageName(locale: SupportedLocale): string {
  return new Intl.DisplayNames([locale], { type: "language" }).of(locale) ?? locale;
}

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
  const providerSetup = useProviderSetup();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [monitors, setMonitors] = useState<MonitorInfo[]>([]);
  const [startup, setStartup] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");

  useEffect(() => {
    let active = true;
    const showOptionalFailure = () => {
      if (active) setMessage(i18n.t("guidance.retryLater", { provider: "Dashy" }));
    };
    void getSettings()
      .then(async (loadedSettings) => {
        const locale = resolveLocale(loadedSettings.locale);
        if (!active) return;
        setSettings({ ...loadedSettings, locale });
        await setLocale(locale);
        await applyTrayLocale(locale).catch(() => undefined);
      })
      .catch(showOptionalFailure);
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
  }, []);

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

  if (settings === null) {
    return <main
      className="settings-app"
      data-testid="settings-scroll-surface"
      data-scroll-owner="settings"
      tabIndex={0}
    ><p role="status">{t("status.loading")}</p></main>;
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
    const saved = await save({ enabledProviders });
    if (saved) setSettings(saved);
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
      />
    </section>
    <p className="settings-message" role="status" aria-live="polite">{message}</p>
  </main>;
}
