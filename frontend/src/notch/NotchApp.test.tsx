import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { setLocale } from "../i18n";
import { formatDateTime, formatNumber } from "../i18n";
import type { DashboardSnapshot } from "../dashboard";
import { CursorCard } from "./CursorCard";
import { GitHubCard } from "./GitHubCard";
import { MetricRail } from "./MetricRail";
import { NotchApp } from "./NotchApp";
import { ProgressRing } from "./ProgressRing";
import { UsageProviderCard } from "./UsageProviderCard";
import { VisualFixtureApp } from "./VisualFixtureApp.dev";

const days = Array.from({ length: 84 }, (_, index) => {
  const date = new Date(Date.UTC(2026, 5, 4 + index));
  return {
    date: date.toISOString().slice(0, 10),
    count: index === 83 ? 7 : index % 4,
    level: index === 83 ? 4 : index % 5,
  };
});

const connected: DashboardSnapshot = {
  github: {
    status: "connected", accountLogin: "fixture-user", contributionDays: days,
    currentStreakDays: 12, lastSuccessfulRefresh: "2026-08-29T09:00:00Z", errorKind: null,
  },
  claude: {
    status: "connected", remainingPercent: 59,
    shortWindow: { labelKey: "short", remainingPercent: 83, resetsAt: "2026-09-03T14:00:00Z" },
    weeklyWindow: { labelKey: "weekly", remainingPercent: 59, resetsAt: null },
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z", errorKind: null,
  },
  codex: {
    status: "connected", remainingPercent: 68,
    shortWindow: { labelKey: "short", remainingPercent: 68, resetsAt: null },
    weeklyWindow: { labelKey: "weekly", remainingPercent: 72, resetsAt: "2026-09-04T00:00:00Z" },
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z", errorKind: null,
  },
  grok: {
    status: "connected", remainingPercent: 61,
    shortWindow: null,
    weeklyWindow: { labelKey: "monthly", remainingPercent: 61, resetsAt: "2026-09-15T00:00:00Z" },
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z", errorKind: null,
  },
  cursor: {
    status: "connected", subscriptionTier: "pro", accountEmail: "fixture@cursor.com",
    lastSuccessfulRefresh: "2026-08-29T09:00:00Z", errorKind: null,
  },
  refreshedAt: "2026-08-29T09:00:00Z",
};

afterEach(async () => {
  cleanup();
  window.history.replaceState({}, "", "/");
  await setLocale("en");
});

