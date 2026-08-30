import type { DashboardSnapshot, ProviderId } from "../dashboard";
import { resolveLocale, type SupportedLocale } from "../i18n";
import type { EdgePlacement } from "../window";

export type VisualFixture = {
  placement: EdgePlacement;
  provider: ProviderId;
  background: "bright" | "dark";
  locale: SupportedLocale;
  now: Date;
  snapshot: DashboardSnapshot;
};

const NOW = new Date(2026, 7, 26, 12);
const contributionDays = Array.from({ length: 84 }, (_, index) => {
  const date = new Date(Date.UTC(2026, 5, 4 + index));
  return { date: date.toISOString().slice(0, 10), count: (index * 3) % 8, level: index % 5 };
});

const snapshot: DashboardSnapshot = {
  github: {
    status: "connected", accountLogin: "fixture", contributionDays,
    currentStreakDays: 12, lastSuccessfulRefresh: "2026-08-29T09:00:00Z", errorKind: null,
  },
  claude: {
    status: "connected", remainingPercent: 73,
    shortWindow: { labelKey: "short", remainingPercent: 73, resetsAt: "2026-08-29T10:51:00Z" },
    weeklyWindow: { labelKey: "weekly", remainingPercent: 93, resetsAt: "2026-09-03T21:00:00Z" },
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z", errorKind: null,
  },
  codex: {
    status: "connected", remainingPercent: 79,
    shortWindow: { labelKey: "short", remainingPercent: 79, resetsAt: "2026-08-29T12:00:00Z" },
    weeklyWindow: { labelKey: "weekly", remainingPercent: 52, resetsAt: "2026-09-04T21:00:00Z" },
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z", errorKind: null,
  },
  refreshedAt: "2026-08-29T09:00:00Z",
};

export function readVisualFixture(url = window.location.href): VisualFixture | null {
  const params = new URL(url).searchParams;
  if (params.get("fixture") !== "1") return null;
  const rawPlacement = params.get("placement");
  const placement: EdgePlacement = rawPlacement === "left" || rawPlacement === "top" ? rawPlacement : "right";
  const rawProvider = params.get("provider");
  const provider: ProviderId = rawProvider === "codex" || rawProvider === "github" ? rawProvider : "claude";
  return {
    placement,
    provider,
    background: params.get("background") === "bright" ? "bright" : "dark",
    locale: resolveLocale(params.get("locale")),
    now: NOW,
    snapshot,
  };
}

function nextFrame() {
  return new Promise<void>((resolve) => {
    if (typeof window.requestAnimationFrame === "function") {
      window.requestAnimationFrame(() => resolve());
    } else {
      window.setTimeout(resolve, 0);
    }
  });
}

export async function markVisualFixtureReady(fixture: VisualFixture) {
  for (let attempt = 0; attempt < 120; attempt += 1) {
    await nextFrame();
    const app = document.querySelector<HTMLElement>("[data-testid='notch-app']");
    const wrapper = document.querySelector<HTMLElement>("[data-visual-fixture='true']");
    const surface = document.querySelector<HTMLElement>("[data-testid='notch-surface']");
    const providerCard = document.querySelector<HTMLElement>(`.provider-${fixture.provider}`);
    const settled = document.documentElement.lang === fixture.locale
      && wrapper?.classList.contains(`fixture-${fixture.background}`)
      && surface?.dataset.placement === fixture.placement
      && providerCard !== null;
    if (!settled || !app) continue;

    app.dataset.fixtureReady = "true";
    app.dispatchEvent(new CustomEvent("dashy:fixture-ready", { bubbles: true }));
    return;
  }
  throw new Error("Dashy visual fixture did not settle");
}
