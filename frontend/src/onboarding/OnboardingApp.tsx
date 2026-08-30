import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderId } from "../dashboard";
import i18n, { resolveLocale, setLocale } from "../i18n";
import { ProviderManager } from "../setup/ProviderManager";
import { useProviderSetup } from "../setup/useProviderSetup";
import { completeOnboarding, getSettings, type AppSettings } from "../window";

export function OnboardingApp() {
  const { t } = useTranslation();
  const controller = useProviderSetup();
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [enabledProviders, setEnabledProviders] = useState<ProviderId[]>([]);
  const [selectionReady, setSelectionReady] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const [message, setMessage] = useState("");
  const finishInFlight = useRef(false);

  useEffect(() => {
    let active = true;
    void getSettings()
      .then(async (loaded) => {
        await setLocale(resolveLocale(loaded.locale));
        if (active) setSettings(loaded);
      })
      .catch(() => {
        if (active) setMessage(i18n.t("setup.finishFailure"));
      });
    return () => {
      active = false;
    };
  }, []);

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

  return <main className="onboarding-app">
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
