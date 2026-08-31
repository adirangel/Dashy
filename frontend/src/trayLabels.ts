import i18n, { type SupportedLocale } from "./i18n";
import { setTrayLabels, type TrayLabels } from "./window";

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

export async function applyTrayLocale(locale: SupportedLocale) {
  await setTrayLabels(translatedTrayLabels(locale));
}
