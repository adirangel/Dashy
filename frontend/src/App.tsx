import { lazy, Suspense, useEffect } from "react";
import { resolveLocale, setLocale } from "./i18n";
import { SettingsApp } from "./settings/SettingsApp";
import { OnboardingApp } from "./onboarding/OnboardingApp";
import { NotchApp } from "./notch/NotchApp";
import "./settings.css";
import "./onboarding.css";
import { currentWindowLabel, getSettings, isTauriRuntime, listenForLocaleChanges } from "./window";

type AppProps = { windowLabel?: string };

const DevelopmentNotchApp = import.meta.env.DEV
  ? lazy(() => import("./notch/VisualFixtureApp.dev"))
  : null;

function App({ windowLabel = currentWindowLabel() }: AppProps) {
  useEffect(() => {
    if (windowLabel !== "main") return;
    if (!isTauriRuntime()) {
      void setLocale("en");
      return;
    }

    let active = true;
    let unlisten: (() => void) | undefined;
    let eventRevision = 0;
    let localeQueue = Promise.resolve();
    const queueLocale = (value: unknown) => {
      localeQueue = localeQueue.then(async () => {
        if (active) await setLocale(resolveLocale(value));
      });
    };
    const loadPersistedLocale = () => {
      const startingRevision = eventRevision;
      void getSettings()
        .then((settings) => {
          if (active && eventRevision === startingRevision) queueLocale(settings.locale);
        })
        .catch(() => undefined);
    };

    void listenForLocaleChanges((locale) => {
      eventRevision += 1;
      queueLocale(locale);
    }).then((stopListening) => {
      if (!active) {
        stopListening();
        return;
      }
      unlisten = stopListening;
      loadPersistedLocale();
    }).catch(loadPersistedLocale);

    return () => {
      active = false;
      unlisten?.();
    };
  }, [windowLabel]);

  if (windowLabel === "settings") return <SettingsApp />;
  if (windowLabel === "onboarding") return <OnboardingApp />;
  if (import.meta.env.DEV && DevelopmentNotchApp && !isTauriRuntime()) {
    return <Suspense fallback={null}><DevelopmentNotchApp /></Suspense>;
  }
  return <NotchApp />;
}

export default App;
