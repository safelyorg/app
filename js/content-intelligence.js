(async function () {
  "use strict";

  var wasmUrl = chrome.runtime.getURL("pkg/safely_extension.js");
  var wasm = await import(wasmUrl);
  await wasm.default();

  if (!window.__safelyAddTab) return;

  var pageData = window.__safelyData;

  var sigResult = JSON.parse(
    wasm.analyze_signals(JSON.stringify(pageData.signals)),
  );
  var summaryLvl = sigResult.level;
  var summaryText = sigResult.text;

  var intelTabHTML =
    '<div class="safely-intel-summary safely-alert-' +
    summaryLvl +
    '"><span>&#9679;</span><span>' +
    summaryText +
    "</span></div>" +
    '<div class="safely-section-label" style="margin-top:14px">Listing signals</div><div class="safely-signal-list">' +
    wasm.build_signal_rows(JSON.stringify(pageData.signals)) +
    "</div>" +
    '<div class="safely-section-label" style="margin-top:18px">Price vs market</div>' +
    '<div class="safely-price-compare">' +
    '<div class="safely-compare-row"><span class="safely-compare-label">This listing</span><div class="safely-compare-track"><div class="safely-compare-fill safely-fill-primary" style="width:72%"></div></div><span class="safely-compare-val">85K</span></div>' +
    '<div class="safely-compare-row"><span class="safely-compare-label">Market avg</span><div class="safely-compare-track"><div class="safely-compare-fill safely-fill-neutral" style="width:75%"></div></div><span class="safely-compare-val">88K</span></div>' +
    '<div class="safely-compare-row"><span class="safely-compare-label">Lowest found</span><div class="safely-compare-track"><div class="safely-compare-fill safely-fill-low" style="width:60%"></div></div><span class="safely-compare-val">71K</span></div>' +
    '<div class="safely-compare-row"><span class="safely-compare-label">Suspicion zone</span><div class="safely-compare-track"><div class="safely-compare-fill safely-fill-danger" style="width:40%"></div></div><span class="safely-compare-val">&lt;52K</span></div></div>' +
    '<div class="safely-section-label" style="margin-top:18px">Recommended checks</div><div style="display:flex;flex-direction:column;gap:8px">' +
    '<div class="safely-check-card"><div class="safely-check-title">Ask for a live video call</div><div class="safely-check-body">Verify the item is physically in the seller\'s hands before sending any payment.</div></div>' +
    '<div class="safely-check-card"><div class="safely-check-title">Check IMEI on delivery</div><div class="safely-check-body">Dial *#06# on the device and confirm the number matches what the seller declared at deal creation.</div></div>' +
    '<div class="safely-check-card"><div class="safely-check-title">Do not pay to number in listing</div><div class="safely-check-body">A phone number in the listing could route your payment outside Safely escrow protection.</div></div></div>';

  window.__safelyAddTab(
    "intelligence",
    "Intelligence",
    intelTabHTML,
    '<svg viewBox="0 0 24 24" fill="none" stroke="#8e8e93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="2"/><path d="M16.24 7.76a6 6 0 010 8.48"/><path d="M19.07 4.93a10 10 0 010 14.14"/><path d="M7.76 16.24a6 6 0 010-8.48"/><path d="M4.93 19.07a10 10 0 010-14.14"/></svg>',
  );
})();
