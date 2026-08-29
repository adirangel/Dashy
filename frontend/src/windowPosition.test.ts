import { describe, expect, it } from "vitest";
import { topRightPosition } from "./windowPosition";

describe("topRightPosition", () => {
  it("places the window inside the monitor work area with a scaled margin", () => {
    expect(topRightPosition({
      workArea: {
        position: { x: 100, y: 50 },
        size: { width: 1920, height: 1040 },
      },
      windowSize: { width: 570, height: 810 },
      scaleFactor: 1.5,
      margin: 18,
    })).toEqual({ x: 1423, y: 77 });
  });

  it("keeps an oversized window reachable from the work area's top-left edge", () => {
    expect(topRightPosition({
      workArea: {
        position: { x: -1280, y: 0 },
        size: { width: 800, height: 600 },
      },
      windowSize: { width: 900, height: 700 },
      scaleFactor: 1,
      margin: 18,
    })).toEqual({ x: -1262, y: 18 });
  });
});