describe("compact metric rail", () => {
  it.each([
    ["right", "vertical"], ["left", "vertical"], ["top", "horizontal"],
  ] as const)("uses the shared %s placement geometry", (placement, orientation) => {
    render(<NotchApp placement={placement} snapshot={connected} selectedProvider="claude" />);
    expect(screen.getByRole("toolbar", { name: /provider status/i }))
      .toHaveAttribute("aria-orientation", orientation);
    expect(screen.getByRole("button", { name: /settings/i })).toHaveClass("settings-launcher");
    expect(screen.getByTestId("notch-surface")).toHaveClass(`placement-${placement}`);
  });

  it.each(["claude", "codex", "github", "grok", "cursor"] as const)(
    "binds the connector to the selected %s metric",
    (provider) => {
      render(<NotchApp placement="top" snapshot={connected} selectedProvider={provider} />);

      expect(document.querySelector(".notch-join")).toHaveAttribute("data-provider", provider);
      expect(screen.getByTestId("provider-card-region")).toHaveAttribute("data-provider", provider);
    },
  );

  it("uses provider-aligned compact cards while keeping the taller GitHub card centered", () => {
    const view = render(<NotchApp placement="right" snapshot={connected} selectedProvider="claude" />);
    expect(screen.getByTestId("provider-card-region")).toHaveAttribute("data-layout", "compact");

    view.rerender(<NotchApp placement="right" snapshot={connected} selectedProvider="github" />);
    expect(screen.getByTestId("provider-card-region")).toHaveAttribute("data-layout", "tall");

    view.rerender(<NotchApp placement="right" snapshot={connected} selectedProvider="cursor" />);
    expect(screen.getByTestId("provider-card-region")).toHaveAttribute("data-layout", "compact");
  });

  it("keeps a stale cursor card in the bounded tall-card layout", () => {
    render(<NotchApp
      placement="right"
      snapshot={{ ...connected, cursor: { ...connected.cursor, status: "stale" } }}
      selectedProvider="cursor"
    />);

    expect(screen.getByTestId("provider-card-region")).toHaveAttribute("data-layout", "tall");
  });

  it("keeps stale usage cards in the bounded tall-card layout", () => {
    render(<NotchApp
      placement="left"
      snapshot={{ ...connected, claude: { ...connected.claude, status: "stale" } }}
      selectedProvider="claude"
    />);

    expect(screen.getByTestId("provider-card-region")).toHaveAttribute("data-layout", "tall");
  });

  it("uses real provider summaries, a localized streak, and accessible hit targets", () => {
    render(<NotchApp placement="right" snapshot={connected} selectedProvider="claude" />);
    const buttons = screen.getAllByRole("button", { name: /Claude|Codex|GitHub|Grok|Cursor/ });
    expect(buttons).toHaveLength(5);
    buttons.forEach((button) => expect(button).toHaveClass("metric-button"));
    expect(screen.getByRole("progressbar", { name: /Claude/i })).toHaveAttribute("aria-valuenow", "59");
    expect(screen.getByRole("progressbar", { name: /Codex/i })).toHaveAttribute("aria-valuenow", "68");
    expect(screen.getByRole("progressbar", { name: /Grok/i })).toHaveAttribute("aria-valuenow", "61");
    expect(screen.getByRole("group", { name: /GitHub/i })).not.toHaveAttribute("aria-valuenow");
    expect(screen.getByRole("group", { name: /Cursor/i })).not.toHaveAttribute("aria-valuenow");
    expect(screen.getByText("12d")).toBeInTheDocument();
    expect(screen.getByText("pro")).toHaveClass("metric-value--text");
  });

  it("announces a connected cursor account without a tier as connected", () => {
    render(<NotchApp
      placement="right"
      snapshot={{
        ...connected,
        cursor: { ...connected.cursor, subscriptionTier: null, accountEmail: null },
      }}
      selectedProvider="claude"
    />);

    expect(screen.getByRole("button", { name: "Cursor: Connected" })).toBeInTheDocument();
  });

  it.each([
    ["connected", "59%", "metric-value"],
    ["loading", "Loading", "metric-status"],
    ["unavailable", "Unavailable", "metric-status"],
    ["notInstalled", "Not installed", "metric-status"],
    ["notAuthenticated", "Sign in required", "metric-status"],
    ["stale", "Last known data", "metric-status"],
  ] as const)(
    "keeps exactly one localized line below the top ring for %s",
    (status, expectedText, expectedClass) => {
      const statusSnapshot: DashboardSnapshot | null = status === "loading"
        ? null
        : {
          ...connected,
          claude: {
            ...connected.claude,
            status,
            remainingPercent: ["unavailable", "notInstalled", "notAuthenticated"].includes(status)
              ? null
              : connected.claude.remainingPercent,
          },
        };
      render(<MetricRail
        providers={["claude", "codex", "github"]}
        placement="top"
        snapshot={statusSnapshot}
        selectedProvider="claude"
        onSelect={() => undefined}
      />);

      const button = screen.getByRole("button", { name: /Claude/i });
      const compactLines = button.querySelectorAll(":scope > .metric-value, :scope > .metric-status");
      expect(compactLines).toHaveLength(1);
      expect(compactLines[0]).toHaveClass(expectedClass);
      expect(compactLines[0]).toHaveTextContent(expectedText);
      expect(button).toHaveAccessibleName(expect.stringContaining(expectedText));
      expect(within(button).getByTestId("provider-glyph-claude")).toBeInTheDocument();
    },
  );

  it("never fabricates zeroes for unavailable providers", () => {
    const unavailable: DashboardSnapshot = {
      ...connected,
      claude: { ...connected.claude, status: "unavailable", remainingPercent: null, shortWindow: null, weeklyWindow: null },
      codex: { ...connected.codex, status: "notInstalled", remainingPercent: null, shortWindow: null, weeklyWindow: null },
      github: { ...connected.github, status: "notAuthenticated", currentStreakDays: null, contributionDays: null },
    };
    render(<NotchApp placement="right" snapshot={unavailable} selectedProvider="claude" />);
    expect(screen.queryByText("0%")).not.toBeInTheDocument();
    expect(screen.getAllByText("Unavailable").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Not installed").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Sign in required").length).toBeGreaterThan(0);
  });

  it.each(["connected", "stale"] as const)("keeps a %s GitHub rail neutral when today's record is absent", (status) => {
    const withoutToday: DashboardSnapshot = {
      ...connected,
      github: {
        ...connected.github,
        status,
        contributionDays: connected.github.contributionDays!.filter((day) => day.date !== "2026-08-26"),
      },
    };
    render(<MetricRail
      providers={["claude", "codex", "github"]}
      placement="right"
      snapshot={withoutToday}
      selectedProvider="github"
      onSelect={() => undefined}
      now={new Date(2026, 7, 26, 12)}
    />);
    const button = screen.getByRole("button", { name: /GitHub/i });
    const ring = within(button).getByRole("group");
    expect(ring).toHaveClass("is-neutral");
    expect(ring.querySelector("circle[data-ring-value]")).not.toBeInTheDocument();
    expect(button).toHaveAccessibleName(expect.stringContaining("Today: Unavailable"));
    if (status === "stale") expect(button).toHaveAccessibleName(expect.stringContaining("Last known data"));
  });

  it("keeps an actual level-zero GitHub day distinct from a missing day", () => {
    const zeroToday: DashboardSnapshot = {
      ...connected,
      github: {
        ...connected.github,
        contributionDays: connected.github.contributionDays!.map((day) => day.date === "2026-08-26"
          ? { ...day, count: 0, level: 0 }
          : day),
      },
    };
    render(<MetricRail
      providers={["claude", "codex", "github"]}
      placement="right"
      snapshot={zeroToday}
      selectedProvider="github"
      onSelect={() => undefined}
      now={new Date(2026, 7, 26, 12)}
    />);
    const button = screen.getByRole("button", { name: /GitHub/i });
    const ring = within(button).getByRole("group");
    expect(ring).not.toHaveClass("is-neutral");
    expect(ring.querySelector("circle[data-ring-value]")).toHaveAttribute("data-ring-value", "0");
    expect(button).toHaveAccessibleName(expect.stringContaining("Today: 0 contributions"));
    expect(button).not.toHaveAccessibleName(expect.stringContaining("Unavailable"));
  });

  it("renders provider glyphs only in compact rings", () => {
    render(<NotchApp placement="right" snapshot={connected} selectedProvider="claude" />);
    expect(screen.getAllByTestId(/provider-glyph-/)).toHaveLength(5);
    expect(within(screen.getByRole("article")).queryByTestId(/provider-glyph-/)).not.toBeInTheDocument();
  });

  it("marks a development visual fixture ready only after locale and provider state settle", async () => {
    window.history.replaceState({}, "", "/?fixture=1&placement=left&background=bright&provider=github&locale=ar");
    render(<VisualFixtureApp />);

    await waitFor(() => expect(screen.getByTestId("notch-app")).toHaveAttribute("data-fixture-ready", "true"));
    expect(document.documentElement).toHaveAttribute("lang", "ar");
    expect(screen.getByTestId("notch-surface")).toHaveClass("placement-left");
    expect(screen.getByRole("article")).toHaveClass("provider-github");
  });
});

