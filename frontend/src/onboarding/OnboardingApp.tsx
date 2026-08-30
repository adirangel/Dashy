import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderId } from "../dashboard";
import i18n, { resolveLocale, setLocale } from "../i18n";
import { ProviderManager } from "../setup/ProviderManager";
import { useProviderSetup } from "../setup/useProviderSetup";
import { useWindowActivationRevision } from "../useWindowActivation";
import { completeOnboarding, getSettings, type AppSettings } from "../window";

export function OnboardingApp() {
  const { t } = useTranslation();
  const activationRevision = useWindowActivationRevision();
  const controller = useProviderSetup(activationRevision);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [enabledProviders, setEnabledProviders] = useState<ProviderId[]>([]);
  const [selectionReady, setSelectionReady] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const [message, setMessage] = useState("");
  const finishInFlight = useRef(false);
  const mounted = useRef(true);
  const settingsRequest = useRef(0);
  const latestLocale = useRef<{
    request: number;
    locale: ReturnType<typeof resolveLocale>;
  } | null>(null);

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

  const applyLoadedSettings = useCallback(async (loaded: AppSettings, request: number) => {
    if (!mounted.current || request !== settingsRequest.current) return;
    const locale = resolveLocale(loaded.locale);
    latestLocale.current = { request, locale };
    try {
      await setLocale(locale);
    } catch {
      if (mounted.current && request === settingsRequest.current) {
        setMessage(i18n.t("setup.finishFailure"));
      }
      return;
    }
    if (!mounted.current) return;
    if (request !== settingsRequest.current) {
      await restoreLatestLocale();
      return;
    }
    setSettings({ ...loaded, locale });
  }, [restoreLatestLocale]);

  useEffect(() => {
    if (activationRevision <= 0) return;
    const request = ++settingsRequest.current;
    setMessage("");
    void getSettings()
      .then((loaded) => applyLoadedSettings(loaded, request))
      .catch(() => {
        if (mounted.current && request === settingsRequest.current) {
          setMessage(i18n.t("setup.finishFailure"));
        }
      });
  }, [activationRevision, applyLoadedSettings]);

  useEffect(() => {
    if (selectionReady || !settings || controller.states === null) return;
    const selected = settings.enabledProviders.length > 0
      ? settings.enabledProviders
      : controller.states
          .filter((state) => state.status === "connected")
          .map((state) => state.definition.provider);
    setEnabledProviders(selected);
    setSelectionReady(true);
  }, [controller.states, selectionReady, settings]);

  const finish = useCallback(async () => {
    if (!selectionReady || finishInFlight.current) return;
    finishInFlight.current = true;
    setFinishing(true);
    setMessage("");
    try {
      await completeOnboarding(enabledProviders);
    } catch {
      setMessage(t("setup.finishFailure"));
    } finally {
      finishInFlight.current = false;
      setFinishing(false);
    }
  }, [enabledProviders, selectionReady, t]);

  return <main
    className="onboarding-app"
    data-testid="onboarding-scroll-surface"
    data-scroll-owner="onboarding"
    style={{ height: "100vh", minHeight: 0, overflowY: "auto" }}
    tabIndex={0}
  >
    <header className="onboarding-header">
      <p className="onboarding-eyebrow">{t("setup.eyebrow")}</p>
      <h1 className="onboarding-title">{t("setup.title")}</h1>
      <span className="onboarding-description">{t("setup.description")}</span>
    </header>

    {(controller.states === null || selectionReady) && <ProviderManager
      controller={controller}
      enabledProviders={enabledProviders}
      onEnabledChange={setEnabledProviders}
    />}

    <footer className="onboarding-footer">
      <span className="onboarding-footer-status" role="status" aria-live="polite">
        {message}
      </span>
      {selectionReady && <button
        className="onboarding-finish"
        type="button"
        disabled={finishing}
        onClick={() => { void finish(); }}
      >{t("setup.finish")}</button>}
    </footer>
  </main>;
}
