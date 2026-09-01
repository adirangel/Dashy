import { describe, expect, it } from "vitest";
import { formatDateTime, formatRelativeTime } from "./i18n";

const now = new Date(2026, 7, 26, 12, 0, 0);
const at = (minutes: number) => new Date(now.getTime() + minutes * 60_000);

describe("formatRelativeTime", () => {
  it("uses minutes under an hour, hours under a day, and days beyond", () => {
    expect(formatRelativeTime(at(1), now, "en")).toBe("in 1 minute");
    expect(formatRelativeTime(at(45), now, "en")).toBe("in 45 minutes");
    expect(formatRelativeTime(at(60), now, "en")).toBe("in 1 hour");
    expect(formatRelativeTime(at(111), now, "en")).toBe("in 2 hours");
    expect(formatRelativeTime(at(23 * 60 + 40), now, "en")).toBe("in 1 day");
    expect(formatRelativeTime(at(8 * 24 * 60 + 5 * 60), now, "en")).toBe("in 8 days");
  });

  it("localizes the phrase without any hand-written unit strings", () => {
    expect(formatRelativeTime(at(120), now, "he")).toBe("בעוד שעתיים");
    expect(formatRelativeTime(at(3 * 24 * 60), now, "fr")).toBe("dans 3 jours");
  });

  it("falls back to the absolute time once the moment has passed", () => {
    expect(formatRelativeTime(at(-5), now, "en")).toBe(formatDateTime(at(-5), "en"));
    expect(formatRelativeTime(now, now, "en")).toBe(formatDateTime(now, "en"));
  });

  it("returns an empty string for an unparseable value", () => {
    expect(formatRelativeTime("not-a-date", now, "en")).toBe("");
  });
});
