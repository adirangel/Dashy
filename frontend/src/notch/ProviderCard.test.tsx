import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { unavailableDashboardSnapshot } from "../dashboard";
import { directionForLocale, localeResources, setLocale, SUPPORTED_LOCALES } from "../i18n";
import { CursorCard } from "./CursorCard";
import { GitHubCard } from "./GitHubCard";
import { ProviderCard } from "./ProviderCard";
import { UsageProviderCard } from "./UsageProviderCard";

const errorKinds = ["usageUnavailable", "unsupportedOutput", "timeout", "launch", "process", "network"] as const;

afterEach(async () => {
  cleanup();
  await setLocale("en");
});

describe("provider failure explanations", () => {
  it.each(SUPPORTED_LOCALES)("localizes every safe failure category in %s", async (locale) => {
    await setLocale(locale);
    for (const errorKind of errorKinds) {
      const view = render(<UsageProviderCard provider="codex" snapshot={{
        ...unavailableDashboardSnapshot().codex, errorKind,
      }} />);
      expect(screen.getByText(localeResources[locale].translation.guidance[errorKind]
        .replace("{{provider}}", "Codex"))).toBeInTheDocument();
      expect(screen.getByRole("article")).toHaveAttribute("dir", directionForLocale(locale));
      expect(screen.queryByText(/0%|100%/)).not.toBeInTheDocument();
      expect(screen.queryByText(errorKind)).not.toBeInTheDocument();
      view.unmount();
    }
  });

  it.each([null, "unexpected secret CLI output"])("uses a safe fallback for %s", (errorKind) => {
    render(<UsageProviderCard provider="claude" snapshot={{
      ...unavailableDashboardSnapshot().claude, errorKind,
    }} />);
    expect(screen.getByText("Try Claude again later.")).toBeInTheDocument();
    expect(screen.queryByText("unexpected secret CLI output")).not.toBeInTheDocument();
  });

  it.each(["claude", "codex", "grok"] as const)("passes %s errors to the shared card", (provider) => {
    render(<UsageProviderCard provider={provider} snapshot={{
      ...unavailableDashboardSnapshot()[provider], errorKind: "usageUnavailable",
    }} />);
    expect(screen.getByText(/is signed in but returned no usage limits/)).toBeInTheDocument();
  });

  it("preserves real stale usage and identifies why the newest refresh failed", () => {
    render(<UsageProviderCard provider="codex" snapshot={{
      ...unavailableDashboardSnapshot().codex,
      status: "stale", errorKind: "timeout", lastSuccessfulRefresh: "2026-08-29T09:00:00Z",
      remainingPercent: 37,
      shortWindow: { labelKey: "short", remainingPercent: 37, resetsAt: null },
    }} />);
    expect(screen.getByText("37% remaining")).toBeInTheDocument();
    expect(screen.getByText("Last known data")).toBeInTheDocument();
    expect(screen.getByText(/CLI took too long to respond/)).toBeInTheDocument();
  });

  it("shows connected Codex weekly usage without inventing an unknown reset", () => {
    render(<UsageProviderCard provider="codex" snapshot={{
      ...unavailableDashboardSnapshot().codex,
      status: "connected", errorKind: null, remainingPercent: 72,
      weeklyWindow: { labelKey: "weekly", remainingPercent: 72, resetsAt: null },
    }} />);
    expect(screen.getByText("Connected")).toBeInTheDocument();
    expect(screen.getByText("Weekly")).toBeInTheDocument();
    expect(screen.getByText("72% remaining")).toBeInTheDocument();
    expect(screen.queryByText("Current session")).not.toBeInTheDocument();
    expect(screen.queryByText(/Resets/)).not.toBeInTheDocument();
    expect(screen.queryByText("Last known data")).not.toBeInTheDocument();
  });

  it.each([
    ["missingExecutable", "Install the Codex CLI, then reopen Dashy."],
    ["authentication", "Sign in to Codex, then retry."],
  ])("preserves actionable %s repairs for cached data", (errorKind, expected) => {
    render(<ProviderCard provider="codex" status="stale" errorKind={errorKind} />);
    expect(screen.getByText(expected)).toBeInTheDocument();
  });

  it("passes failures through the GitHub and Cursor cards too", () => {
    const snapshot = unavailableDashboardSnapshot();
    render(<><GitHubCard snapshot={{ ...snapshot.github, errorKind: "launch" }} />
      <CursorCard snapshot={{ ...snapshot.cursor, errorKind: "network" }} /></>);
    expect(screen.getByText(/could not start the GitHub CLI/)).toBeInTheDocument();
    expect(screen.getByText(/Cursor CLI could not reach its service/)).toBeInTheDocument();
  });

  it.each(["connected", "loading"] as const)("does not show an old failure in %s state", (status) => {
    render(<ProviderCard provider="codex" status={status} errorKind="timeout" />);
    expect(screen.queryByText(/took too long/)).not.toBeInTheDocument();
  });
});
