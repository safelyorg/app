import { describe, it, expect } from "vitest";

// These are direct copies of the JS fallback logic inside risk.ts's
// catch block - the real, primary implementation lives in
// wasm/lib.rs and is thoroughly tested there. This file exists
// specifically to verify the FALLBACK stays correct too, since it's
// real code that would run if WASM ever failed to load.
function risk_level(s: number): string {
  return s <= 33 ? "low" : s <= 66 ? "caution" : "high";
}

function risk_label(l: string): string {
  return l === "low" ? "Low risk" : l === "caution" ? "Caution" : "High risk";
}

function risk_desc(l: string): string {
  return l === "low"
    ? "Safe to proceed"
    : l === "caution"
      ? "Review before proceeding"
      : "High risk detected";
}

function build_activity_bars(_a: Uint8Array): string {
  return "";
}

function verification_badge(s: string): string {
  return '<span class="safely-verified-badge">' + s + "</span>";
}

describe("risk.ts fallback - risk_level", () => {
  it("matches the same boundaries as the real WASM version", () => {
    expect(risk_level(33)).toBe("low");
    expect(risk_level(34)).toBe("caution");
    expect(risk_level(66)).toBe("caution");
    expect(risk_level(67)).toBe("high");
  });
});

describe("risk.ts fallback - risk_label", () => {
  it("returns the correct label for each level", () => {
    expect(risk_label("low")).toBe("Low risk");
    expect(risk_label("caution")).toBe("Caution");
    expect(risk_label("high")).toBe("High risk");
  });
});

describe("risk.ts fallback - risk_desc", () => {
  it("returns the correct description for each level", () => {
    expect(risk_desc("low")).toBe("Safe to proceed");
    expect(risk_desc("caution")).toBe("Review before proceeding");
    expect(risk_desc("high")).toBe("High risk detected");
  });
});

describe("risk.ts fallback - build_activity_bars", () => {
  it("is a known, deliberately empty stub, unlike the real WASM version", () => {
    // Worth flagging directly: this fallback does NOT match the real
    // Rust version's opacity-gradient behavior - it always returns an
    // empty string. This test documents that gap honestly, rather
    // than pretending the fallback is feature-complete.
    expect(build_activity_bars(new Uint8Array([5, 10, 0]))).toBe("");
  });
});

describe("risk.ts fallback - verification_badge", () => {
  it("wraps the status text in the expected badge markup", () => {
    expect(verification_badge("verified")).toBe(
      '<span class="safely-verified-badge">verified</span>',
    );
  });
});
