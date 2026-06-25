(async function () {
  "use strict";

  var wasm;
  try {
    var wasmUrl = chrome.runtime.getURL("pkg/wasm.js");
    wasm = await import(wasmUrl);
    await wasm.default();
  } catch (e) {
    console.warn("Safely: WASM blocked, using JS fallback");
    wasm = {
      analyze_signals: function (j) {
        var signals = JSON.parse(j);
        var bad = signals.filter(function (s) {
          return s.type === "bad" || s.type === "caution";
        }).length;
        var level = bad === 0 ? "low" : bad === 1 ? "caution" : "high";
        var text =
          bad === 0
            ? "All " +
              signals.length +
              " signals checked. No red flags detected."
            : bad + " of " + signals.length + " signals need your attention.";
        return JSON.stringify({ level: level, text: text });
      },
      build_signal_rows: function (j) {
        var signals = JSON.parse(j);
        return signals
          .map(function (s) {
            return (
              '<div class="safely-signal-row"><span class="safely-signal-label-wrap">' +
              '<span class="safely-signal-label">' +
              s.label +
              "</span>" +
              '<span class="safely-signal-sublabel">' +
              s.sub +
              "</span>" +
              '</span><span class="safely-signal-value safely-signal-' +
              s.type +
              '">' +
              s.value +
              "</span></div>"
            );
          })
          .join("");
      },
    };
  }

  if (!window.__safelyAddTab) return;

  function buildIntelligenceTab() {
    var pageData = window.__safelyData;
    var sigResult = JSON.parse(
      wasm.analyze_signals(JSON.stringify(pageData.signals)),
    );
    var summaryLvl = sigResult.level;
    var summaryText = sigResult.text;
    return (
      '<div class="safely-intel-summary safely-alert-' +
      summaryLvl +
      '"><span>&#9679;</span><span>' +
      summaryText +
      "</span></div>" +
      '<div class="safely-section-label" style="margin-top:14px">Listing signals</div><div class="safely-signal-list">' +
      wasm.build_signal_rows(JSON.stringify(pageData.signals)) +
      "</div>" +
      (function () {
        var priceSignal = pageData.signals.find(function (s) {
          return s.label === "Price analysis";
        });
        var verdict = priceSignal ? priceSignal.value : "unknown";
        var reasoning = priceSignal
          ? priceSignal.sub
          : "No price data available.";
        var verdictClass =
          verdict === "normal"
            ? "low"
            : verdict === "unknown"
              ? "low"
              : "caution";
        return (
          '<div class="safely-section-label" style="margin-top:18px">Price vs market</div>' +
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
          "</div>"
        );
      })() +
      '<div class="safely-section-label" style="margin-top:18px">Recommended checks</div><div style="display:flex;flex-direction:column;gap:8px">' +
      '<div class="safely-check-card"><div class="safely-check-title">Ask for a live video call</div><div class="safely-check-body">Verify the item is physically in the seller\'s hands before sending any payment.</div></div>' +
      '<div class="safely-check-card"><div class="safely-check-title">Check IMEI on delivery</div><div class="safely-check-body">Dial *#06# on the device and confirm the number matches what the seller declared at deal creation.</div></div>' +
      '<div class="safely-check-card"><div class="safely-check-title">Do not pay to number in listing</div><div class="safely-check-body">A phone number in the listing could route your payment outside Safely escrow protection.</div></div></div>'
    );
  }

  window.__safelyAddTab(
    "intelligence",
    "Intelligence",
    buildIntelligenceTab(),
    '<svg viewBox="0 0 24 24" fill="none" stroke="#8e8e93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="2"/><path d="M16.24 7.76a6 6 0 010 8.48"/><path d="M19.07 4.93a10 10 0 010 14.14"/><path d="M7.76 16.24a6 6 0 010-8.48"/><path d="M4.93 19.07a10 10 0 010-14.14"/></svg>',
  );

  window.addEventListener("safely-data-ready", function () {
    var tabEl = document.getElementById("safely-tab-intelligence");
    if (tabEl) tabEl.innerHTML = buildIntelligenceTab();
  });
})();