describe("provider details", () => {
  it("renders grok's single monthly billing window without a weekly label", () => {
    render(<UsageProviderCard provider="grok" snapshot={connected.grok} />);
    expect(screen.getByRole("heading", { name: "Grok" })).toBeInTheDocument();
    expect(screen.getByText("Monthly")).toBeInTheDocument();
    expect(screen.getByText("61% remaining")).toBeInTheDocument();
    expect(screen.queryByText("Weekly")).not.toBeInTheDocument();
    expect(screen.getByText(`Resets ${formatDateTime("2026-09-15T00:00:00Z")}`)).toBeInTheDocument();
  });

  it("renders the cursor account card with plan, account, and no percentages", () => {
    render(<CursorCard snapshot={connected.cursor} />);
    expect(screen.getByRole("heading", { name: "Cursor" })).toBeInTheDocument();
    expect(screen.getByText("Plan")).toBeInTheDocument();
    expect(screen.getByTestId("cursor-plan-value")).toHaveTextContent("pro");
    expect(screen.getByTestId("cursor-account-value")).toHaveTextContent("fixture@cursor.com");
    expect(screen.getByText(/Cursor does not report usage limits/)).toBeInTheDocument();
    expect(screen.queryByText(/%/)).not.toBeInTheDocument();
  });

  it("keeps absent cursor account fields as explicit unavailability", () => {
    render(<CursorCard snapshot={{
      ...connected.cursor,
      subscriptionTier: null,
      accountEmail: null,
    }} />);
    expect(within(screen.getByTestId("cursor-plan-value")).getByText("Unavailable")).toBeInTheDocument();
    expect(within(screen.getByTestId("cursor-account-value")).getByText("Unavailable")).toBeInTheDocument();
  });

  it("shows install guidance for a missing cursor CLI", () => {
    render(<CursorCard snapshot={{ ...connected.cursor, status: "notInstalled", subscriptionTier: null, accountEmail: null }} />);
    expect(screen.getByText("Not installed")).toBeInTheDocument();
    expect(screen.getByText("Install the Cursor CLI, then reopen Dashy.")).toBeInTheDocument();
  });

  it("renders independent short and weekly usage windows with localized reset times", () => {
    render(<UsageProviderCard provider="claude" snapshot={connected.claude} />);
    expect(screen.getByRole("heading", { name: "Claude" })).toBeInTheDocument();
    expect(screen.getByText("Current session")).toBeInTheDocument();
    expect(screen.getByText("83% remaining")).toBeInTheDocument();
    expect(screen.getByText("Weekly")).toBeInTheDocument();
    expect(screen.getByText("59% remaining")).toBeInTheDocument();
    expect(screen.getByText(`Resets ${formatDateTime("2026-09-03T14:00:00Z")}`)).toBeInTheDocument();
    expect(screen.queryByText(/Thursday 12:00 AM/)).not.toBeInTheDocument();
  });

  it.each(["he", "ar"] as const)("renders only the structured reset timestamp in %s", async (locale) => {
    await setLocale(locale);
    render(<UsageProviderCard provider="claude" snapshot={{
      ...connected.claude,
      shortWindow: { ...connected.claude.shortWindow!, resetsAt: "2026-09-03T14:00:00Z" },
      weeklyWindow: { ...connected.claude.weeklyWindow!, resetsAt: null },
    }} />);
    const localizedReset = formatDateTime("2026-09-03T14:00:00Z", locale);
    expect(screen.getByText((content) => content.includes(localizedReset))).toBeInTheDocument();
    expect(screen.queryByText(/Sep 3 at 2:00 PM|Thursday 12:00 AM/)).not.toBeInTheDocument();
  });

  it("renders local-date GitHub activity, streak, and an aligned 84-day heatmap without a percentage", () => {
    render(<GitHubCard snapshot={connected.github} now={new Date(2026, 7, 26, 12)} />);
    const juneFourth = new Intl.DateTimeFormat("en", { dateStyle: "medium", timeZone: "UTC" })
      .format(new Date("2026-06-04T00:00:00Z"));
    const juneSeventh = new Intl.DateTimeFormat("en", { dateStyle: "medium", timeZone: "UTC" })
      .format(new Date("2026-06-07T00:00:00Z"));
    expect(screen.getByText("12 day streak")).toBeInTheDocument();
    expect(screen.getByText("7 contributions")).toBeInTheDocument();
    expect(screen.getByRole("list", { name: /last 12 weeks/i }).children).toHaveLength(84);
    expect(screen.getByLabelText(`0 contributions — ${juneFourth}`)).toHaveStyle({ gridRow: "5", gridColumn: "1" });
    expect(screen.getByLabelText(`3 contributions — ${juneSeventh}`)).toHaveStyle({ gridRow: "1", gridColumn: "2" });
    expect(screen.queryByText(/12%/)).not.toBeInTheDocument();
  });

  it.each([
    ["connected", null],
    ["stale", "Last known data"],
    ["notInstalled", "Install the Claude CLI"],
    ["notAuthenticated", "Sign in to Claude"],
    ["unavailable", "Try Claude again later"],
    ["loading", "Loading"],
  ] as const)("centralizes the %s provider state", (status, expected) => {
    const snapshot = { ...connected.claude, status: status === "loading" ? "unavailable" : status };
    render(<UsageProviderCard provider="claude" snapshot={status === "loading" ? null : snapshot} />);
    const card = screen.getByRole("article");
    expect(card).toHaveAttribute("data-status", status);
    if (expected) expect(within(card).getByText(new RegExp(expected))).toBeInTheDocument();
    expect(within(card).queryByText(snapshot.errorKind ?? "raw backend error")).not.toBeInTheDocument();
  });

  it("mirrors inner content for Hebrew without changing right-edge placement", async () => {
    await setLocale("he");
    render(<NotchApp placement="right" snapshot={connected} selectedProvider="github" />);
    expect(screen.getByTestId("notch-surface")).toHaveClass("placement-right");
    expect(screen.getByRole("article")).toHaveAttribute("dir", "rtl");
    expect(screen.getByText(/רצף של 12 ימים/)).toBeInTheDocument();
  });

  it("localizes actionable provider states in an RTL locale", async () => {
    await setLocale("he");
    render(<UsageProviderCard provider="claude" snapshot={{ ...connected.claude, status: "notInstalled" }} />);
    expect(screen.getByText("לא מותקן")).toBeInTheDocument();
    expect(screen.getByText(/התקינו את כלי שורת הפקודה של Claude/)).toBeInTheDocument();
    expect(screen.getByRole("article")).toHaveAttribute("dir", "rtl");
  });

  it("formats every visible and accessible Arabic number with the active locale", async () => {
    await setLocale("ar");
    const localizedSnapshot: DashboardSnapshot = {
      ...connected,
      github: {
        ...connected.github,
        currentStreakDays: 1234,
        contributionDays: connected.github.contributionDays!.map((day) => day.date === "2026-08-26" ? { ...day, count: 1234 } : day),
      },
    };
    const view = render(<NotchApp placement="right" snapshot={localizedSnapshot} selectedProvider="github" />);
    const claudeRemaining = formatNumber(59, "ar");
    const streak = formatNumber(1234, "ar");
    const todayCount = formatNumber(1234, "ar");
    expect(screen.getByRole("button", { name: new RegExp(`Claude.*${claudeRemaining}`) })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: new RegExp(`GitHub.*${streak}`) })).toBeInTheDocument();
    view.unmount();
    render(<GitHubCard snapshot={localizedSnapshot.github} now={new Date(2026, 7, 26, 12)} />);
    expect(screen.getByTestId("github-streak-value")).toHaveTextContent(streak);
    expect(screen.getByTestId("github-today-value")).toHaveTextContent(todayCount);
    const todayCell = screen.getByLabelText(new RegExp(todayCount));
    const localizedToday = new Intl.DateTimeFormat("ar", { dateStyle: "medium", timeZone: "UTC" })
      .format(new Date("2026-08-26T00:00:00Z"));
    expect(todayCell).toHaveAccessibleName(expect.stringContaining(localizedToday));
    expect(todayCell).not.toHaveAccessibleName(expect.stringContaining("2026-08-26"));
    expect(screen.queryByText(/1234/)).not.toBeInTheDocument();
  });

  it("does not invent a zero streak when connected GitHub data omits the streak", () => {
    render(<GitHubCard snapshot={{ ...connected.github, currentStreakDays: null }} now={new Date(2026, 7, 26, 12)} />);
    expect(screen.getByTestId("github-streak-value")).toHaveTextContent("—");
    expect(within(screen.getByTestId("github-streak-value")).getByText("Unavailable")).toBeInTheDocument();
    expect(screen.queryByText(/0 day streak/)).not.toBeInTheDocument();
  });

  it("does not invent zero contributions when today's local-date record is absent", () => {
    render(<GitHubCard snapshot={{
      ...connected.github,
      contributionDays: connected.github.contributionDays!.filter((day) => day.date !== "2026-08-26"),
    }} now={new Date(2026, 7, 26, 12)} />);
    expect(screen.getByTestId("github-today-value")).toHaveTextContent("—");
    expect(within(screen.getByTestId("github-today-value")).getByText("Unavailable")).toBeInTheDocument();
    expect(screen.queryByText(/0 contributions/)).not.toBeInTheDocument();
  });
});

describe("ProgressRing", () => {
  it("clamps numeric values and keeps GitHub intensity non-progress semantic", () => {
    const { rerender } = render(<ProgressRing value={140} label="Claude remaining"><span>100%</span></ProgressRing>);
    expect(screen.getByRole("progressbar")).toHaveAttribute("aria-valuenow", "100");
    expect(document.querySelector("circle[data-ring-value]"))?.toHaveAttribute("data-ring-value", "100");
    rerender(<ProgressRing value={-10} label="GitHub activity" semantic="activity"><span>2d</span></ProgressRing>);
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(screen.getByRole("group", { name: "GitHub activity" })).toBeInTheDocument();
    expect(document.querySelector("circle[data-ring-value]"))?.toHaveAttribute("data-ring-value", "0");
  });
});
