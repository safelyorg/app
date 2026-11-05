"use strict";
// The "Details" tab (its id stays "intelligence").
(function () {
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
    if (!window.__safelyAddTab)
        return;
    function esc(text) {
        return escapeHtml(text);
    }
    function capitalizeFirst(str) {
        return str ? str.charAt(0).toUpperCase() + str.slice(1) : str;
    }
    function isFlagged(type) {
        return type !== "good" && type !== "info";
    }
    const CHEVRON = '<svg class="safely-chev" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg>';
    // How many rows show before "Show more".
    const FIRST_FINE = 2;
    const FIRST_STEPS = 3;
    const FIRST_PLATFORMS = 3;
    function parseChecklistSignal(sub) {
        const marker = "###CHECKLIST###";
        const idx = sub.indexOf(marker);
        if (idx === -1)
            return { realSub: sub, checklist: [] };
        const checklist = sub
            .slice(idx + marker.length)
            .split(";")
            .filter(Boolean)
            .map((entry) => {
            const [name, present] = entry.split("|");
            return [name, present === "true"];
        });
        return { realSub: sub.slice(0, idx), checklist };
    }
    // One row. With extra text it opens on click; extra rows stay hidden
    // until "Show more".
    function row(opts) {
        const canOpen = !!opts.moreHTML;
        return ('<div class="safely-row' +
            (opts.extra ? " safely-extra" : "") +
            (opts.ticked ? " safely-ticked" : "") +
            '">' +
            (canOpen
                ? '<button type="button" class="safely-row-head" data-safely-open aria-expanded="false">'
                : '<div class="safely-row-head">') +
            opts.mark +
            '<span class="safely-row-title">' +
            esc(opts.title) +
            (opts.value ? '<span class="safely-row-value">' + esc(opts.value) + "</span>" : "") +
            "</span>" +
            (canOpen ? CHEVRON + "</button>" : "</div>") +
            (canOpen
                ? '<div class="safely-row-more"><div><div class="safely-row-text">' +
                    opts.moreHTML +
                    "</div></div></div>"
                : "") +
            "</div>");
    }
    function moreButton(hidden) {
        if (hidden <= 0)
            return "";
        const label = tr("Show {n} more", { n: hidden });
        return ('<button type="button" class="safely-more-btn" data-safely-more="' +
            esc(label) +
            '"><span>' +
            esc(label) +
            "</span>" +
            CHEVRON +
            "</button>");
    }
    function signalRow(s, extra) {
        const flagged = isFlagged(s.type);
        const { realSub, checklist } = parseChecklistSignal(s.sub || "");
        const list = checklist.length === 0
            ? ""
            : "<ul>" +
                checklist
                    .map(([name, present]) => '<li class="' +
                    (present ? "safely-has" : "safely-missing") +
                    '"><span class="safely-li-mark">' +
                    (present ? SAFELY_CHECK_ICON : SAFELY_CROSS_ICON) +
                    "</span>" +
                    esc(tr(name)) +
                    "</li>")
                    .join("") +
                "</ul>";
        const text = capitalizeFirst(realSub.trim());
        return row({
            mark: '<span class="safely-mark ' +
                (flagged ? "safely-mark-flag" : "safely-mark-ok") +
                '">' +
                (flagged ? "!" : s.type === "info" ? "i" : SAFELY_CHECK_ICON) +
                "</span>",
            title: tr(capitalizeFirst(s.label)),
            value: tr(capitalizeFirst(s.value)),
            moreHTML: text || list ? esc(text) + list : "",
            extra,
        });
    }
    // ── Outside records ──
    // What official lists, trade fairs and review sites say about the
    // seller (backend services/sources/databases/). They get their own
    // section, in groups, instead of sitting between the listing's own
    // checks. The same groups and order as the dashboard
    // (dashboard/ts/detail_view.ts) - change both together.
    const OUTSIDE_GROUPS = [
        [
            "Official lists",
            [
                "Sanctions lists",
                "Company register (GLEIF)",
                "Company number",
                "Official register",
                "EU VAT number",
                "US stock market (SEC)",
                "US government register (SAM.gov)",
                "China Customs AEO",
            ],
        ],
        ["Reputation", ["Reclame Aqui", "BBB rating"]],
        [
            "Trade shows and chambers",
            [
                "Canton Fair",
                "HKTDC fair",
                "US trade show",
                "Brazil trade show",
                "France trade show",
                "Chamber of commerce",
            ],
        ],
        ["Website history", ["Website history", "Website traffic", "Security certificate"]],
    ];
    function isOutsideRecord(s) {
        return OUTSIDE_GROUPS.some(([, labels]) => labels.indexOf(s.label) !== -1);
    }
    // Warnings first, then good signs, then notes.
    function outsideOrder(s) {
        return isFlagged(s.type) ? 0 : s.type === "good" ? 1 : 2;
    }
    // The web addresses in a check's text, and the text without them
    // ("... Exhibitor page: https://..." -> "..."): each address becomes
    // a button instead.
    function splitLinks(text) {
        const links = (text.match(/https?:\/\/[^\s)]+/g) || []).map((u) => u.replace(/[.,;]+$/, ""));
        let rest = text.replace(/:?\s*https?:\/\/[^\s)]+/g, "").trim();
        // A label left alone at the end ("... own. Exhibitor page").
        rest = rest.replace(/([.!?])\s+[^.!?]{1,40}$/, "$1");
        return { text: rest, links: links.filter((u, i) => links.indexOf(u) === i) };
    }
    function siteName(url) {
        const host = url.replace(/^https?:\/\//, "").split(/[/?#]/)[0];
        return host.replace(/^www\./, "");
    }
    const OUT_ICON = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M7 17L17 7"/><path d="M8 7h9v9"/></svg>';
    function sourceButtons(links) {
        if (links.length === 0)
            return "";
        return ('<span class="safely-sources">' +
            links
                .map((url) => '<a class="safely-source-btn" href="' +
                esc(url) +
                '" target="_blank" rel="noopener noreferrer">' +
                esc(tr("Open on {site}", { site: siteName(url) })) +
                OUT_ICON +
                "</a>")
                .join("") +
            "</span>");
    }
    function outsideRow(s) {
        const flagged = isFlagged(s.type);
        const { text, links } = splitLinks(capitalizeFirst((s.sub || "").trim()));
        return row({
            mark: '<span class="safely-mark ' +
                (flagged ? "safely-mark-flag" : "safely-mark-ok") +
                '">' +
                (flagged ? "!" : s.type === "info" ? "i" : SAFELY_CHECK_ICON) +
                "</span>",
            title: tr(s.label),
            value: tr(capitalizeFirst(s.value)),
            moreHTML: text || links.length ? esc(text) + sourceButtons(links) : "",
        });
    }
    // "1 warning · 3 good signs · 2 notes".
    function outsideSummary(records) {
        const warnings = records.filter((s) => isFlagged(s.type)).length;
        const good = records.filter((s) => s.type === "good").length;
        const notes = records.length - warnings - good;
        const parts = [];
        if (warnings > 0)
            parts.push(warnings === 1 ? tr("1 warning") : tr("{n} warnings", { n: warnings }));
        if (good > 0)
            parts.push(good === 1 ? tr("1 good sign") : tr("{n} good signs", { n: good }));
        if (notes > 0)
            parts.push(notes === 1 ? tr("1 note") : tr("{n} notes", { n: notes }));
        return parts.join(" · ");
    }
    function buildOutsideRecords(signals) {
        const records = signals.filter(isOutsideRecord);
        if (records.length === 0)
            return "";
        const groups = OUTSIDE_GROUPS.map(([name, labels]) => {
            const inGroup = records
                .filter((s) => labels.indexOf(s.label) !== -1)
                .sort((a, b) => outsideOrder(a) - outsideOrder(b));
            if (inGroup.length === 0)
                return "";
            return ('<div class="safely-subheading">' +
                esc(tr(name)) +
                '</div><div class="safely-rows">' +
                inGroup.map(outsideRow).join("") +
                "</div>");
        }).join("");
        return ('<div class="safely-section safely-outside">' +
            '<div class="safely-heading">' +
            esc(tr("Outside records")) +
            "</div>" +
            '<div class="safely-section-note">' +
            esc(tr("What official lists, trade fairs and review sites say about this seller.")) +
            '</div><div class="safely-section-summary">' +
            esc(outsideSummary(records)) +
            "</div>" +
            groups +
            "</div>");
    }
    function buildCounts(flagged, fine) {
        return ('<div class="safely-counts">' +
            '<div class="safely-count' +
            (flagged > 0 ? " safely-count-hot" : "") +
            '"><b>' +
            flagged +
            "</b><small>" +
            esc(tr(flagged === 1 ? "needs a look" : "need a look")) +
            '</small></div><div class="safely-count"><b>' +
            fine +
            "</b><small>" +
            esc(tr(fine === 1 ? "looks fine" : "look fine")) +
            "</small></div></div>");
    }
    function buildSignalLists(signals) {
        const flagged = signals.filter((s) => isFlagged(s.type));
        const fine = signals.filter((s) => !isFlagged(s.type));
        let html = "";
        if (flagged.length > 0) {
            html +=
                '<div class="safely-heading">' +
                    esc(tr("Needs a look")) +
                    '</div><div class="safely-rows">' +
                    flagged.map((s) => signalRow(s, false)).join("") +
                    "</div>";
        }
        if (fine.length > 0) {
            html +=
                '<div class="safely-heading">' +
                    esc(tr("Looks fine")) +
                    '</div><div class="safely-rows">' +
                    fine.map((s, i) => signalRow(s, i >= FIRST_FINE)).join("") +
                    moreButton(fine.length - FIRST_FINE) +
                    "</div>";
        }
        return html;
    }
    const PLATFORM_ORDER = [
        "Facebook", "LinkedIn", "TikTok", "Instagram", "Reddit", "Trustpilot",
        "Reviews", "Contact (Facebook)", "Contact (LinkedIn)", "Contact (Web)",
    ];
    // The whole-web scam searches (backend osint.rs WEB_SCAM_SEARCH and
    // COMPLAINT_SITES). They get their own "Scam warnings online" section
    // instead of a row in "Online presence".
    const WEB_SCAM_ROWS = ["Google search", "Complaint sites"];
    function socialResults() {
        return (window.__safelyData || {}).socialCandidates || [];
    }
    // Each search's links, one list per row name, without repeats.
    function groupLinks(results) {
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
        return grouped;
    }
    function linksHTML(links) {
        return links
            .map((link) => '<a class="safely-link-out" href="' +
            esc(link.url) +
            '" target="_blank" rel="noopener noreferrer">' +
            esc(link.title || link.url) +
            "</a>")
            .join("");
    }
    // "Scam warnings online": the name found next to scam words anywhere
    // on the web, and on complaint sites. Hidden when these searches did
    // not run (older scans, or web search switched off).
    function buildScamWarnings() {
        const results = socialResults().filter((r) => WEB_SCAM_ROWS.indexOf(r.platform) !== -1);
        if (results.length === 0)
            return "";
        const grouped = groupLinks(results);
        const found = WEB_SCAM_ROWS.filter((p) => grouped[p] && grouped[p].length > 0);
        const rows = found.length === 0
            ? '<div class="safely-row"><div class="safely-row-head"><span class="safely-mark safely-mark-ok">' +
                SAFELY_CHECK_ICON +
                '</span><span class="safely-row-title safely-row-quiet">' +
                esc(tr("No scam warnings found for this name")) +
                "</span></div></div>"
            : found
                .map((platform) => row({
                mark: '<span class="safely-mark safely-mark-flag">!</span>',
                title: tr(platform),
                value: grouped[platform].length === 1
                    ? tr("1 link")
                    : tr("{n} links", { n: grouped[platform].length }),
                moreHTML: esc(tr("Read these before you pay. Some may be about something else.")) +
                    linksHTML(grouped[platform]),
            }))
                .join("");
        return ('<div class="safely-heading">' +
            esc(tr("Scam warnings online")) +
            '</div><div class="safely-rows">' +
            rows +
            "</div>");
    }
    // One row per platform where something was found; it opens to show
    // every link. The first 3 show, the rest sit under "Show more".
    // Platforms with nothing found share one line at the end, always shown.
    // The same platform can be searched under several names, so the
    // results are merged first.
    function buildPresence() {
        const results = socialResults().filter((r) => WEB_SCAM_ROWS.indexOf(r.platform) === -1);
        if (results.length === 0)
            return "";
        const grouped = groupLinks(results);
        const order = PLATFORM_ORDER.filter((p) => p in grouped).concat(Object.keys(grouped).filter((p) => PLATFORM_ORDER.indexOf(p) === -1));
        const found = order.filter((p) => grouped[p].length > 0);
        const missing = order.filter((p) => grouped[p].length === 0);
        const rows = found
            .map((platform, i) => {
            const links = grouped[platform];
            return row({
                extra: i >= FIRST_PLATFORMS,
                mark: '<span class="safely-mark safely-mark-ok">' + SAFELY_CHECK_ICON + "</span>",
                title: tr(platform),
                value: links.length === 1 ? tr("1 link") : tr("{n} links", { n: links.length }),
                moreHTML: linksHTML(links),
            });
        })
            .join("");
        const none = missing.length === 0
            ? ""
            : '<div class="safely-row"><div class="safely-row-head"><span class="safely-mark safely-mark-none">' + SAFELY_CROSS_ICON + '</span><span class="safely-row-title safely-row-quiet">' +
                esc(tr("Not found on {list}", { list: missing.map((p) => tr(p)).join(", ") })) +
                "</span></div></div>";
        return ('<div class="safely-heading">' +
            esc(tr("Online presence")) +
            '</div><div class="safely-rows">' +
            rows +
            moreButton(found.length - FIRST_PLATFORMS) +
            none +
            "</div>");
    }
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
    // Shown first when the product needs a licence or prescription and the
    // seller is not its maker (the backend marks it "Licence needed, not
    // the maker"), e.g. botox sold by a trading company.
    const B2B_REGULATED_PRODUCT_CHECK = [
        "Buy only from the brand owner or an authorised distributor",
        "This product needs a licence or prescription, and fakes of it can be dangerous. Ask the supplier for a letter from the brand owner showing they are an authorised distributor, and check it with the brand owner directly. You may also need your own import licence.",
    ];
    // Shown first when the product needs a licence but the seller makes it
    // itself (the backend marks it "Licence needed"), e.g. a defence
    // manufacturer selling its own riot-control equipment.
    const B2B_LICENCE_CHECK = [
        "Check the licences before you order",
        "This product needs a licence to sell or import. Make sure you are allowed to import it into your country, and ask the supplier for its export licence and any end-user certificate the sale needs before you pay.",
    ];
    // Shown first when the company offers several classic bait products
    // of fake commodity deals (ICUMSA 45 sugar, EN590 diesel, Urea 46...).
    // The backend marks it "Commodity scam pattern".
    const B2B_COMMODITY_CHECK = [
        "Don't pay fees before an inspection",
        "Fake commodity sellers ask for fees, a deposit or document costs before anything ships. Pay nothing until the goods are inspected by a company you choose (such as SGS), and pay only by Letter of Credit.",
    ];
    // "Before you pay" on a shop's own website ("Check this website"):
    // consumer steps, not the supplier ones above. The same list as the
    // dashboard (ts/detail_view.ts) and the PDF (backend pdf_report.rs).
    const WEBSITE_CHECKS = [
        ["Pay by card or PayPal", "Card and PayPal payments can be disputed if the order never arrives or is not as described. Money sent by bank transfer, Pix, crypto or gift card cannot be got back."],
        ["Read the return policy first", "Check how returns and refunds work before you buy, and save a copy. If the shop has no clear return policy, ask before paying."],
        ["Start with a small order", "Buy one item first and wait until it arrives as described before placing a bigger order."],
        ["Keep proof of your order", "Save screenshots of the product page, the price and your order confirmation. You will need them for a refund or a dispute."],
        ["Contact the shop before a big order", "Write to the email or phone number the shop gives and see if a real person answers. A shop that never replies is a warning sign."],
    ];
    // Shown first when the address copies a famous brand.
    const WEBSITE_LOOKALIKE_CHECK = [
        "Buy only on the brand's real website",
        "This address copies a famous brand's name, but it is not the brand's own website. Type the brand's real address yourself and buy there instead.",
    ];
    // Shown first when the shop only shows ways to pay that can't be reversed.
    const WEBSITE_RISKY_PAYMENT_CHECK = [
        "Don't pay by bank transfer, Pix or crypto",
        "This shop only shows ways to pay that cannot be reversed. Pay only by card or PayPal. If the shop does not accept them, do not buy.",
    ];
    // The steps for a shop's own website: the special ones its checks call
    // for first, then the usual ones.
    function websiteChecks(signals) {
        const lookalike = signals.some((s) => s.label === "Domain check" && s.type === "caution" && s.value.indexOf("Looks like") === 0);
        const risky = signals.some((s) => s.label === "Payment at checkout" && s.value === "Risky methods only");
        return (lookalike ? [WEBSITE_LOOKALIKE_CHECK] : [])
            .concat(risky ? [WEBSITE_RISKY_PAYMENT_CHECK] : [])
            .concat(isSupplierWebsite(signals) ? SUPPLIER_WEBSITE_CHECKS : WEBSITE_CHECKS);
    }
    // A factory or supplier's own website: it sells by quote, so no prices
    // and no way to pay are shown. Its buyers pay by bank transfer, like a
    // marketplace supplier, so it gets supplier steps, not shop ones.
    const SUPPLIER_WEBSITE_CHECKS = [
        ["Pay only to the company's own bank account", "Pay by bank transfer only to an account in the company's own name, never to a person. Ask for the bank details in writing and check that the name matches the company."],
    ].concat(B2B_CHECKS.slice(1));
    function isSupplierWebsite(signals) {
        const notShown = (label) => signals.some((s) => s.label === label && s.value === "Not shown");
        return notShown("Payment at checkout") && notShown("Prices");
    }
    function isWebsiteScan() {
        const data = window.__safelyData || {};
        return !!data.seller && data.seller.platform === "Website";
    }
    function buildRecommendedChecks(signals) {
        const payment = signals.find((s) => s.label === "Advance payment request" && s.type === "caution");
        // "Full prepayment" gets its own advice; any other flagged payment
        // ("Untraceable payment", or an older scan's "Detected") keeps the
        // Western Union warning.
        const paymentCheck = !payment
            ? []
            : payment.value === "Full prepayment"
                ? [B2B_FULL_PREPAYMENT_CHECK]
                : [B2B_RISKY_PAYMENT_CHECK];
        const regulated = signals.find((s) => s.label === "Regulated product" && s.type === "caution");
        const regulatedCheck = !regulated
            ? []
            : regulated.value === "Licence needed, not the maker"
                ? [B2B_REGULATED_PRODUCT_CHECK]
                : [B2B_LICENCE_CHECK];
        const commodityCheck = signals.some((s) => s.label === "Product range" && s.value === "Commodity scam pattern")
            ? [B2B_COMMODITY_CHECK]
            : [];
        const checks = isWebsiteScan()
            ? websiteChecks(signals)
            : commodityCheck.concat(regulatedCheck, paymentCheck, B2B_CHECKS);
        return ('<div class="safely-heading">' +
            esc(tr("Before you pay")) +
            '</div><div class="safely-rows">' +
            checks
                .map(([title, body], i) => row({
                mark: '<span class="safely-tick" data-safely-tick="' +
                    esc(title) +
                    '" role="checkbox" aria-checked="' +
                    ticked.has(title) +
                    '">' +
                    SAFELY_CHECK_ICON +
                    "</span>",
                title: tr(title),
                moreHTML: esc(tr(body)),
                extra: i >= FIRST_STEPS,
                ticked: ticked.has(title),
            }))
                .join("") +
            moreButton(checks.length - FIRST_STEPS) +
            "</div>");
    }
    // Steps ticked in "Before you pay" (English titles), kept while the
    // tab redraws, e.g. on a language change. A new scan clears them.
    const ticked = new Set();
    // panel.ts flips the row's class first; this only remembers it.
    document.addEventListener("click", (e) => {
        const tick = e.target.closest("[data-safely-tick]");
        if (!tick)
            return;
        const key = tick.getAttribute("data-safely-tick") || "";
        const row = tick.closest(".safely-row");
        if (row && row.classList.contains("safely-ticked"))
            ticked.add(key);
        else
            ticked.delete(key);
    });
    function buildDetailsTab() {
        const signals = (window.__safelyData || {}).signals || [];
        // The listing's own checks; outside records have their own section.
        const own = signals.filter((s) => !isOutsideRecord(s));
        const flagged = own.filter((s) => isFlagged(s.type)).length;
        return (buildCounts(flagged, own.length - flagged) +
            buildSignalLists(own) +
            buildOutsideRecords(signals) +
            buildPresence() +
            buildScamWarnings() +
            buildRecommendedChecks(signals));
    }
    function redrawDetailsTab() {
        const tabEl = document.getElementById("safely-tab-intelligence");
        if (tabEl)
            tabEl.innerHTML = buildDetailsTab();
    }
    window.__safelyAddTab("intelligence", "Details", buildDetailsTab(), '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="2"/><path d="M16.24 7.76a6 6 0 010 8.48"/><path d="M19.07 4.93a10 10 0 010 14.14"/><path d="M7.76 16.24a6 6 0 010-8.48"/><path d="M4.93 19.07a10 10 0 010-14.14"/></svg>', null);
    window.addEventListener("safely-data-ready", () => {
        ticked.clear();
        redrawDetailsTab();
    });
    window.addEventListener("safely-lang-changed", redrawDetailsTab);
    window.addEventListener("safely-result-text-changed", redrawDetailsTab);
})();
