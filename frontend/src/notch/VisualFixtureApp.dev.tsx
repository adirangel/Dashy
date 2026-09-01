import { useEffect, useState } from "react";
import { setLocale } from "../i18n";
import { NotchApp } from "./NotchApp";
import { markVisualFixtureReady, readVisualFixture } from "./visualFixture.dev";
import "./visualFixture.dev.css";

function VisualFixtureApp() {
  const [fixture] = useState(() => readVisualFixture());
  const [localeReady, setLocaleReady] = useState(fixture === null);

  useEffect(() => {
    if (!fixture) return;
    let active = true;
    void setLocale(fixture.locale).then(() => {
      if (active) setLocaleReady(true);
    });
    return () => { active = false; };
  }, [fixture]);

  useEffect(() => {
    if (!fixture || !localeReady) return;
    void markVisualFixtureReady(fixture).catch(() => undefined);
  }, [fixture, localeReady]);

  if (!fixture) return <NotchApp />;
  if (!localeReady) return null;

  return <div
    className={`visual-fixture-stage fixture-${fixture.background}`}
    data-visual-fixture="true"
  >
    <NotchApp
      placement={fixture.placement}
      snapshot={fixture.snapshot}
      selectedProvider={fixture.provider}
      now={fixture.now}
    />
  </div>;
}

export default VisualFixtureApp;
