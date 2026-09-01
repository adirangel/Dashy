import { afterEach, describe, expect, it } from "vitest";
import {
  SUPPORTED_LOCALES,
  directionForLocale,
  localeResources,
  resolveLocale,
  setLocale,
} from "./i18n";

function leafKeys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) return [prefix];
  return Object.entries(value).flatMap(([key, child]) =>
    leafKeys(child, prefix ? `${prefix}.${key}` : key),
  );
}

describe("localization contract", () => {
  afterEach(async () => {
    await setLocale("en");
  });

  it("keeps the exact English leaf-key contract in every supported locale", () => {
    expect(SUPPORTED_LOCALES).toEqual(["en", "he", "ar", "es", "ru", "fr", "zh-CN", "ja"]);
    const englishKeys = leafKeys(localeResources.en.translation).sort();

    for (const locale of SUPPORTED_LOCALES) {
      expect(leafKeys(localeResources[locale].translation).sort()).toEqual(englishKeys);
    }
  });

  it("exposes the complete provider setup leaf-key contract", () => {
    expect(leafKeys(localeResources.en.translation.setup, "setup").sort()).toEqual([
      "setup.actionFailure", "setup.back", "setup.cancel", "setup.command",
      "setup.confirmInstall",
      "setup.confirmLogin", "setup.connect", "setup.connected", "setup.connecting",
      "setup.continue", "setup.description",
      "setup.eyebrow", "setup.finish", "setup.finishFailure", "setup.install",
      "setup.installDisclosure", "setup.installManualDisclosure", "setup.installing",
      "setup.languageDescription", "setup.languageTitle",
      "setup.loading", "setup.loginDisclosure",
      "setup.manualHelp", "setup.manualHelpFailure",
      "setup.needsAttention", "setup.notInstalled", "setup.packageId", "setup.publisher",
      "setup.retry", "setup.signInRequired", "setup.stepLabel", "setup.title",
      "setup.useProvider",
    ]);
  });

  it("uses RTL only for Hebrew and Arabic", () => {
    expect(directionForLocale("he")).toBe("rtl");
    expect(directionForLocale("ar")).toBe("rtl");
    for (const locale of ["en", "es", "ru", "fr", "zh-CN", "ja"] as const) {
      expect(directionForLocale(locale)).toBe("ltr");
    }
  });

  it("falls unknown stored locale values back to English", () => {
    expect(resolveLocale("de")).toBe("en");
    expect(resolveLocale(null)).toBe("en");
    expect(resolveLocale(undefined)).toBe("en");
  });

  it("updates the root language and direction whenever locale changes", async () => {
    await setLocale("ar");
    expect(document.documentElement.lang).toBe("ar");
    expect(document.documentElement.dir).toBe("rtl");

    await setLocale("fr");
    expect(document.documentElement.lang).toBe("fr");
    expect(document.documentElement.dir).toBe("ltr");
  });
});
