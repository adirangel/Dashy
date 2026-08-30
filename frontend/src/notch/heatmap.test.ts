import { describe, expect, it } from "vitest";
import { localIsoDate, positionContributionDays } from "./heatmap";

describe("heatmap helpers", () => {
  it("uses the host local calendar date rather than a UTC substring", () => {
    expect(localIsoDate(new Date(2026, 7, 26, 23, 30))).toBe("2026-08-26");
  });

  it("aligns date-only contribution records to calendar weeks", () => {
    const positioned = positionContributionDays([
      { date: "2026-06-04", count: 1, level: 1 },
      { date: "2026-06-07", count: 2, level: 2 },
    ]);
    expect(positioned[0]?.style).toEqual({ gridRow: 5, gridColumn: 1 });
    expect(positioned[1]?.style).toEqual({ gridRow: 1, gridColumn: 2 });
  });
});
