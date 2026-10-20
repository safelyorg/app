interface SafelySignal {
  label: string;
  sub: string;
  value: string;
  type: string;
}

interface SocialCandidateLink {
  platform: string;
  title: string;
  url: string;
}

interface PlatformCheckResult {
  platform: string;
  variant_searched: string;
  found: boolean;
  candidates: SocialCandidateLink[];
}

// The "Details" tab (its id stays "intelligence").
(function () {
  "use strict";

  // Translates through core/i18n.ts; plain English if it isn't loaded.
  // Only what is SHOWN is translated - the signal labels and values
  // the code below checks ("Advance payment request", "Full
  // prepayment"...) always stay in English in the data itself.
  function tr(en: string, vars?: Record<string, string | number>): string {
    const i18n = (window as any).__safelyI18n;
    if (i18n) return i18n.t(en, vars);
    if (!vars) return en;
    return en.replace(/\{(\w+)\}/g, (whole, key) =>
      Object.prototype.hasOwnProperty.call(vars, key) ? String(vars[key]) : whole,
    );
  }

  if (!(window as any).__safelyAddTab) return;

  function esc(text: string): string {
    return escapeHtml(text);
  }

  function capitalizeFirst(str: string): string {
    return str ? str.charAt(0).toUpperCase() + str.slice(1) : str;
  }

  function isFlagged(type: string): boolean {
    return type !== "good" && type !== "info";
  }

  const CHEVRON =
    '<svg class="safely-chev" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg>';

  // How many rows show before "Show more".
  const FIRST_FINE = 2;
  const FIRST_STEPS = 3;
  const FIRST_PLATFORMS = 3;

  function parseChecklistSignal(sub: string): { realSub: string; checklist: [string, boolean][] } {
    const marker = "###CHECKLIST###";
    const idx = sub.indexOf(marker);
    if (idx === -1) return { realSub: sub, checklist: [] };
    const checklist: [string, boolean][] = sub
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
  function row(opts: {
    mark: string;
    title: string;
    value?: string;
    moreHTML?: string;
    extra?: boolean;
    ticked?: boolean;
  }): string {
    const canOpen = !!opts.moreHTML;
    return (
      '<div class="safely-row' +
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
      "</div>"
    );
  }

  function moreButton(hidden: number): string {
    if (hidden <= 0) return "";
    const label = tr("Show {n} more", { n: hidden });
    return (
      '<button type="button" class="safely-more-btn" data-safely-more="' +
      esc(label) +
      '"><span>' +
      esc(label) +
      "</span>" +
      CHEVRON +
      "</button>"
    );
  }

  function signalRow(s: SafelySignal, extra: boolean): string {
    const flagged = isFlagged(s.type);
    const { realSub, checklist } = parseChecklistSignal(s.sub || "");
    const list =
      checklist.length === 0
        ? ""
        : "<ul>" +
          checklist
            .map(
              ([name, present]) =>
                '<li class="' +
                (present ? "safely-has" : "safely-missing") +
                '"><span class="safely-li-mark">' +
                (present ? SAFELY_CHECK_ICON : SAFELY_CROSS_ICON) +
                "</span>" +
                esc(tr(name)) +
                "</li>",
            )
            .join("") +
          "</ul>";
    const text = capitalizeFirst(realSub.trim());
    return row({
      mark:
        '<span class="safely-mark ' +
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

  function buildCounts(flagged: number, fine: number): string {
    return (
      '<div class="safely-counts">' +
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
      "</small></div></div>"
    );
  }

  function buildSignalLists(signals: SafelySignal[]): string {
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

  // One row per platform where something was found; it opens to show
  // every link. The first 3 show, the rest sit under "Show more".
  // Platforms with nothing found share one line at the end, always shown.
  // The same platform can be searched under several names, so the
  // results are merged first.
  function buildPresence(): string {
    const results: PlatformCheckResult[] = ((window as any).__safelyData || {}).socialCandidates || [];
    if (results.length === 0) return "";
    const grouped: Record<string, SocialCandidateLink[]> = {};
    results.forEach((entry) => {
      if (!(entry.platform in grouped)) grouped[entry.platform] = [];
      entry.candidates.forEach((c) => {
        if (!grouped[entry.platform].some((existing) => existing.url === c.url)) {
          grouped[entry.platform].push(c);
        }
      });
    });
    const order = PLATFORM_ORDER.filter((p) => p in grouped).concat(
      Object.keys(grouped).filter((p) => PLATFORM_ORDER.indexOf(p) === -1),
    );
    const found = order.filter((p) => grouped[p].length > 0);
    const missing = order.filter((p) => grouped[p].length === 0);
    const rows = found
      .map((platform, i) => {
        const links = grouped[platform];
        return row({
          extra: i >= FIRST_PLATFORMS,
          mark: '<span class="safely-mark safely-mark-ok">' + SAFELY_CHECK_ICON + "</span>",
          title: tr(platform),
          value:
            links.length === 1 ? tr("1 link") : tr("{n} links", { n: links.length }),
          moreHTML: links
            .map(
              (link) =>
                '<a class="safely-link-out" href="' +
                esc(link.url) +
                '" target="_blank" rel="noopener noreferrer">' +
                esc(link.title || link.url) +
                "</a>",
            )
            .join(""),
        });
      })
      .join("");
    const none =
      missing.length === 0
        ? ""
        : '<div class="safely-row"><div class="safely-row-head"><span class="safely-mark safely-mark-none">' + SAFELY_CROSS_ICON + '</span><span class="safely-row-title safely-row-quiet">' +
          esc(tr("Not found on {list}", { list: missing.map((p) => tr(p)).join(", ") })) +
          "</span></div></div>";
    return (
      '<div class="safely-heading">' +
      esc(tr("Online presence")) +
      '</div><div class="safely-rows">' +
      rows +
      moreButton(found.length - FIRST_PLATFORMS) +
      none +
      "</div>"
    );
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

  function isB2bScan(signals: SafelySignal[]): boolean {
    return signals.some((s) => B2B_ONLY_LABELS.includes(s.label));
  }

  const B2C_CHECKS: [string, string][] = [
    ["Ask for a live video call", "Verify the item is physically in the seller's hands before sending any payment."],
    ["Check IMEI on delivery", "Dial *#06# on the device and confirm the number matches what the seller declared at deal creation."],
    ["Do not pay to number in listing", "A phone number in the listing could route your payment outside Safely escrow protection."],
  ];

  const B2B_CHECKS: [string, string][] = [
    ["Use buyer protection if the platform offers it", "If the platform has protected payment (e.g. Trade Assurance on Alibaba), pay through it so the order is covered. If it has none, pay only by bank transfer to an account in the company's own name."],
    ["Match the business licence to the bank account", "Ask for the business licence and check that the company name on it matches the listing and the name on the bank account exactly."],
    ["Confirm bank details by phone or video", "Before the first payment, and whenever bank details change, confirm them live with a contact you already know. Never act on changed details sent only by email."],
    ["Order a sample first", "Pay for a sample and check its quality before placing a bulk order."],
    ["Ask for a live video call", "Ask the supplier to show where they work and your goods live: the production line if they are a factory, the warehouse or office if they are a trader or shipping company. This confirms they really operate where they say."],
  ];

  // Shown first on a B2B scan when Claude found an untraceable payment
  // method (Western Union / MoneyGram / crypto / gift cards / a
  // personal account). The backend marks it "Untraceable payment".
  const B2B_RISKY_PAYMENT_CHECK: [string, string] = [
    "Do not pay by Western Union, MoneyGram or crypto",
    "This supplier lists payment methods that cannot be reversed or traced to a company. Pay only by bank transfer to an account in the company's own name, or through the platform's protected payment.",
  ];

  // Shown first when the supplier wants the full price before shipment
  // by a normal method (the backend marks it "Full prepayment").
  const B2B_FULL_PREPAYMENT_CHECK: [string, string] = [
    "Don't pay everything before shipment",
    "This supplier asks for the full price before the goods are shipped. Try to pay the balance only against a copy of the Bill of Lading, or use a Letter of Credit, and pay only to a bank account in the company's own name.",
  ];

  // Shown first when the product needs a licence or prescription and the
  // seller is not its maker (the backend marks it "Licence needed, not
  // the maker"), e.g. botox sold by a trading company.
  const B2B_REGULATED_PRODUCT_CHECK: [string, string] = [
    "Buy only from the brand owner or an authorised distributor",
    "This product needs a licence or prescription, and fakes of it can be dangerous. Ask the supplier for a letter from the brand owner showing they are an authorised distributor, and check it with the brand owner directly. You may also need your own import licence.",
  ];

  // Shown first when the product needs a licence but the seller makes it
  // itself (the backend marks it "Licence needed"), e.g. a defence
  // manufacturer selling its own riot-control equipment.
  const B2B_LICENCE_CHECK: [string, string] = [
    "Check the licences before you order",
    "This product needs a licence to sell or import. Make sure you are allowed to import it into your country, and ask the supplier for its export licence and any end-user certificate the sale needs before you pay.",
  ];

  // Shown first when the company offers several classic bait products
  // of fake commodity deals (ICUMSA 45 sugar, EN590 diesel, Urea 46...).
  // The backend marks it "Commodity scam pattern".
  const B2B_COMMODITY_CHECK: [string, string] = [
    "Don't pay fees before an inspection",
    "Fake commodity sellers ask for fees, a deposit or document costs before anything ships. Pay nothing until the goods are inspected by a company you choose (such as SGS), and pay only by Letter of Credit.",
  ];


  function buildRecommendedChecks(signals: SafelySignal[]): string {
    const isB2b = isB2bScan(signals);
    const payment = isB2b
      ? signals.find((s) => s.label === "Advance payment request" && s.type === "caution")
      : undefined;
    // "Full prepayment" gets its own advice; any other flagged payment
    // ("Untraceable payment", or an older scan's "Detected") keeps the
    // Western Union warning.
    const paymentCheck: [string, string][] = !payment
      ? []
      : payment.value === "Full prepayment"
        ? [B2B_FULL_PREPAYMENT_CHECK]
        : [B2B_RISKY_PAYMENT_CHECK];
    const regulated = isB2b
      ? signals.find((s) => s.label === "Regulated product" && s.type === "caution")
      : undefined;
    const regulatedCheck: [string, string][] = !regulated
      ? []
      : regulated.value === "Licence needed, not the maker"
        ? [B2B_REGULATED_PRODUCT_CHECK]
        : [B2B_LICENCE_CHECK];
    const commodityCheck: [string, string][] =
      isB2b &&
      signals.some((s) => s.label === "Product range" && s.value === "Commodity scam pattern")
        ? [B2B_COMMODITY_CHECK]
        : [];
    const checks = isB2b
      ? commodityCheck.concat(regulatedCheck, paymentCheck, B2B_CHECKS)
      : B2C_CHECKS;
    return (
      '<div class="safely-heading">' +
      esc(tr("Before you pay")) +
      '</div><div class="safely-rows">' +
      checks
        .map(([title, body], i) =>
          row({
            mark:
              '<span class="safely-tick" data-safely-tick="' +
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
          }),
        )
        .join("") +
      moreButton(checks.length - FIRST_STEPS) +
      "</div>"
    );
  }

  // Steps ticked in "Before you pay" (English titles), kept while the
  // tab redraws, e.g. on a language change. A new scan clears them.
  const ticked = new Set<string>();

  // panel.ts flips the row's class first; this only remembers it.
  document.addEventListener("click", (e) => {
    const tick = (e.target as HTMLElement).closest("[data-safely-tick]") as HTMLElement | null;
    if (!tick) return;
    const key = tick.getAttribute("data-safely-tick") || "";
    const row = tick.closest(".safely-row");
    if (row && row.classList.contains("safely-ticked")) ticked.add(key);
    else ticked.delete(key);
  });

  function buildDetailsTab(): string {
    const signals: SafelySignal[] = ((window as any).__safelyData || {}).signals || [];
    const flagged = signals.filter((s) => isFlagged(s.type)).length;
    return (
      buildCounts(flagged, signals.length - flagged) +
      buildSignalLists(signals) +
      buildPresence() +
      buildRecommendedChecks(signals)
    );
  }

  function redrawDetailsTab(): void {
    const tabEl = document.getElementById("safely-tab-intelligence");
    if (tabEl) tabEl.innerHTML = buildDetailsTab();
  }

  (window as any).__safelyAddTab(
    "intelligence",
    "Details",
    buildDetailsTab(),
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="2"/><path d="M16.24 7.76a6 6 0 010 8.48"/><path d="M19.07 4.93a10 10 0 010 14.14"/><path d="M7.76 16.24a6 6 0 010-8.48"/><path d="M4.93 19.07a10 10 0 010-14.14"/></svg>',
    null,
  );

  window.addEventListener("safely-data-ready", () => {
    ticked.clear();
    redrawDetailsTab();
  });
  window.addEventListener("safely-lang-changed", redrawDetailsTab);
  window.addEventListener("safely-result-text-changed", redrawDetailsTab);
})();
