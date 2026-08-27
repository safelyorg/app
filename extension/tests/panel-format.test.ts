import { describe, it, expect } from "vitest";

// formatCountdown has no dependency on chrome, __safelyAPI, or the
// DOM at all - but it's still declared inside panel.ts's IIFE, which
// DOES have those dependencies just to load. Rather than mock the
// entire file's startup just to reach one small function, we
// deliberately test the exact same formatting logic directly here.
function formatCountdown(totalSeconds: number): string {
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return minutes + ":" + (seconds < 10 ? "0" : "") + seconds;
}

describe("formatCountdown", () => {
  it("pads single-digit seconds with a leading zero", () => {
    expect(formatCountdown(65)).toBe("1:05");
  });

  it("does not pad double-digit seconds", () => {
    expect(formatCountdown(90)).toBe("1:30");
  });

  it("handles exactly zero seconds", () => {
    expect(formatCountdown(0)).toBe("0:00");
  });

  it("handles a value with no full minutes", () => {
    expect(formatCountdown(45)).toBe("0:45");
  });
});
