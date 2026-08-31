import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderId } from "../dashboard";
import i18n, {
  SUPPORTED_LOCALES, directionForLocale, languageName, resolveLocale, setLocale,
  type SupportedLocale,
} from "../i18n";
import { ProviderManager } from "../setup/ProviderManager";
import { useProviderSetup } from "../setup/useProviderSetup";
import { applyTrayLocale } from "../trayLabels";
import { useWindowActivationRevision } from "../useWindowActivation";
import { completeOnboarding, emitLocaleChanged, getSettings, type AppSettings } from "../window";

type OnboardingStep = "language" | "providers";
const STEP_COUNT = 2;

export function OnboardingApp() {
  const { t } = useTranslation();
  const activationRevision = useWindowActivationRevision();
  const controller = useProviderSetup(activationRevision);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [step, setStep] = useState<OnboardingStep>("language");
  const [chosenLocale, setChosenLocale] = useState<SupportedLocale | null>(null);
  const [enabledProviders, setEnabledProviders] = useState<ProviderId[]>([]);
  const [selectionReady, setSelectionReady] = useState(false);
  const [finishing, setFinishing] = useState(false);
  const [message, setMessage] = useState("");
  const finishInFlight = useRef(false);
  const mounted = useRef(true);
  const settingsRequest = useRef(0);
  const chosenLocaleRef = useRef<SupportedLocale | null>(null);
  const stepHeading = useRef<HTMLHeadingElement | null>(null);
  const focusStepHeading = useRef(false);
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
    // A locale the user already picked in step 1 outranks the persisted one, so an
    // activation reload can never revert a live language choice.
    const locale = chosenLocaleRef.current ?? resolveLocale(loaded.locale);
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
    const selected = settings.onboardingCompleted
      ? settings.enabledProviders
      : settings.enabledProviders.length > 0
        ? settings.enabledProviders
        : controller.states
            .filter((state) => state.status === "connected")
            .map((state) => state.definition.provider);
    setEnabledProviders(selected);
    setSelectionReady(true);
  }, [controller.states, selectionReady, settings]);

  useEffect(() => {
    if (!focusStepHeading.current) return;
    focusStepHeading.current = false;
    stepHeading.current?.focus();
  }, [step]);

  const goToStep = useCallback((next: OnboardingStep) => {
    focusStepHeading.current = true;
    setStep(next);
  }, []);

  const chooseLocale = useCallback((locale: SupportedLocale) => {
    chosenLocaleRef.current = locale;
    setChosenLocale(locale);
    latestLocale.current = { request: settingsRequest.current, locale };
    void setLocale(locale).catch(() => undefined);
  }, []);

  const finish = useCallback(async () => {
    if (!selectionReady || finishInFlight.current) return;
    finishInFlight.current = true;
    setFinishing(true);
    setMessage("");
    const locale = chosenLocaleRef.current ?? resolveLocale(settings?.locale);
    try {
      // Push localized tray labels first so the completion lifecycle's tray refresh
      // renders them; the tray keeps its defaults if this best-effort push fails.
      await applyTrayLocale(locale).catch(() => undefined);
      await completeOnboarding(enabledProviders, locale);
      await emitLocaleChanged(locale).catch(() => undefined);
    } catch {
      setMessage(t("setup.finishFailure"));
    } finally {
      finishInFlight.current = false;
      setFinishing(false);
    }
  }, [enabledProviders, selectionReady, settings, t]);

  const selectedLocale = chosenLocale ?? resolveLocale(settings?.locale);
  const languageStep = step === "language";

  return <main
    className="onboarding-app"
    data-testid="onboarding-scroll-surface"
    data-scroll-owner="onboarding"
    data-step={step}
    style={{ height: "100vh", minHeight: 0, overflowY: "auto" }}
    tabIndex={0}
  >
    <header className="onboarding-header">
      <p className="onboarding-eyebrow">{t("setup.eyebrow")}</p>
      <h1 className="onboarding-title" ref={stepHeading} tabIndex={-1}>
        {languageStep ? t("setup.languageTitle") : t("setup.title")}
      </h1>
      <span className="onboarding-description">
        {languageStep ? t("setup.languageDescription") : t("setup.description")}
      </span>
      <p className="onboarding-step-label">
        {t("setup.stepLabel", { current: languageStep ? 1 : 2, total: STEP_COUNT })}
      </p>
    </header>

    {languageStep && <div
      className="onboarding-language-grid"
      role="radiogroup"
      aria-label={t("setup.languageTitle")}
    >
      {SUPPORTED_LOCALES.map((locale) => <label
        className="onboarding-language-option"
        key={locale}
        lang={locale}
        dir={directionForLocale(locale)}
      >
        <input
          type="radio"
          name="onboarding-language"
          value={locale}
          checked={selectedLocale === locale}
          onChange={() => chooseLocale(locale)}
        />
        <span>{languageName(locale)}</span>
      </label>)}
    </div>}

    {!languageStep && (controller.states === null || selectionReady) && <ProviderManager
      controller={controller}
      enabledProviders={enabledProviders}
      onEnabledChange={setEnabledProviders}
      actionsRequireSelection
    />}

    <footer className="onboarding-footer">
      <span className="onboarding-footer-status" role="status" aria-live="polite">
        {message}
      </span>
      {languageStep
        ? <button
          className="onboarding-finish"
          type="button"
          onClick={() => goToStep("providers")}
        >{t("setup.continue")}</button>
        : <div className="onboarding-step-actions">
          <button
            className="onboarding-back"
            type="button"
            disabled={finishing}
            onClick={() => goToStep("language")}
          >{t("setup.back")}</button>
          {selectionReady && <button
            className="onboarding-finish"
            type="button"
            disabled={finishing}
            onClick={() => { void finish(); }}
          >{t("setup.finish")}</button>}
        </div>}
    </footer>
  </main>;
}
