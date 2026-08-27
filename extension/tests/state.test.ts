import { describe, it, expect } from "vitest";
import "../ts/core/state";

const defaultData = () => {
  (window as any).__safelyResetState();
  return (window as any).__safelyData;
};

describe("defaultData (via __safelyResetState)", () => {
  it("returns the correct, complete default shape", () => {
    const result = defaultData();
    expect(result.riskScore).toBe(0);
    expect(result.fraudReportCount).toBe(0);
    expect(result.signals).toEqual([]);
    expect(result.seller.name).toBe("Unknown");
    expect(result.seller.verification).toBe("unknown");
    expect(result.seller.monthlyActivity).toHaveLength(12);
    expect(result.seller.monthlyActivity.every((v: number) => v === 0)).toBe(true);
  });

  it("returns a genuinely fresh object each time, not a shared reference", () => {
    const first = defaultData();
    first.seller.monthlyActivity[0] = 99;
    const second = defaultData();
    expect(second.seller.monthlyActivity[0]).toBe(0);
  });
});
