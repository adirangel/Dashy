import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { ProviderId } from "../dashboard";
import i18n, {
  SUPPORTED_LOCALES, directionForLocale, languageName, resolveLocale, setLocale,
  type SupportedLocale,
} from "../i18n";
import { ProviderManager } from "../setup/ProviderManager";
import { isProviderReady } from "../setup/api";
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
            .filter((state) => state.status === "connected" || state.status === "stale")
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

  const unfinishedProviders = enabledProviders.filter((provider) => {
    const state = controller.states?.find((candidate) => candidate.definition.provider === provider);
    return controller.loadFailed || !state || !isProviderReady(state);
  });
  const providersReady = unfinishedProviders.length === 0;
  const setupBusy = controller.busyProvider !== null;

  const finish = useCallback(async (deferSetup = false) => {
    if (!selectionReady || finishInFlight.current || setupBusy || (!deferSetup && !providersReady)) return;
    finishInFlight.current = true;
    setFinishing(true);
    setMessage("");
    const locale = chosenLocaleRef.current ?? resolveLocale(settings?.locale);
    try {
      // Push localized tray labels first so the completion lifecycle's tray refresh
      // renders them; the tray keeps its defaults if this best-effort push fails.
      await applyTrayLocale(locale).catch(() => undefined);
      await completeOnboarding(enabledProviders, locale, deferSetup);
      await emitLocaleChanged(locale).catch(() => undefined);
    } catch (error) {
      const incomplete = error === "provider_setup_incomplete"
        || (error instanceof Error && error.message === "provider_setup_incomplete");
      setMessage(t(incomplete ? "setup.finishNotReady" : "setup.finishFailure"));
      if (incomplete) await controller.reload();
    } finally {
      finishInFlight.current = false;
      setFinishing(false);
    }
  }, [controller, enabledProviders, providersReady, selectionReady, settings, setupBusy, t]);

  const selectedLocale = chosenLocale ?? resolveLocale(settings?.locale);
  const languageStep = step === "language";
  const unfinishedNames = new Intl.ListFormat(resolveLocale(i18n.resolvedLanguage), { type: "conjunction" })
    .format(unfinishedProviders.map((provider) => t(`providers.${provider}`)));

  return <main
    className="onboarding-app"
    data-testid="onboarding-scroll-surface"
    data-scroll-owner="onboarding"
    data-step={step}
    style={{ height: "100vh", minHeight: 0, overflowY: "auto" }}
    tabIndex={0}
  >
    <header className="onboarding-header">
      <p className="settings-eyebrow">{t("setup.eyebrow")}</p>
      <div className="onboarding-title-row">
        <h1 className="onboarding-title" ref={stepHeading} tabIndex={-1}>
          {languageStep ? t("setup.languageTitle") : t("setup.title")}
        </h1>
        <span className="settings-chip">
          {t("setup.stepLabel", { current: languageStep ? 1 : 2, total: STEP_COUNT })}
        </span>
      </div>
      <span className="onboarding-description">
        {languageStep ? t("setup.languageDescription") : t("setup.description")}
      </span>
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

    {!languageStep && (controller.states === null || selectionReady) && <section
      className="settings-group settings-group--providers onboarding-providers"
      aria-label={t("setup.title")}
    >
      <ProviderManager
        controller={controller}
        enabledProviders={enabledProviders}
        onEnabledChange={setEnabledProviders}
        selectionDisabled={finishing}
        actionsRequireSelection
      />
    </section>}

    <footer className="onboarding-footer">
      <span className="onboarding-footer-status" id="onboarding-readiness" role="status" aria-live="polite">
        {message}
        {!languageStep && selectionReady && !providersReady && <>
          {message && <br />}
          {t("setup.providersNotReady", { providers: `\u2068${unfinishedNames}\u2069` })}
          <br />
          {t("setup.deferHint")}
        </>}
      </span>
      {languageStep
        ? <button
          className="settings-primary-button"
          type="button"
          onClick={() => goToStep("providers")}
        >{t("setup.continue")}</button>
        : <div className="onboarding-step-actions">
          <button
            className="settings-ghost-button"
            type="button"
            disabled={finishing}
            onClick={() => goToStep("language")}
          >{t("setup.back")}</button>
          {selectionReady && !providersReady && <button
            className="settings-ghost-button"
            type="button"
            disabled={finishing || setupBusy}
            aria-describedby="onboarding-readiness"
            onClick={() => { void finish(true); }}
          >{t("setup.defer")}</button>}
          {selectionReady && <button
            className="settings-primary-button"
            type="button"
            disabled={finishing || setupBusy || !providersReady}
            aria-describedby={!providersReady ? "onboarding-readiness" : undefined}
            onClick={() => { void finish(); }}
          >{t("setup.finish")}</button>}
        </div>}
    </footer>
  </main>;
}
