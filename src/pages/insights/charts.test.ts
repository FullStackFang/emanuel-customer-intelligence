import { describe, expect, it } from "vitest";
import { reasonChartSeries } from "./charts";

describe("reasonChartSeries", () => {
  it("keeps every returned primary exit outcome out of Other", () => {
    const result = reasonChartSeries([
      { fy: 2026, reason: "Administrative or Unknown Exit", n: 2 },
      { fy: 2026, reason: "Structural Exit", n: 3 },
      { fy: 2026, reason: "Conversion Loss", n: 4 },
      { fy: 2026, reason: "Addressable Churn", n: 5 },
    ]);

    expect(result.categories).toEqual([
      "Addressable Churn",
      "Conversion Loss",
      "Structural Exit",
      "Administrative or Unknown Exit",
    ]);
    expect(result.data).toEqual([{
      fy: "FY2026",
      "Addressable Churn": 5,
      "Conversion Loss": 4,
      "Structural Exit": 3,
      "Administrative or Unknown Exit": 2,
    }]);
  });

  it("renders a future category as its own deterministically ordered series", () => {
    const result = reasonChartSeries([
      { fy: 2026, reason: "Zeta outcome", n: 1 },
      { fy: 2026, reason: "Alpha outcome", n: 2 },
    ]);

    expect(result.categories).toEqual(["Alpha outcome", "Zeta outcome"]);
    expect(result.data[0]).not.toHaveProperty("Other");
  });
});
