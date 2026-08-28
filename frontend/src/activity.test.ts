import { describe, expect, it } from "vitest";
import { currentStreak } from "./activity";

describe("currentStreak", () => {
  it("counts consecutive active days from today", () => expect(currentStreak([1, 0, 2, 3, 1])).toBe(3));
  it("returns zero when today is inactive", () => expect(currentStreak([1, 2, 0])).toBe(0));
});
