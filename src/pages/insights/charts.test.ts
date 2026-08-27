import { describe, expect, it } from "vitest";
import { rankedReasons } from "./charts";

describe("rankedReasons", () => {
  it("sums each fine reason across fiscal years and ranks high to low", () => {
    const ranked = rankedReasons([
      { fy: 2025, reason: "Non-payment", n: 4 },
      { fy: 2026, reason: "Non-payment", n: 6 },
      { fy: 2026, reason: "Moved", n: 7 },
      { fy: 2026, reason: "Displeased", n: 2 },
    ]);

    expect(ranked).toEqual([
      { reason: "Non-payment", n: 10 },
      { reason: "Moved", n: 7 },
      { reason: "Displeased", n: 2 },
    ]);
  });

  it("keeps affordability and disengagement as distinct rows", () => {
    const ranked = rankedReasons([
      { fy: 2026, reason: "Financial hardship", n: 3 },
      { fy: 2026, reason: "No longer engaged", n: 3 },
    ]);

    // Same count: distinct reasons, tie broken by name — never merged.
    expect(ranked.map((r) => r.reason)).toEqual(["Financial hardship", "No longer engaged"]);
  });
});
