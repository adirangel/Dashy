import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import ar from "./locales/ar";
import en, { type Messages } from "./locales/en";
import es from "./locales/es";
import fr from "./locales/fr";
import he from "./locales/he";
import ja from "./locales/ja";
import ru from "./locales/ru";
import zhCN from "./locales/zh-CN";

export const SUPPORTED_LOCALES = ["en", "he", "ar", "es", "ru", "fr", "zh-CN", "ja"] as const;
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

export const localeResources: Record<SupportedLocale, { translation: Messages }> = {
  en: { translation: en },
  he: { translation: he },
  ar: { translation: ar },
  es: { translation: es },
  ru: { translation: ru },
  fr: { translation: fr },
  "zh-CN": { translation: zhCN },
  ja: { translation: ja },
};

export function resolveLocale(value: unknown): SupportedLocale {
  return typeof value === "string" && (SUPPORTED_LOCALES as readonly string[]).includes(value)
    ? value as SupportedLocale
    : "en";
}

export function directionForLocale(locale: SupportedLocale): "ltr" | "rtl" {
  return locale === "he" || locale === "ar" ? "rtl" : "ltr";
}

export function languageName(locale: SupportedLocale): string {
  return new Intl.DisplayNames([locale], { type: "language" }).of(locale) ?? locale;
}

function updateRootLocale(locale: SupportedLocale) {
  if (typeof document === "undefined") return;
  document.documentElement.lang = locale;
  document.documentElement.dir = directionForLocale(locale);
}

void i18n.use(initReactI18next).init({
  resources: localeResources,
  lng: "en",
  fallbackLng: "en",
  supportedLngs: [...SUPPORTED_LOCALES],
  nonExplicitSupportedLngs: false,
  interpolation: { escapeValue: false },
  initAsync: false,
  returnNull: false,
});
updateRootLocale("en");

export async function setLocale(value: unknown): Promise<SupportedLocale> {
  const locale = resolveLocale(value);
  await i18n.changeLanguage(locale);
  updateRootLocale(locale);
  return locale;
}

export function formatNumber(value: number, locale = resolveLocale(i18n.resolvedLanguage)) {
  return new Intl.NumberFormat(locale).format(value);
}

export function formatDateTime(value: string | Date, locale = resolveLocale(i18n.resolvedLanguage)) {
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(locale, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

export function formatTime(value: string | Date, locale = resolveLocale(i18n.resolvedLanguage)) {
  const date = value instanceof Date ? value : new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(locale, { timeStyle: "short" }).format(date);
}

export default i18n;
