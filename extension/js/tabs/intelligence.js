"use strict";
(async function () {
    "use strict";
    // Translates through core/i18n.ts; plain English if it isn't loaded.
    // Only what is SHOWN is translated - the signal labels and values
    // the code below checks ("Advance payment request", "Full
    // prepayment"...) always stay in English in the data itself.
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
                    info: "#6fb3ef",
                };
                return signals
                    .map((s, idx) => {
                    const color = COLORS[s.type] || "#ff5d5d";
                    const { realSub, checklist } = parseChecklistSignal(s.sub);
                    const dropdownId = "safely-checklist-" + idx;
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
                        window.escapeHtml(capitalizeFirst(realSub) || "") +
                        "</div>" +
                        buildChecklistDropdown(dropdownId, checklist) +
                        "</div>");
                })
                    .join("");
            },
        };
    }
    if (!window.__safelyAddTab)
        return;
    const PLATFORM_ORDER = [
        "Facebook", "LinkedIn", "TikTok", "Instagram", "Reddit", "Trustpilot",
        "Reviews", "Contact (Facebook)", "Contact (LinkedIn)", "Contact (Web)",
    ];
    function buildSocialPresenceSection() {
        const pageData = window.__safelyData;
        const results = pageData.socialCandidates || [];
        if (results.length === 0)
            return "";
        // Merge every variant's results into one, deduplicated list per
        // real platform - the same platform can be checked with several
        // real name variants, and the same, genuine link often shows up
        // more than once across them.
        const grouped = {};
        results.forEach((entry) => {
            if (!(entry.platform in grouped))
                grouped[entry.platform] = [];
            entry.candidates.forEach((c) => {
                if (!grouped[entry.platform].some((existing) => existing.url === c.url)) {
                    grouped[entry.platform].push(c);
                }
            });
        });
        const order = PLATFORM_ORDER.filter((p) => p in grouped).concat(Object.keys(grouped).filter((p) => !PLATFORM_ORDER.includes(p)));
        const groupsHTML = order
            .map((platform, index) => {
            const isLast = index === order.length - 1;
            const borderStyle = isLast ? "" : "border-bottom:1px solid rgba(255,255,255,0.08);";
            const links = grouped[platform];
            const body = links.length === 0
                ? '<div style="padding:4px;font-size:12px;color:#8e8e93;">' +
                    tr("Not found") +
                    "</div>"
                : links
                    .map((link) => '<div style="display:flex;align-items:flex-start;gap:8px;padding:6px 4px;">' +
                    '<span style="color:#8e8e93;flex-shrink:0;margin-top:1px;">&#8226;</span>' +
                    '<a href="' +
                    window.escapeHtml(link.url) +
                    '" target="_blank" rel="noopener noreferrer" style="font-size:12px;line-height:1.4;color:#f2f1ed;flex:1;min-width:0;text-decoration:none;" onmouseover="this.style.color=\'#6fb3ef\'" onmouseout="this.style.color=\'#f2f1ed\'">' +
                    window.escapeHtml(link.title) +
                    "</a></div>")
                    .join("");
            return ('<div style="padding:10px 0;' +
                borderStyle +
                '">' +
                '<div style="font-size:11px;font-weight:700;color:#8e8e93;text-transform:uppercase;letter-spacing:0.4px;margin-bottom:4px;padding:0 4px;">' +
                window.escapeHtml(tr(platform)) +
                "</div>" +
                body +
                "</div>");
        })
            .join("");
        return ('<div class="safely-section-label" style="margin-top:18px">' +
            tr("Social presence check") +
            "</div>" +
            '<div class="safely-check-card">' +
            '<button id="safely-social-toggle" type="button" style="width:100%;text-align:left;background:none;border:none;cursor:pointer;padding:0;font-size:13px;font-weight:600;color:#f2f1ed;display:flex;justify-content:space-between;align-items:center;">' +
            '<span id="safely-social-toggle-text">' +
            tr("Click to drop down") +
            '</span><span id="safely-social-arrow">&#9662;</span>' +
            "</button>" +
            '<div id="safely-social-dropdown" style="display:none;margin-top:12px;">' +
            groupsHTML +
            "</div></div>");
    }
    function parseChecklistSignal(sub) {
        const marker = "###CHECKLIST###";
        const idx = sub.indexOf(marker);
        if (idx === -1)
            return { realSub: sub, checklist: [] };
        const realSub = sub.slice(0, idx);
        const checklistRaw = sub.slice(idx + marker.length);
        const checklist = checklistRaw
            .split(";")
            .filter(Boolean)
            .map((entry) => {
            const [name, present] = entry.split("|");
            return [name, present === "true"];
        });
        return { realSub, checklist };
    }
    function buildChecklistDropdown(id, checklist) {
        if (checklist.length === 0)
            return "";
        const rows = checklist
            .map(([name, present]) => {
            const icon = present
                ? '<span style="color:#35d0a6;flex-shrink:0;">&#10003;</span>'
                : '<span style="color:#8e8e93;flex-shrink:0;">&#10005;</span>';
            return ('<div style="display:flex;align-items:center;gap:8px;padding:6px 4px;">' +
                icon +
                '<span style="font-size:12px;color:#f2f1ed;">' +
                window.escapeHtml(tr(name)) +
                "</span></div>");
        })
            .join("");
        return ('<button id="' +
            id +
            '-toggle" type="button" style="width:100%;text-align:left;background:none;border:none;cursor:pointer;padding:8px 0 0 0;font-size:12px;font-weight:600;color:#8e8e93;display:flex;justify-content:space-between;align-items:center;">' +
            '<span id="' +
            id +
            '-toggle-text">' +
            tr("Click to see checks") +
            '</span><span id="' +
            id +
            '-arrow">&#9662;</span>' +
            "</button>" +
            '<div id="' +
            id +
            '-dropdown" style="display:none;margin-top:6px;">' +
            rows +
            "</div>");
    }
    function attachChecklistListener(id) {
        const toggle = document.getElementById(id + "-toggle");
        const dropdown = document.getElementById(id + "-dropdown");
        const arrow = document.getElementById(id + "-arrow");
        const toggleText = document.getElementById(id + "-toggle-text");
        if (toggle && dropdown && arrow && toggleText) {
            toggle.addEventListener("click", () => {
                const isOpen = dropdown.style.display !== "none";
                dropdown.style.display = isOpen ? "none" : "block";
                arrow.innerHTML = isOpen ? "&#9662;" : "&#9652;";
                toggleText.textContent = isOpen ? tr("Click to see checks") : tr("Click to hide checks");
            });
        }
    }
    // Worst first: on a tie, the more serious kind is the dominant one.
    const MIX_ORDER = ["bad", "caution", "info", "good"];
    const MIX_COLORS = {
        good: "#35d0a6",
        info: "#6fb3ef",
        caution: "#f2b84c",
        bad: "#ff5d5d",
    };
    const MIX_NAMES = {
        good: "Good",
        info: "Info",
        caution: "Warning",
        bad: "Red flag",
    };
    function mixType(type) {
        return type === "good" || type === "info" || type === "caution" ? type : "bad";
    }
    // Whole percentages that always add up to 100 (largest remainder).
    function mixParts(signals) {
        const total = signals.length;
        if (total === 0)
            return [];
        const counts = { good: 0, info: 0, caution: 0, bad: 0 };
        signals.forEach((s) => (counts[mixType(s.type)] += 1));
        const parts = ["good", "info", "caution", "bad"]
            .filter((t) => counts[t] > 0)
            .map((t) => {
            const exact = (counts[t] * 100) / total;
            return { type: t, count: counts[t], pct: Math.floor(exact), rest: exact % 1 };
        });
        let left = 100 - parts.reduce((sum, p) => sum + p.pct, 0);
        parts
            .slice()
            .sort((a, b) => b.rest - a.rest)
            .forEach((p) => {
            if (left > 0) {
                p.pct += 1;
                left -= 1;
            }
        });
        return parts.map(({ type, count, pct }) => ({ type, count, pct }));
    }
    function dominantPart(parts) {
        return parts.reduce((best, p) => p.count > best.count ||
            (p.count === best.count && MIX_ORDER.indexOf(p.type) < MIX_ORDER.indexOf(best.type))
            ? p
            : best);
    }
    const MIX_STYLE = "<style>" +
        ".smx{display:flex;align-items:center;gap:16px;padding:14px;margin-top:12px;border-radius:14px;background:rgba(255,255,255,0.03);border:1px solid rgba(255,255,255,0.07);}" +
        ".smx-ring{width:124px;height:124px;flex-shrink:0;cursor:pointer;-webkit-tap-highlight-color:transparent;}" +
        ".smx-ring svg{width:100%;height:100%;overflow:visible;}" +
        ".smx-seg{fill:none;stroke-width:12;stroke:var(--dom);stroke-dasharray:var(--whole);transition:stroke .45s ease,stroke-dasharray .45s ease,stroke-width .2s ease,opacity .2s ease;}" +
        ".smx-open .smx-seg{stroke:var(--c);stroke-dasharray:var(--split);}" +
        ".smx-open .smx-seg.smx-dim{opacity:.35;}" +
        ".smx-open .smx-seg.smx-hot{stroke-width:16;}" +
        ".smx-glow{fill:none;stroke:var(--dom);stroke-width:22;opacity:.10;transition:opacity .45s ease,stroke .45s ease;}" +
        ".smx-open .smx-glow{opacity:0;}" +
        ".smx-pct{font-family:'JetBrains Mono',monospace;font-weight:700;font-size:24px;transition:fill .3s ease;}" +
        ".smx-name{font-family:Inter,sans-serif;font-weight:700;font-size:9px;letter-spacing:.6px;text-transform:uppercase;transition:fill .3s ease;}" +
        ".smx-count{font-family:Inter,sans-serif;font-size:8px;fill:#8a8a93;}" +
        ".smx-side{flex:1;min-width:0;}" +
        ".smx-title{font-size:11px;font-weight:700;color:#8a8a93;text-transform:uppercase;letter-spacing:.5px;}" +
        ".smx-lead{font-size:14px;font-weight:700;margin:3px 0 8px;}" +
        ".smx-row{display:flex;align-items:center;gap:8px;padding:4px 6px;margin:0 -6px;border-radius:8px;font-size:12px;color:#c9c8c3;cursor:default;transition:background .2s ease,opacity .2s ease;}" +
        ".smx-row.smx-hot{background:rgba(255,255,255,0.06);}" +
        ".smx-row.smx-dim{opacity:.45;}" +
        ".smx-dot{width:8px;height:8px;border-radius:50%;flex-shrink:0;}" +
        ".smx-row b{margin-left:auto;font-family:'JetBrains Mono',monospace;font-size:11px;}" +
        ".smx-hint{font-size:10.5px;color:#6f6f78;margin-top:8px;line-height:1.4;}" +
        "[data-smx-type]{transition:opacity .2s ease;}" +
        "</style>";
    function buildSignalMix(signals) {
        const parts = mixParts(signals);
        if (parts.length === 0)
            return "";
        const dom = dominantPart(parts);
        const r = 46;
        const c = 2 * Math.PI * r;
        // Gap between parts when split; none when there is only one part.
        const gap = parts.length > 1 ? 3 : 0;
        let start = 0;
        const segs = parts
            .map((p) => {
            const len = (p.count / signals.length) * c;
            // At rest every part runs a hair past its end, so the ring
            // reads as one solid colour with no seams.
            const whole = Math.min(len + 1, c) + " " + c;
            const split = Math.max(len - gap, 0.5) + " " + c;
            const seg = '<circle class="smx-seg" data-smx="' +
                p.type +
                '" cx="60" cy="60" r="' +
                r +
                '" stroke-dashoffset="' +
                -start +
                '" transform="rotate(-90 60 60)" style="--c:' +
                MIX_COLORS[p.type] +
                ";--whole:" +
                whole +
                ";--split:" +
                split +
                '"><title>' +
                window.escapeHtml(tr(MIX_NAMES[p.type]) + ": " + p.pct + "%") +
                "</title></circle>";
            start += len;
            return seg;
        })
            .join("");
        const rows = parts
            .map((p) => '<div class="smx-row" data-smx="' +
            p.type +
            '"><span class="smx-dot" style="background:' +
            MIX_COLORS[p.type] +
            '"></span><span>' +
            window.escapeHtml(tr(MIX_NAMES[p.type])) +
            " · " +
            p.count +
            "</span><b style=\"color:" +
            MIX_COLORS[p.type] +
            '">' +
            p.pct +
            "%</b></div>")
            .join("");
        const hint = isB2bScan(signals) && parts.some((p) => p.type === "info")
            ? tr("Info cards are not warnings, but each adds 5 points to the score (15 at most).")
            : tr("Hover the ring to see each part.");
        return (MIX_STYLE +
            '<div class="smx" id="safely-smx" style="--dom:' +
            MIX_COLORS[dom.type] +
            '" data-dom="' +
            dom.type +
            '">' +
            '<div class="smx-ring" id="safely-smx-ring" role="img" aria-label="' +
            window.escapeHtml(tr("Signal mix")) +
            '"><svg viewBox="0 0 120 120">' +
            '<circle class="smx-glow" cx="60" cy="60" r="' +
            r +
            '"/>' +
            '<circle cx="60" cy="60" r="' +
            r +
            '" fill="none" stroke="#1b1b20" stroke-width="12"/>' +
            segs +
            '<text id="safely-smx-pct" class="smx-pct" x="60" y="62" text-anchor="middle"></text>' +
            '<text id="safely-smx-name" class="smx-name" x="60" y="76" text-anchor="middle"></text>' +
            '<text id="safely-smx-count" class="smx-count" x="60" y="87" text-anchor="middle"></text>' +
            "</svg></div>" +
            '<div class="smx-side">' +
            '<div class="smx-title">' +
            window.escapeHtml(tr("Signal mix")) +
            "</div>" +
            '<div class="smx-lead" style="color:' +
            MIX_COLORS[dom.type] +
            '">' +
            window.escapeHtml(tr("Mostly: {type}", { type: tr(MIX_NAMES[dom.type]) })) +
            "</div>" +
            rows +
            '<div class="smx-hint">' +
            window.escapeHtml(hint) +
            "</div></div></div>");
    }
    function attachSignalMixListeners() {
        const box = document.getElementById("safely-smx");
        const ring = document.getElementById("safely-smx-ring");
        const pctEl = document.getElementById("safely-smx-pct");
        const nameEl = document.getElementById("safely-smx-name");
        const countEl = document.getElementById("safely-smx-count");
        if (!box || !ring || !pctEl || !nameEl || !countEl)
            return;
        const signals = (window.__safelyData || {}).signals || [];
        const parts = mixParts(signals);
        if (parts.length === 0)
            return;
        const dom = dominantPart(parts);
        const tab = document.getElementById("safely-tab-intelligence") || document;
        function show(p) {
            pctEl.textContent = p.pct + "%";
            pctEl.setAttribute("fill", MIX_COLORS[p.type]);
            nameEl.textContent = tr(MIX_NAMES[p.type]);
            nameEl.setAttribute("fill", MIX_COLORS[p.type]);
            countEl.textContent = tr("{n} of {total} cards", { n: p.count, total: signals.length });
        }
        // Highlights one kind everywhere: its ring part, its legend row and
        // its cards in the list. null clears the highlight.
        function focus(type) {
            box.querySelectorAll("[data-smx]").forEach((el) => {
                const mine = el.getAttribute("data-smx") === type;
                el.classList.toggle("smx-hot", type !== null && mine);
                el.classList.toggle("smx-dim", type !== null && !mine);
            });
            tab.querySelectorAll("[data-smx-type]").forEach((card) => {
                card.style.opacity =
                    type === null || card.getAttribute("data-smx-type") === type ? "" : "0.35";
            });
            const part = parts.find((p) => p.type === type);
            show(part || dom);
        }
        function open() {
            box.classList.add("smx-open");
        }
        function close() {
            box.classList.remove("smx-open");
            focus(null);
        }
        show(dom);
        ring.addEventListener("mouseenter", open);
        ring.addEventListener("mouseleave", close);
        box.querySelectorAll("[data-smx]").forEach((el) => {
            const type = el.getAttribute("data-smx");
            el.addEventListener("mouseenter", () => {
                open();
                focus(type);
            });
            el.addEventListener("mouseleave", () => {
                if (el.classList.contains("smx-row"))
                    close();
                else
                    focus(null);
            });
        });
        // Touch screens have no hover: a tap opens the ring and picks the
        // tapped part, a tap on the middle closes it again.
        ring.addEventListener("click", (e) => {
            const target = e.target.closest("[data-smx]");
            if (target) {
                open();
                focus(target.getAttribute("data-smx"));
            }
            else if (box.classList.contains("smx-open")) {
                close();
            }
            else {
                open();
            }
        });
    }
    // Real, direct TypeScript rendering for signal rows - deliberately
    // bypasses wasm.build_signal_rows, since the compiled WASM module
    // doesn't know how to parse the new ###CHECKLIST### marker. Keeping
    // this logic here, in one, single, real place, avoids needing to
    // touch or recompile the Rust/WASM source at all.
    function buildSignalRowsTs(signals) {
        return signals
            .map((s, idx) => {
            const color = MIX_COLORS[mixType(s.type)];
            const { realSub, checklist } = parseChecklistSignal(s.sub);
            const dropdownId = "safely-checklist-" + idx;
            return ('<div class="safely-check-card" data-smx-type="' +
                mixType(s.type) +
                '">' +
                '<div style="display:flex;justify-content:space-between;align-items:baseline;gap:8px;">' +
                '<div class="safely-check-title">' +
                window.escapeHtml(tr(capitalizeFirst(s.label))) +
                "</div>" +
                '<div style="font-weight:700;white-space:nowrap;font-size:13px;color:' +
                color +
                ';">' +
                window.escapeHtml(tr(capitalizeFirst(s.value))) +
                "</div></div>" +
                '<div class="safely-check-body">' +
                window.escapeHtml(capitalizeFirst(realSub) || "") +
                "</div>" +
                buildChecklistDropdown(dropdownId, checklist) +
                "</div>");
        })
            .join("");
    }
    function attachSocialPresenceListeners() {
        const toggle = document.getElementById("safely-social-toggle");
        const dropdown = document.getElementById("safely-social-dropdown");
        const arrow = document.getElementById("safely-social-arrow");
        const toggleText = document.getElementById("safely-social-toggle-text");
        if (toggle && dropdown && arrow && toggleText) {
            toggle.addEventListener("click", () => {
                const isOpen = dropdown.style.display !== "none";
                dropdown.style.display = isOpen ? "none" : "block";
                arrow.innerHTML = isOpen ? "&#9662;" : "&#9652;";
                toggleText.textContent = isOpen ? tr("Click to drop down") : tr("Click to drop up");
            });
        }
    }
    // Labels only the backend's B2B path produces. Used to tell a B2B
    // supplier scan apart from a consumer (OLX-style) scan without
    // relying on a separate platform list here that could drift out of
    // sync with the backend's scraper registry.
    const B2B_ONLY_LABELS = [
        "Company profile completeness",
        "Listing completeness",
        "Registration consistency",
    ];
    function isB2bScan(signals) {
        return signals.some((s) => B2B_ONLY_LABELS.includes(s.label));
    }
    const B2C_CHECKS = [
        ["Ask for a live video call", "Verify the item is physically in the seller's hands before sending any payment."],
        ["Check IMEI on delivery", "Dial *#06# on the device and confirm the number matches what the seller declared at deal creation."],
        ["Do not pay to number in listing", "A phone number in the listing could route your payment outside Safely escrow protection."],
    ];
    const B2B_CHECKS = [
        ["Use buyer protection if the platform offers it", "If the platform has protected payment (e.g. Trade Assurance on Alibaba), pay through it so the order is covered. If it has none, pay only by bank transfer to an account in the company's own name."],
        ["Match the business licence to the bank account", "Ask for the business licence and check that the company name on it matches the listing and the name on the bank account exactly."],
        ["Confirm bank details by phone or video", "Before the first payment, and whenever bank details change, confirm them live with a contact you already know. Never act on changed details sent only by email."],
        ["Order a sample first", "Pay for a sample and check its quality before placing a bulk order."],
        ["Ask for a live video call", "Ask the supplier to show where they work and your goods live: the production line if they are a factory, the warehouse or office if they are a trader or shipping company. This confirms they really operate where they say."],
    ];
    // Shown first on a B2B scan when Claude found an untraceable payment
    // method (Western Union / MoneyGram / crypto / gift cards / a
    // personal account). The backend marks it "Untraceable payment".
    const B2B_RISKY_PAYMENT_CHECK = [
        "Do not pay by Western Union, MoneyGram or crypto",
        "This supplier lists payment methods that cannot be reversed or traced to a company. Pay only by bank transfer to an account in the company's own name, or through the platform's protected payment.",
    ];
    // Shown first when the supplier wants the full price before shipment
    // by a normal method (the backend marks it "Full prepayment").
    const B2B_FULL_PREPAYMENT_CHECK = [
        "Don't pay everything before shipment",
        "This supplier asks for the full price before the goods are shipped. Try to pay the balance only against a copy of the Bill of Lading, or use a Letter of Credit, and pay only to a bank account in the company's own name.",
    ];
    // Shown first when the product needs a licence or prescription (botox,
    // fillers, prescription medicines). The backend adds a "Regulated
    // product" card only for these products.
    const B2B_REGULATED_PRODUCT_CHECK = [
        "Buy only from the brand owner or an authorised distributor",
        "This product needs a licence or prescription, and fakes of it can be dangerous. Ask the supplier for a letter from the brand owner showing they are an authorised distributor, and check it with the brand owner directly. You may also need your own import licence.",
    ];
    // Shown first when the company offers several classic bait products
    // of fake commodity deals (ICUMSA 45 sugar, EN590 diesel, Urea 46...).
    // The backend marks it "Commodity scam pattern".
    const B2B_COMMODITY_CHECK = [
        "Don't pay fees before an inspection",
        "Fake commodity sellers ask for fees, a deposit or document costs before anything ships. Pay nothing until the goods are inspected by a company you choose (such as SGS), and pay only by Letter of Credit.",
    ];
    function buildRecommendedChecks(signals) {
        const isB2b = isB2bScan(signals);
        const payment = isB2b
            ? signals.find((s) => s.label === "Advance payment request" && s.type === "caution")
            : undefined;
        // "Full prepayment" gets its own advice; any other flagged payment
        // ("Untraceable payment", or an older scan's "Detected") keeps the
        // Western Union warning.
        const paymentCheck = !payment
            ? []
            : payment.value === "Full prepayment"
                ? [B2B_FULL_PREPAYMENT_CHECK]
                : [B2B_RISKY_PAYMENT_CHECK];
        const regulatedCheck = isB2b && signals.some((s) => s.label === "Regulated product" && s.type === "caution")
            ? [B2B_REGULATED_PRODUCT_CHECK]
            : [];
        const commodityCheck = isB2b &&
            signals.some((s) => s.label === "Product range" && s.value === "Commodity scam pattern")
            ? [B2B_COMMODITY_CHECK]
            : [];
        const checks = isB2b
            ? commodityCheck.concat(regulatedCheck, paymentCheck, B2B_CHECKS)
            : B2C_CHECKS;
        const cards = checks
            .map(([title, body]) => '<div class="safely-check-card"><div class="safely-check-title">' +
            window.escapeHtml(tr(title)) +
            '</div><div class="safely-check-body">' +
            window.escapeHtml(tr(body)) +
            "</div></div>")
            .join("");
        return ('<div class="safely-section-label" style="margin-top:18px">' +
            tr("Recommended checks") +
            "</div>" +
            '<div style="display:flex;flex-direction:column;gap:8px">' +
            cards +
            "</div>");
    }
    // The coloured summary line at the top. The level always comes from
    // WASM; in Portuguese the sentence is written here, since WASM only
    // writes English.
    function summaryFor(signals) {
        const result = JSON.parse(wasm.analyze_signals(JSON.stringify(signals)));
        if (!isPortuguese())
            return result;
        const bad = signals.filter((s) => s.type === "bad" || s.type === "caution").length;
        const text = bad === 0
            ? tr("All {n} signals checked. No red flags detected.", { n: signals.length })
            : tr("{bad} of {n} signals need your attention.", { bad, n: signals.length });
        return { level: result.level, text };
    }
    function buildIntelligenceTab() {
        const pageData = window.__safelyData;
        const sigResult = summaryFor(pageData.signals || []);
        const summaryLvl = sigResult.level;
        const summaryText = sigResult.text;
        return ('<div class="safely-intel-summary safely-alert-' +
            summaryLvl +
            '"><span>&#9679;</span><span>' +
            summaryText +
            "</span></div>" +
            buildSignalMix(pageData.signals || []) +
            '<div class="safely-section-label" style="margin-top:14px">' +
            tr("Listing signals") +
            '</div><div style="display:flex;flex-direction:column;gap:8px">' +
            buildSignalRowsTs(pageData.signals) +
            "</div>" +
            buildSocialPresenceSection() +
            (function () {
                // On B2B scans the "Price analysis" row above already shows this
                // exact text, so the separate box would only repeat it.
                if (isB2bScan(pageData.signals))
                    return "";
                const priceSignal = pageData.signals.find((s) => s.label === "Price analysis");
                const verdict = priceSignal ? priceSignal.value : "unknown";
                const reasoning = priceSignal ? priceSignal.sub : tr("No price data available.");
                const verdictClass = verdict === "normal" ? "low" : verdict === "unknown" ? "low" : "caution";
                return ('<div class="safely-section-label" style="margin-top:18px">' +
                    tr("Price vs market") +
                    "</div>" +
                    '<div class="safely-network-alert safely-alert-' +
                    verdictClass +
                    '" style="margin-top:8px">' +
                    "<span>&#9679;</span>" +
                    "<div>" +
                    '<div style="font-weight:600;margin-bottom:4px">' +
                    tr(verdict.charAt(0).toUpperCase() + verdict.slice(1)) +
                    "</div>" +
                    '<div style="font-size:12px;opacity:0.85">' +
                    reasoning +
                    "</div>" +
                    "</div>" +
                    "</div>");
            })() +
            buildRecommendedChecks(pageData.signals || []) +
            buildRiskFactorsSection(pageData.riskFactors));
    }
    const SEVERITY_COLORS = {
        hard: "#ff5d5d",
        compound: "#f2b84c",
        soft: "#8e8e93",
    };
    const SEVERITY_LABELS = {
        // Not "Confirmed": the same check's row above can read "Not
        // confirmed" (e.g. legitimacy not confirmed), and the two looked
        // like they contradicted each other.
        hard: "Serious",
        compound: "Pattern match",
        soft: "Worth noting",
    };
    function capitalizeFirst(str) {
        if (!str)
            return str;
        return str.charAt(0).toUpperCase() + str.slice(1);
    }
    // Same ###CHECKLIST### parsing the signal rows already use above -
    // a risk factor's description is very often copied straight from
    // the signal.sub it was derived from (e.g. "Listing completeness"),
    // so without this same parsing step, the raw marker and the raw
    // "Name|true;Name|false" data was leaking straight into the visible
    // text instead of becoming a real, clickable checklist dropdown.
    function buildRiskFactorsSection(riskFactors) {
        if (!riskFactors || riskFactors.length === 0)
            return "";
        const rows = riskFactors
            .map((factor, idx) => {
            const color = SEVERITY_COLORS[factor.severity] || "#8e8e93";
            const severityLabel = tr(SEVERITY_LABELS[factor.severity] || factor.severity);
            const shortTitle = factor.contributing_signals && factor.contributing_signals.length > 0
                ? factor.contributing_signals.map((label) => tr(label)).join(" + ")
                : tr(capitalizeFirst(factor.name.replace(/_/g, " ")));
            const { realSub, checklist } = parseChecklistSignal(factor.description || "");
            const dropdownId = "safely-riskfactor-checklist-" + idx;
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
                window.escapeHtml(capitalizeFirst(realSub) || "") +
                "</div>" +
                buildChecklistDropdown(dropdownId, checklist) +
                "</div>");
        })
            .join("");
        return ('<div class="safely-section-label" style="margin-top:18px">' +
            tr("Risk Factors") +
            '</div><div style="display:flex;flex-direction:column;gap:8px">' +
            rows +
            "</div>");
    }
    function attachAllChecklistListeners() {
        attachSocialPresenceListeners();
        attachSignalMixListeners();
        const pageData = window.__safelyData;
        (pageData.signals || []).forEach((_, idx) => {
            attachChecklistListener("safely-checklist-" + idx);
        });
        (pageData.riskFactors || []).forEach((_, idx) => {
            attachChecklistListener("safely-riskfactor-checklist-" + idx);
        });
    }
    function redrawIntelligenceTab() {
        const tabEl = document.getElementById("safely-tab-intelligence");
        if (tabEl) {
            tabEl.innerHTML = buildIntelligenceTab();
            attachAllChecklistListeners();
        }
    }
    window.__safelyAddTab("intelligence", "Intelligence", buildIntelligenceTab(), '<svg viewBox="0 0 24 24" fill="none" stroke="#8e8e93" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="2"/><path d="M16.24 7.76a6 6 0 010 8.48"/><path d="M19.07 4.93a10 10 0 010 14.14"/><path d="M7.76 16.24a6 6 0 010-8.48"/><path d="M4.93 19.07a10 10 0 010-14.14"/></svg>', attachAllChecklistListeners);
    window.addEventListener("safely-data-ready", redrawIntelligenceTab);
    window.addEventListener("safely-lang-changed", redrawIntelligenceTab);
    window.addEventListener("safely-result-text-changed", redrawIntelligenceTab);
})();
