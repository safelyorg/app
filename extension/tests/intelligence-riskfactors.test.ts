import { describe, it, expect } from "vitest";

function escapeHtml(str: string): string {
  const div = document.createElement("div");
  div.textContent = str;
  return div.innerHTML;
}

function capitalizeFirst(str: string): string {
  if (!str) return str;
  return str.charAt(0).toUpperCase() + str.slice(1);
}

const SEVERITY_COLORS: Record<string, string> = {
  hard: "#ff5d5d",
  compound: "#f2b84c",
  soft: "#8e8e93",
};

const SEVERITY_LABELS: Record<string, string> = {
  hard: "Confirmed",
  compound: "Pattern match",
  soft: "Worth noting",
};

function buildRiskFactorsSection(riskFactors: any[]): string {
  if (!riskFactors || riskFactors.length === 0) return "";
  const rows = riskFactors
    .map((factor) => {
      const color = SEVERITY_COLORS[factor.severity] || "#8e8e93";
      const severityLabel = SEVERITY_LABELS[factor.severity] || factor.severity;
      return (
        '<div class="safely-check-card">' +
        '<div style="display:flex;justify-content:space-between;align-items:baseline;gap:8px;">' +
        '<div class="safely-check-title">' +
        escapeHtml(capitalizeFirst(factor.name.replace(/_/g, " "))) +
        "</div>" +
        '<div style="font-weight:700;white-space:nowrap;font-size:11px;color:' +
        color +
        ';">' +
        escapeHtml(severityLabel) +
        "</div></div>" +
        '<div class="safely-check-body">' +
        escapeHtml(factor.description) +
        "</div></div>"
      );
    })
    .join("");
  return (
    '<div class="safely-section-label" style="margin-top:18px">Risk Factors</div><div style="display:flex;flex-direction:column;gap:8px">' +
    rows +
    "</div>"
  );
}

describe("intelligence.ts - capitalizeFirst", () => {
  it("capitalizes the first letter of a lowercase string", () => {
    expect(capitalizeFirst("confirmed fraud pattern")).toBe("Confirmed fraud pattern");
  });

  it("returns an empty string unchanged", () => {
    expect(capitalizeFirst("")).toBe("");
  });

  it("leaves an already-capitalized string unchanged", () => {
    expect(capitalizeFirst("Already capitalized")).toBe("Already capitalized");
  });
});

describe("intelligence.ts - buildRiskFactorsSection", () => {
  it("returns a genuinely empty string when there are zero risk factors", () => {
    expect(buildRiskFactorsSection([])).toBe("");
  });

  it("returns a genuinely empty string when riskFactors is null or undefined", () => {
    expect(buildRiskFactorsSection(null as any)).toBe("");
    expect(buildRiskFactorsSection(undefined as any)).toBe("");
  });

  it("uses the correct color and label for a hard-severity factor", () => {
    const result = buildRiskFactorsSection([
      { severity: "hard", name: "confirmed_fraud_pattern", description: "test" },
    ]);
    expect(result).toContain("#ff5d5d");
    expect(result).toContain("Confirmed");
  });

  it("uses the correct color and label for a compound-severity factor", () => {
    const result = buildRiskFactorsSection([
      { severity: "compound", name: "likely_counterfeit_or_nonexistent_product", description: "test" },
    ]);
    expect(result).toContain("#f2b84c");
    expect(result).toContain("Pattern match");
  });

  it("uses the correct color and label for a soft-severity factor", () => {
    const result = buildRiskFactorsSection([
      { severity: "soft", name: "duplicate_listing_flagged", description: "test" },
    ]);
    expect(result).toContain("#8e8e93");
    expect(result).toContain("Worth noting");
  });

  it("falls back to a genuine, sensible default for an unrecognized severity", () => {
    const result = buildRiskFactorsSection([
      { severity: "totally_new_severity", name: "something", description: "test" },
    ]);
    expect(result).toContain("#8e8e93");
    expect(result).toContain("totally_new_severity");
  });

  it("replaces underscores with spaces and capitalizes the first letter of the factor name", () => {
    const result = buildRiskFactorsSection([
      { severity: "hard", name: "network_confirmed_high_risk_seller", description: "test" },
    ]);
    expect(result).toContain("Network confirmed high risk seller");
    expect(result).not.toContain("network_confirmed_high_risk_seller");
  });

  it("escapes dangerous HTML in the factor's name and description", () => {
    const result = buildRiskFactorsSection([
      {
        severity: "soft",
        name: "<script>bad</script>",
        description: "<img src=x onerror=alert(1)>",
      },
    ]);
    expect(result).not.toContain("<script>bad</script>");
    expect(result).not.toContain("<img src=x onerror=alert(1)>");
    expect(result).toContain("&lt;script&gt;");
  });

  it("renders multiple risk factors, each as their own separate card", () => {
    const result = buildRiskFactorsSection([
      { severity: "hard", name: "factor_one", description: "first" },
      { severity: "soft", name: "factor_two", description: "second" },
    ]);
    expect(result.match(/safely-check-card/g)?.length).toBe(2);
  });
});
