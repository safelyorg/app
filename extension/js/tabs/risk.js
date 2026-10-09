"use strict";
(async function () {
    "use strict";
    // Translates through core/i18n.ts; plain English if it isn't loaded.
    function tr(en, vars) {
        const i18n = window.__safelyI18n;
        if (i18n)
            return i18n.t(en, vars);
        if (!vars)
            return en;
        return en.replace(/\{(\w+)\}/g, (whole, key) => Object.prototype.hasOwnProperty.call(vars, key) ? String(vars[key]) : whole);
    }
    function isPortuguese() {
        const i18n = window.__safelyI18n;
        return !!i18n && i18n.getLang() === "pt-br";
    }
    let wasm;
    try {
        const wasmUrl = chrome.runtime.getURL("pkg/wasm.js");
        wasm = await import(wasmUrl);
        await wasm.default();
    }
    catch (e) {
        console.warn("Safely: WASM blocked, using JS fallback");
        console.error("Safely: real WASM loading error was:", e);
        wasm = {
            default: async () => { },
            risk_level: (s) => (s <= 33 ? "low" : s <= 66 ? "caution" : "high"),
            risk_label: (l) => l === "low" ? "Low risk" : l === "caution" ? "Caution" : "High risk",
            risk_desc: (l) => l === "low"
                ? "Safe to proceed"
                : l === "caution"
                    ? "Review before proceeding"
                    : "High risk detected",
            build_activity_bars: (a) => {
                const values = Array.from(a);
                const rawMax = values.length > 0 ? Math.max(...values) : 1;
                const max = rawMax === 0 ? 1 : rawMax;
                return values
                    .map((v) => {
                    const pct = Math.round((v / max) * 100);
                    const heightPx = Math.max(2, Math.round((pct / 100) * 44));
                    const opacity = v === 0 ? 0.15 : 0.3 + (pct / 100) * 0.7;
                    return ('<div style="flex:1;display:flex;flex-direction:column;justify-content:flex-end;align-items:center;height:100%;position:relative;">' +
                        '<div class="safely-activity-bar" style="height:' +
                        heightPx +
                        "px;opacity:" +
                        opacity.toFixed(2) +
                        ';width:100%;"></div>' +
                        '<span style="position:absolute;top:6px;font-size:9px;color:#ffffff;line-height:1;">' +
                        v +
                        "</span></div>");
                })
                    .join("");
            },
            verification_badge: (s) => '<span class="safely-verified-badge">' + escapeHtml(s) + "</span>",
        };
    }
    if (!window.__safelyAddTab)
        return;
    let currentRiskSubTab = "seller";
    // Same palette as the dashboard's RISK_HEX map.
    const RISK_HEX = {
        low: "#35d0a6",
        caution: "#f2b84c",
        high: "#ff5d5d",
    };
    // English words for each risk level, used as keys for the Portuguese
    // text (the WASM module only speaks English).
    const LEVEL_LABEL = {
        low: "Low risk",
        caution: "Caution",
        high: "High risk",
    };
    const LEVEL_DESC = {
        low: "Safe to proceed",
        caution: "Review before proceeding",
        high: "High risk detected",
    };
    function riskLabelFor(level) {
        return isPortuguese() ? tr(LEVEL_LABEL[level] || LEVEL_LABEL.high) : wasm.risk_label(level);
    }
    function riskDescFor(level) {
        return isPortuguese() ? tr(LEVEL_DESC[level] || LEVEL_DESC.high) : wasm.risk_desc(level);
    }
    // The status badge comes from WASM in English; in Portuguese only its
    // visible word is swapped, so its colour and style stay the same.
    function verificationBadgeFor(status) {
        const html = wasm.verification_badge(status);
        if (!isPortuguese() || !status)
            return html;
        const word = status.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
        return html.replace(new RegExp(">\\s*" + word + "\\s*<", "i"), ">" + escapeHtml(tr(status)) + "<");
    }
    // The line under the activity chart. In Portuguese it is written
    // here from the report count; in English the server's sentence is
    // shown as before.
    function networkSummaryFor(pageData) {
        if (!isPortuguese())
            return pageData.seller.networkSummary;
        const count = pageData.fraudReportCount || 0;
        if (count === 0)
            return tr("No fraud reports found on the Safely network.");
        if (count === 1)
            return tr("1 fraud report found on the Safely network. Proceed with caution.");
        return tr("{n} fraud reports found on the Safely network. High-risk seller.", { n: count });
    }
    // A seller detail ("Not found", "About 11 years", a name...) in the
    // current language, made safe to put in the page.
    function detail(value) {
        return escapeHtml(tr(value === null || value === undefined ? "" : String(value)));
    }
    function buildRiskGauge(score, level) {
        const color = RISK_HEX[level] || RISK_HEX.high;
        const r = 44;
        const circumference = 2 * Math.PI * r;
        const offset = circumference * (1 - score / 100);
        let ticks = "";
        const tickCount = 40;
        for (let i = 0; i < tickCount; i++) {
            const angle = (i * 360) / tickCount;
            const major = i % 5 === 0;
            const len = major ? 6 : 3;
            const outerR = 54;
            const innerR = outerR - len;
            ticks +=
                '<line x1="60" y1="' +
                    (60 - outerR) +
                    '" x2="60" y2="' +
                    (60 - innerR) +
                    '" stroke="' +
                    (major ? "#3a3a42" : "#24242b") +
                    '" stroke-width="1.5" transform="rotate(' +
                    angle +
                    ' 60 60)" />';
        }
        return ('<svg viewBox="0 0 120 120" style="width:100%;height:100%">' +
            ticks +
            '<circle cx="60" cy="60" r="' +
            r +
            '" fill="none" stroke="#1b1b20" stroke-width="9" />' +
            '<circle cx="60" cy="60" r="' +
            r +
            '" fill="none" stroke="' +
            color +
            '" stroke-width="9" stroke-linecap="round" stroke-dasharray="' +
            circumference +
            '" stroke-dashoffset="' +
            offset +
            '" transform="rotate(-90 60 60)" />' +
            '<text x="60" y="57" text-anchor="middle" font-family="JetBrains Mono, monospace" font-weight="700" font-size="30" fill="' +
            color +
            '">' +
            score +
            "</text>" +
            '<text x="60" y="75" text-anchor="middle" font-family="Inter, sans-serif" font-weight="600" font-size="9" fill="#8a8a93" letter-spacing="0.5">/ 100</text>' +
            "</svg>");
    }
    function buildSellerSection() {
        const pageData = window.__safelyData;
        const score = pageData.riskScore || 0;
        const lvl = wasm.risk_level(score);
        const riskLabel = riskLabelFor(lvl);
        const riskDesc = riskDescFor(lvl);
        const riskColor = RISK_HEX[lvl] || RISK_HEX.high;
        const activityBars = wasm.build_activity_bars(new Uint8Array(pageData.seller.monthlyActivity.map((v) => Math.min(255, Math.max(0, v)))));
        const circleHTML = '<div style="text-align:center;padding:20px 16px 10px">' +
            '<div style="width:120px;height:120px;margin:0 auto 12px">' +
            buildRiskGauge(score, lvl) +
            "</div>" +
            '<div style="font-size:18px;font-weight:700;color:' +
            riskColor +
            '">' +
            riskLabel +
            "</div>" +
            '<div style="font-size:13px;color:#8a8a93;margin-top:4px">' +
            riskDesc +
            "</div>" +
            "</div>";
        const sellerCardHTML = '<div class="safely-section-label">' +
            tr("Seller Information") +
            '</div><div class="safely-seller-card"><div class="safely-seller-name">' +
            detail(pageData.seller.name) +
            '</div><div class="safely-seller-detail"><span>' +
            tr("Username") +
            "</span><span>" +
            detail(pageData.seller.handle) +
            '</span></div><div class="safely-seller-detail"><span>' +
            tr("Phone") +
            "</span><span>" +
            detail(pageData.seller.phone) +
            '</span></div><div class="safely-seller-detail"><span>' +
            tr("Account age") +
            "</span><span>" +
            detail(pageData.seller.accountAge) +
            '</span></div><div class="safely-seller-detail"><span>' +
            tr("Location") +
            "</span><span>" +
            detail(pageData.seller.location) +
            '</span></div><div class="safely-seller-detail"><span>' +
            tr("Last active") +
            "</span><span>" +
            detail(pageData.seller.lastActive) +
            '</span></div><div class="safely-seller-detail"><span>' +
            tr("Status") +
            "</span>" +
            verificationBadgeFor(pageData.seller.verification) +
            '</div><div class="safely-seller-detail"><span>' +
            tr("Fraud Reports") +
            '</span><span style="color:' +
            (pageData.fraudReportCount > 0 ? "#ff5d5d" : "#8a8a93") +
            '">' +
            (pageData.fraudReportCount || 0) +
            '</span></div><div class="safely-seller-detail"><span>' +
            tr("Platform") +
            '</span><span style="text-transform:capitalize">' +
            detail(pageData.seller.platform) +
            "</span></div></div>";
        const activityHTML = '<div class="safely-section-label" style="margin-top:18px">' +
            tr("Visit activity — 12 months") +
            "</div>" +
            '<div class="safely-activity-card">' +
            '<div style="display:flex;align-items:flex-end;gap:3px;height:56px">' +
            activityBars +
            "</div>" +
            '<div style="display:flex;gap:3px;margin-top:4px">' +
            (function () {
                const months = [
                    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
                    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
                ];
                return months
                    .map((m) => '<span style="flex:1;text-align:center;font-size:8px;color:#8a8a93;">' +
                    tr(m) +
                    "</span>")
                    .join("");
            })() +
            "</div></div>";
        const networkHTML = '<div class="safely-network-alert safely-alert-' +
            lvl +
            '" style="margin-top:14px"><span>&#9679;</span><span>' +
            networkSummaryFor(pageData) +
            "</span></div>";
        const outcomeButtonsHTML = '<div style="display:flex;gap:8px;margin-top:14px;">' +
            '<button id="safely-outcome-proceed" style="flex:1;padding:10px;border-radius:8px;border:1px solid #35d0a6;background:transparent;color:#35d0a6;font-size:12px;font-weight:600;cursor:pointer;">' +
            escapeHtml(tr("I'm proceeding")) +
            "</button>" +
            '<button id="safely-outcome-abort" style="flex:1;padding:10px;border-radius:8px;border:1px solid #ff5d5d;background:transparent;color:#ff5d5d;font-size:12px;font-weight:600;cursor:pointer;">' +
            escapeHtml(tr("I'm backing out")) +
            "</button>" +
            "</div>" +
            '<div id="safely-outcome-confirmed" style="display:none;text-align:center;margin-top:10px;font-size:12px;color:#8a8a93;">' +
            tr("Thanks - your response has been recorded.") +
            "</div>";
        return circleHTML + sellerCardHTML + activityHTML + networkHTML + outcomeButtonsHTML;
    }
    // The report reasons: [value sent to the server, name, description].
    const REPORT_REASONS = [
        ["scam", "Scam", "Seller took payment and disappeared"],
        ["fake_item", "Fake item", "Item was counterfeit or misrepresented"],
        ["no_delivery", "No delivery", "Payment sent but item never arrived"],
        ["wrong_item", "Wrong item", "Received something different"],
        ["non_responsive", "Non responsive", "Seller stopped responding after payment"],
    ];
    function buildReportSection() {
        const reasons = REPORT_REASONS.map(([value, name, desc]) => '<label class="safely-report-reason"><input type="radio" name="safely-report-reason" value="' +
            value +
            '"><div class="safely-report-reason-text"><span class="safely-report-reason-name">' +
            tr(name) +
            '</span><span class="safely-reason-desc">' +
            tr(desc) +
            "</span></div></label>").join("");
        return ('<div class="safely-report-section">' +
            '<div class="safely-section-label">' +
            tr("Report this seller") +
            "</div>" +
            '<p class="safely-report-desc">' +
            tr("If you experienced fraud or suspicious behavior from this seller, help protect others by submitting a report.") +
            "</p>" +
            '<div class="safely-section-label" style="margin-top:14px">' +
            tr("Select reason") +
            "</div>" +
            '<div class="safely-report-reasons" id="safely-report-reasons">' +
            reasons +
            "</div>" +
            '<button class="safely-report-btn" id="safely-report-submit">' +
            tr("Submit Report") +
            "</button>" +
            '<div class="safely-report-success" id="safely-report-success" style="display:none">' +
            "<span>&#10003;</span> " +
            tr("Report submitted. Thank you for helping protect the community.") +
            "</div>" +
            "</div>");
    }
    function buildRiskTab() {
        const sellerVisible = currentRiskSubTab === "seller";
        return ('<div class="safely-sub-tabs">' +
            '<button class="safely-sub-tab' +
            (sellerVisible ? " safely-active" : "") +
            '" id="safely-risk-subtab-seller">' +
            tr("Risk") +
            "</button>" +
            '<button class="safely-sub-tab' +
            (!sellerVisible ? " safely-active" : "") +
            '" id="safely-risk-subtab-report">' +
            tr("Report") +
            "</button>" +
            "</div>" +
            '<div id="safely-risk-seller-content"' +
            (sellerVisible ? "" : ' style="display:none"') +
            ">" +
            buildSellerSection() +
            "</div>" +
            '<div id="safely-risk-report-content"' +
            (!sellerVisible ? "" : ' style="display:none"') +
            ">" +
            buildReportSection() +
            "</div>");
    }
    function fraudCountContribution(count) {
        if (count === 0)
            return 0;
        if (count === 1)
            return 20;
        if (count === 2)
            return 35;
        return 50;
    }
    function attachRiskTabListeners() {
        const root = document.getElementById("safely-tab-risk");
        if (!root)
            return;
        const sellerBtn = root.querySelector("#safely-risk-subtab-seller");
        const reportBtn = root.querySelector("#safely-risk-subtab-report");
        const sellerContent = root.querySelector("#safely-risk-seller-content");
        const reportContent = root.querySelector("#safely-risk-report-content");
        if (sellerBtn && reportBtn && sellerContent && reportContent) {
            sellerBtn.addEventListener("click", () => {
                currentRiskSubTab = "seller";
                sellerBtn.classList.add("safely-active");
                reportBtn.classList.remove("safely-active");
                sellerContent.style.display = "";
                reportContent.style.display = "none";
            });
            reportBtn.addEventListener("click", () => {
                currentRiskSubTab = "report";
                reportBtn.classList.add("safely-active");
                sellerBtn.classList.remove("safely-active");
                reportContent.style.display = "";
                sellerContent.style.display = "none";
            });
        }
        const submitBtn = root.querySelector("#safely-report-submit");
        if (submitBtn) {
            submitBtn.addEventListener("click", async () => {
                const selected = root.querySelector('input[name="safely-report-reason"]:checked');
                if (!selected) {
                    alert(tr("Please select a reason before submitting."));
                    return;
                }
                const pageData = window.__safelyData;
                submitBtn.textContent = tr("Submitting...");
                submitBtn.disabled = true;
                const reportData = {
                    platform: pageData.seller.platform || "olx",
                    platform_id: pageData.seller.platformId || null,
                    report_type: selected.value,
                    description: null,
                    listing_url: window.location.href,
                };
                const result = await window.__safelyAPI.submitReport(reportData);
                if (!result || result.error) {
                    submitBtn.textContent = tr("Submit Report");
                    submitBtn.disabled = false;
                    if (result && result.error === "unauthorized") {
                        alert(tr("Please sign in again to submit a report."));
                    }
                    else {
                        alert(tr("Failed to submit report. Please try again."));
                    }
                    return;
                }
                const success = root.querySelector("#safely-report-success");
                if (success)
                    success.style.display = "flex";
                submitBtn.style.display = "none";
                // Reflect the report immediately without another /analyze
                // call - re-fetching here would quietly double-count this
                // visit's monthly activity. Updating the already-loaded data
                // in place and redrawing only the seller section avoids that.
                //
                // Mirrors the exact fraud-count contribution used by the
                // backend's calculate_risk_score - a step function, not a
                // flat +N per report.
                const oldCount = window.__safelyData.fraudReportCount || 0;
                const newCount = oldCount + 1;
                const delta = fraudCountContribution(newCount) - fraudCountContribution(oldCount);
                window.__safelyData.fraudReportCount = newCount;
                window.__safelyData.seller.verification = "reported";
                window.__safelyData.riskScore = Math.min(100, (window.__safelyData.riskScore || 0) + delta);
                // The network-alert sentence is plain text from the last
                // analyze call - swap in the new count wherever a standalone
                // number appears in it. (In Portuguese the sentence is written
                // from the count itself, so it is always right.)
                if (window.__safelyData.seller.networkSummary) {
                    window.__safelyData.seller.networkSummary = window.__safelyData.seller.networkSummary.replace(/\d+/, String(newCount));
                }
                const sellerContentEl = document.getElementById("safely-risk-seller-content");
                if (sellerContentEl)
                    sellerContentEl.innerHTML = buildSellerSection();
            });
        }
        const proceedBtn = root.querySelector("#safely-outcome-proceed");
        const abortBtn = root.querySelector("#safely-outcome-abort");
        const outcomeConfirmed = root.querySelector("#safely-outcome-confirmed");
        async function handleOutcomeClick(action) {
            const pageData = window.__safelyData;
            if (!pageData.analysisId)
                return;
            if (proceedBtn)
                proceedBtn.disabled = true;
            if (abortBtn)
                abortBtn.disabled = true;
            const success = await window.__safelyAPI.submitOutcome(pageData.analysisId, action);
            if (success) {
                if (proceedBtn)
                    proceedBtn.style.display = "none";
                if (abortBtn)
                    abortBtn.style.display = "none";
                if (outcomeConfirmed)
                    outcomeConfirmed.style.display = "block";
            }
            else {
                if (proceedBtn)
                    proceedBtn.disabled = false;
                if (abortBtn)
                    abortBtn.disabled = false;
            }
        }
        if (proceedBtn) {
            proceedBtn.addEventListener("click", () => handleOutcomeClick("proceeded"));
        }
        if (abortBtn) {
            abortBtn.addEventListener("click", () => handleOutcomeClick("aborted"));
        }
    }
    function redrawRiskTab() {
        const tabEl = document.getElementById("safely-tab-risk");
        if (tabEl) {
            tabEl.innerHTML = buildRiskTab();
            attachRiskTabListeners();
        }
    }
    window.__safelyAddTab("risk", "Risk", buildRiskTab(), '<svg viewBox="0 0 24 24" fill="none" stroke="#8a8a93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><polyline points="9 12 11 14 15 10"/></svg>', () => {
        if (window.__safelyPreventInputBubbling) {
            window.__safelyPreventInputBubbling();
        }
        attachRiskTabListeners();
    });
    window.addEventListener("safely-data-ready", redrawRiskTab);
    window.addEventListener("safely-lang-changed", redrawRiskTab);
    window.addEventListener("safely-result-text-changed", redrawRiskTab);
})();
