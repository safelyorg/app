"use strict";
(async function () {
    "use strict";
    let wasm;
    try {
        const wasmUrl = chrome.runtime.getURL("pkg/wasm.js");
        wasm = await import(wasmUrl);
        await wasm.default();
        console.log("Safely: real WASM module loaded successfully");
    }
    catch (e) {
        console.warn("Safely: WASM blocked, using JS fallback");
        console.error("Safely: real WASM loading error was:", e);
        wasm = {
            default: async () => { },
            analyze_signals: (j) => {
                const signals = JSON.parse(j);
                const bad = signals.filter((s) => s.type === "bad" || s.type === "caution").length;
                const level = bad === 0 ? "low" : bad === 1 ? "caution" : "high";
                const text = bad === 0
                    ? "All " + signals.length + " signals checked. No red flags detected."
                    : bad + " of " + signals.length + " signals need your attention.";
                return JSON.stringify({ level, text });
            },
            // Redesigned to match the Recommended Checks card style - separate
            // rounded cards with spacing between them, instead of a single
            // bordered list with colored differentiator lines. The status
            // word itself is now the only color differentiation.
            build_signal_rows: (j) => {
                const signals = JSON.parse(j);
                const COLORS = {
                    good: "#35d0a6",
                    caution: "#f2b84c",
                    info: "#8e8e93",
                };
                return signals
                    .map((s) => {
                    const color = COLORS[s.type] || "#ff5d5d";
                    return ('<div class="safely-check-card">' +
                        '<div style="display:flex;justify-content:space-between;align-items:baseline;gap:8px;">' +
                        '<div class="safely-check-title">' +
                        window.escapeHtml(capitalizeFirst(s.label)) +
                        "</div>" +
                        '<div style="font-weight:700;white-space:nowrap;font-size:13px;color:' +
                        color +
                        ';">' +
                        window.escapeHtml(capitalizeFirst(s.value)) +
                        "</div></div>" +
                        '<div class="safely-check-body">' +
                        window.escapeHtml(capitalizeFirst(s.sub) || "") +
                        "</div></div>");
                })
                    .join("");
            },
        };
    }
    if (!window.__safelyAddTab)
        return;
    function buildIntelligenceTab() {
        const pageData = window.__safelyData;
        const sigResult = JSON.parse(wasm.analyze_signals(JSON.stringify(pageData.signals)));
        const summaryLvl = sigResult.level;
        const summaryText = sigResult.text;
        return ('<div class="safely-intel-summary safely-alert-' +
            summaryLvl +
            '"><span>&#9679;</span><span>' +
            summaryText +
            "</span></div>" +
            '<div class="safely-section-label" style="margin-top:14px">Listing signals</div><div style="display:flex;flex-direction:column;gap:8px">' +
            wasm.build_signal_rows(JSON.stringify(pageData.signals)) +
            "</div>" +
            (function () {
                const priceSignal = pageData.signals.find((s) => s.label === "Price analysis");
                const verdict = priceSignal ? priceSignal.value : "unknown";
                const reasoning = priceSignal ? priceSignal.sub : "No price data available.";
                const verdictClass = verdict === "normal" ? "low" : verdict === "unknown" ? "low" : "caution";
                return ('<div class="safely-section-label" style="margin-top:18px">Price vs market</div>' +
                    '<div class="safely-network-alert safely-alert-' +
                    verdictClass +
                    '" style="margin-top:8px">' +
                    "<span>&#9679;</span>" +
                    "<div>" +
                    '<div style="font-weight:600;margin-bottom:4px">' +
                    verdict.charAt(0).toUpperCase() +
                    verdict.slice(1) +
                    "</div>" +
                    '<div style="font-size:12px;opacity:0.85">' +
                    reasoning +
                    "</div>" +
                    "</div>" +
                    "</div>");
            })() +
            '<div class="safely-section-label" style="margin-top:18px">Recommended checks</div><div style="display:flex;flex-direction:column;gap:8px">' +
            '<div class="safely-check-card"><div class="safely-check-title">Ask for a live video call</div><div class="safely-check-body">Verify the item is physically in the seller\'s hands before sending any payment.</div></div>' +
            '<div class="safely-check-card"><div class="safely-check-title">Check IMEI on delivery</div><div class="safely-check-body">Dial *#06# on the device and confirm the number matches what the seller declared at deal creation.</div></div>' +
            '<div class="safely-check-card"><div class="safely-check-title">Do not pay to number in listing</div><div class="safely-check-body">A phone number in the listing could route your payment outside Safely escrow protection.</div></div></div>' +
            buildRiskFactorsSection(pageData.riskFactors));
    }
    const SEVERITY_COLORS = {
        hard: "#ff5d5d",
        compound: "#f2b84c",
        soft: "#8e8e93",
    };
    const SEVERITY_LABELS = {
        hard: "Confirmed",
        compound: "Pattern match",
        soft: "Worth noting",
    };
    function capitalizeFirst(str) {
        if (!str)
            return str;
        return str.charAt(0).toUpperCase() + str.slice(1);
    }
    function buildRiskFactorsSection(riskFactors) {
        if (!riskFactors || riskFactors.length === 0)
            return "";
        const rows = riskFactors
            .map((factor) => {
            const color = SEVERITY_COLORS[factor.severity] || "#8e8e93";
            const severityLabel = SEVERITY_LABELS[factor.severity] || factor.severity;
            const shortTitle = factor.contributing_signals && factor.contributing_signals.length > 0
                ? factor.contributing_signals.join(" + ")
                : capitalizeFirst(factor.name.replace(/_/g, " "));
            return ('<div class="safely-check-card">' +
                '<div style="display:flex;justify-content:space-between;align-items:baseline;gap:8px;">' +
                '<div class="safely-check-title">' +
                window.escapeHtml(shortTitle) +
                "</div>" +
                '<div style="font-weight:700;white-space:nowrap;font-size:11px;color:' +
                color +
                ';">' +
                window.escapeHtml(severityLabel) +
                "</div></div>" +
                '<div class="safely-check-body">' +
                window.escapeHtml(factor.description) +
                "</div></div>");
        })
            .join("");
        return ('<div class="safely-section-label" style="margin-top:18px">Risk Factors</div><div style="display:flex;flex-direction:column;gap:8px">' +
            rows +
            "</div>");
    }
    window.__safelyAddTab("intelligence", "Intelligence", buildIntelligenceTab(), '<svg viewBox="0 0 24 24" fill="none" stroke="#8e8e93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="2"/><path d="M16.24 7.76a6 6 0 010 8.48"/><path d="M19.07 4.93a10 10 0 010 14.14"/><path d="M7.76 16.24a6 6 0 010-8.48"/><path d="M4.93 19.07a10 10 0 010-14.14"/></svg>', null);
    window.addEventListener("safely-data-ready", () => {
        const tabEl = document.getElementById("safely-tab-intelligence");
        if (tabEl)
            tabEl.innerHTML = buildIntelligenceTab();
    });
})();
