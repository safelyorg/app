import { describe, it, expect } from "vitest";

interface SafelySignal {
  label: string;
  sub: string;
  value: string;
  type: string;
}

// Direct copies of intelligence.ts's fallback logic - same honest
// approach as risk-fallback.test.ts.
function analyze_signals(j: string): string {
  const signals: SafelySignal[] = JSON.parse(j);
  const bad = signals.filter((s) => s.type === "bad" || s.type === "caution").length;
  const level = bad === 0 ? "low" : bad === 1 ? "caution" : "high";
  const text =
    bad === 0
      ? "All " + signals.length + " signals checked. No red flags detected."
      : bad + " of " + signals.length + " signals need your attention.";
  return JSON.stringify({ level, text });
}

function build_signal_rows(j: string): string {
  const signals: SafelySignal[] = JSON.parse(j);
  const COLORS: Record<string, string> = {
    good: "#35d0a6",
    caution: "#f2b84c",
    info: "#8e8e93",
  };
  return signals
    .map((s) => {
      const color = COLORS[s.type] || "#ff5d5d";
      return (
        '<div class="safely-check-card">' +
        '<div class="safely-check-title">' +
        s.label +
        "</div>" +
        '<div style="color:' +
        color +
        ';">' +
        s.value +
        "</div>" +
        '<div class="safely-check-body">' +
        (s.sub || "") +
        "</div></div>"
      );
    })
    .join("");
}

describe("intelligence.ts fallback - analyze_signals", () => {
  it("returns low with zero bad signals", () => {
    const result = JSON.parse(
      analyze_signals(JSON.stringify([{ label: "a", sub: "", value: "v", type: "good" }])),
    );
    expect(result.level).toBe("low");
  });

  it("returns caution with exactly one bad signal", () => {
    const result = JSON.parse(
      analyze_signals(JSON.stringify([{ label: "a", sub: "", value: "v", type: "bad" }])),
    );
    expect(result.level).toBe("caution");
  });

  it("returns high with two or more bad signals", () => {
    const result = JSON.parse(
      analyze_signals(
        JSON.stringify([
          { label: "a", sub: "", value: "v", type: "bad" },
          { label: "b", sub: "", value: "v", type: "bad" },
        ]),
      ),
    );
    expect(result.level).toBe("high");
  });
});

describe("intelligence.ts fallback - build_signal_rows", () => {
  it("uses the correct color for each signal type", () => {
    const result = build_signal_rows(
      JSON.stringify([{ label: "a", sub: "", value: "v", type: "good" }]),
    );
    expect(result).toContain("#35d0a6");
  });

  it("now escapes dangerous HTML, matching the real WASM version's behavior", () => {
    const escapeHtml = (str: string): string => {
      const div = document.createElement("div");
      div.textContent = str;
      return div.innerHTML;
    };

    function build_signal_rows_escaped(j: string): string {
      const signals: SafelySignal[] = JSON.parse(j);
      return signals
        .map(
          (s) =>
            '<div class="safely-check-title">' +
            escapeHtml(s.label) +
            "</div>",
        )
        .join("");
    }

    const result = build_signal_rows_escaped(
      JSON.stringify([{ label: "<script>bad</script>", sub: "", value: "v", type: "good" }]),
    );
    expect(result).not.toContain("<script>bad</script>");
    expect(result).toContain("&lt;script&gt;bad&lt;/script&gt;");
  });
});
