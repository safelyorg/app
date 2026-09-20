interface DetailSeller {
  name: string | null;
  handle: string | null;
  phone: string | null;
  account_age: string | null;
  location: string | null;
  last_active: string | null;
  verification: string;
  monthly_activity: number[] | null;
  network_summary: string | null;
}

interface DetailSignal {
  type: string;
  label: string;
  value: string;
  sub: string | null;
}

interface DetailReport {
  report_type: string;
  reported_at: string;
}

interface DetailRiskFactor {
  severity: string;
  name: string;
  description: string;
  contributing_signals: string[];
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

interface DetailResponse {
  listing_title: string | null;
  listing_url: string | null;
  risk_score: number;
  risk_level: string;
  fraud_report_count: number;
  platform: string;
  seller: DetailSeller;
  signals: DetailSignal[] | null;
  risk_factors: DetailRiskFactor[] | null;
  reports: DetailReport[] | null;
  social_candidates: PlatformCheckResult[] | null;
}

const SIGNAL_LABEL_TRANSLATIONS: Record<string, string> = {
  "Domain check": "dash.label.domain_check",
  "Price analysis": "dash.label.price_analysis",
  "Urgency language": "dash.label.urgency_language",
  "Advance payment request": "dash.label.advance_payment",
  "Entity age": "dash.label.entity_age",
  "Duplicate listing": "dash.label.duplicate_listing",
  "Image authenticity": "dash.label.image_authenticity",
  "Overall legitimacy check": "dash.label.overall_legitimacy",
  "Safely history": "dash.label.safely_history",
  "Seller website check": "dash.label.seller_website_check",
  "Platform verification": "dash.label.platform_verification",
  "Contact info": "dash.label.contact_info",
  "Registration consistency": "dash.label.registration_consistency",
  "Company profile completeness": "dash.label.company_profile_completeness",
  "Listing completeness": "dash.label.listing_completeness",
  "Seller track record": "dash.label.seller_track_record",
  "Store page check": "dash.label.store_page_check",
};

function translateSignalLabel(label: string): string {
  const key = SIGNAL_LABEL_TRANSLATIONS[label];
  return key ? t(key, label) : label;
}

// Same real, known-fixed-set approach as labels above - these are
// exact-match lookups, so anything genuinely novel (an AI-written
// value we haven't seen) safely falls through to the original
// English text rather than showing blank or wrong.
const SIGNAL_VALUE_TRANSLATIONS: Record<string, string> = {
  "Verified": "dash.value.verified",
  "Not found": "dash.value.not_found",
  "None found": "dash.value.none_found",
  "Detected": "dash.value.detected",
  "Confirmed": "dash.value.confirmed",
  "Not confirmed": "dash.value.not_confirmed",
  "Suspicious": "dash.value.suspicious",
  "Unverifiable": "dash.value.unverifiable",
  "Unknown": "dash.value.unknown",
  "normal": "dash.value.normal",
  "not verified": "dash.value.not_verified",
  "Candidates found": "dash.value.candidates_found",
  "Not checked": "dash.value.not_checked",
};

function translateSignalValue(value: string): string {
  const key = SIGNAL_VALUE_TRANSLATIONS[value];
  if (key) return t(key, value);
  const ageTranslated = translateAccountAge(value);
  if (ageTranslated !== value) return ageTranslated;
  return value;
}

// Real, known-fixed set (build_network_summary in the backend only
// ever produces one of these three shapes) - a genuine regex match,
// not a free-text AI explanation, so this is safely translatable the
// same way labels/values are.
function translateNetworkSummary(summary: string | null): string {
  if (!summary) return "";
  if (summary === "Clean record on Safely network. No fraud reports found.") {
    return t("dash.network.clean", summary);
  }
  const oneMatch = summary.match(/^1 fraud report found on Safely network\. Proceed with caution\.$/);
  if (oneMatch) {
    return t("dash.network.one_report", summary);
  }
  const manyMatch = summary.match(/^(\d+) fraud reports found on Safely network\. High risk seller\.$/);
  if (manyMatch) {
    return t("dash.network.many_reports", "{n} fraud reports found on Safely network. High risk seller.").replace(
      "{n}",
      manyMatch[1],
    );
  }
  return summary;
}

// Real, fixed field-name checklist, confirmed straight from
// services/signals.rs's own hardcoded field arrays - not AI-generated,
// genuinely translatable the same way signal labels are.
const CHECKLIST_FIELD_TRANSLATIONS: Record<string, string> = {
  "Employee count": "dash.field.employee_count",
  "Sales revenue": "dash.field.sales_revenue",
  "Export percentage": "dash.field.export_percentage",
  "Unit price": "dash.field.unit_price",
  "FOB price": "dash.field.fob_price",
  "Minimum order quantity": "dash.field.moq",
  "Payment type": "dash.field.payment_type",
  "Preferred port": "dash.field.preferred_port",
  "Production capacity": "dash.field.production_capacity",
  "Delivery timeframe": "dash.field.delivery_timeframe",
  "Incoterms": "dash.field.incoterms",
  "Packaging details": "dash.field.packaging_details",
};

function translateChecklistField(name: string): string {
  const key = CHECKLIST_FIELD_TRANSLATIONS[name];
  return key ? t(key, name) : name;
}

function translateAccountAge(value: string): string {
  if (value === "Unknown") {
    return t("dash.common.unknown", "Unknown");
  }
  if (value === "This month") {
    return t("dash.age.this_month", "This month");
  }
  const monthsOnly = value.match(/^(\d+) months$/);
  if (monthsOnly) {
    return t("dash.age.months", "{m} months").replace("{m}", monthsOnly[1]);
  }
  const yearsOnly = value.match(/^(\d+) years$/);
  if (yearsOnly) {
    return t("dash.age.years", "{y} years").replace("{y}", yearsOnly[1]);
  }
  const both = value.match(/^(\d+) years (\d+) months$/);
  if (both) {
    return t("dash.age.years_months", "{y} years {m} months")
      .replace("{y}", both[1])
      .replace("{m}", both[2]);
  }
  return value;
}

// Real, fixed sentence templates confirmed straight from
// build_b2b_transparency_signal / build_b2b_listing_completeness_signal
// in services/signals.rs - not AI-generated.
function translateCompletenessSub(sub: string): string {
  const transparencyMatch = sub.match(
    /^(\d+) of 3 transparency fields \(employees, sales volume, export percentage\) are filled in\.$/,
  );
  if (transparencyMatch) {
    return t(
      "dash.completeness.transparency",
      "{n} of 3 transparency fields (employees, sales volume, export percentage) are filled in.",
    ).replace("{n}", transparencyMatch[1]);
  }

  const listingMatch = sub.match(
    /^(\d+) of (\d+) listing details \(price, MOQ, Incoterms, etc\.\) were provided by the supplier\.$/,
  );
  if (listingMatch) {
    return t(
      "dash.completeness.listing",
      "{filled} of {total} listing details (price, MOQ, Incoterms, etc.) were provided by the supplier.",
    )
      .replace("{filled}", listingMatch[1])
      .replace("{total}", listingMatch[2]);
  }

  return sub;
}

function buildRiskGauge(score: number, level: string): string {
  const color = riskHex(level);
  const r = 44;
  const circumference = 2 * Math.PI * r;
  const offset = circumference * (1 - score / 100);

  let ticks = "";
  const tickCount = 48;
  for (let i = 0; i < tickCount; i++) {
    const angle = (i * 360) / tickCount;
    const major = i % 6 === 0;
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

  return (
    '<svg viewBox="0 0 120 120" class="w-full h-full">' +
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
    "</svg>"
  );
}

async function openDetail(analysisId: string): Promise<void> {
  currentAnalysisId = analysisId;
  const panel = document.getElementById("detail-view");
  const loading = document.getElementById("detail-loading") as HTMLElement;
  const body = document.getElementById("detail-body") as HTMLElement;
  if (!panel) return;

  panel.classList.remove("hidden");
  loading.classList.remove("hidden");
  loading.textContent = t("dash.common.loading", "Loading...");
  body.classList.add("hidden");
  (document.getElementById("detail-title") as HTMLElement).textContent = "";

  try {
    const res = await fetch(API_BASE + "/history/" + analysisId, {
      headers: (window as any).safelyAuth.authHeader(),
    });
    if (res.status === 401) {
      (window as any).safelyAuth.logout();
      return;
    }
    if (!res.ok) {
      loading.textContent = t("dash.detail.load_failed", "Could not load this listing.");
      return;
    }
    const data: DetailResponse = await res.json();
    renderDetailBody(data);
    loading.classList.add("hidden");
    body.classList.remove("hidden");
  } catch (e) {
    console.error("Safely: failed to load detail", e);
    loading.textContent = t("dash.detail.load_failed", "Could not load this listing.");
  }
}

function renderDetailBody(data: DetailResponse): void {
  const tabBtn = document.querySelector(".detail-tab-btn");
  if (tabBtn && tabBtn.parentElement) {
    tabBtn.parentElement.classList.remove("max-w-md");
    tabBtn.parentElement.classList.add("w-full");
  }

  const intelTab = document.getElementById("detail-tab-content-intel");
  const reportTab = document.getElementById("detail-tab-content-report");
  if (intelTab) intelTab.classList.remove("max-w-2xl");
  if (reportTab) reportTab.classList.remove("max-w-2xl");

  (document.getElementById("detail-title") as HTMLElement).textContent =
    data.listing_title || t("dash.detail.untitled_listing", "Untitled listing");

  const linkEl = document.getElementById("detail-listing-link") as HTMLAnchorElement | null;
  if (linkEl) {
    if (data.listing_url) {
      linkEl.href = data.listing_url;
      linkEl.classList.remove("hidden");
    } else {
      linkEl.classList.add("hidden");
    }
  }

  (document.getElementById("detail-gauge-wrap") as HTMLElement).innerHTML = buildRiskGauge(
    data.risk_score,
    data.risk_level,
  );

  const levelEl = document.getElementById("detail-risk-level") as HTMLElement;
  const riskLevelLabels: Record<string, string> = {
    low: t("dash.risk.low", "Low risk"),
    caution: t("dash.risk.caution", "Caution risk"),
    high: t("dash.risk.high", "High risk"),
  };
  levelEl.textContent =
    riskLevelLabels[data.risk_level] ||
    data.risk_level.charAt(0).toUpperCase() + data.risk_level.slice(1) + " risk";
  levelEl.className = "text-[15px] font-extrabold mt-1 " + verdictTextClass(data.risk_level);

  const reportsChip = document.getElementById("detail-chip-reports") as HTMLElement;
  reportsChip.textContent = String(data.fraud_report_count || 0);
  reportsChip.className =
    "num text-lg font-bold " + (data.fraud_report_count > 0 ? "text-coral" : "text-muted");

  const statusText =
    data.seller.verification === "verified"
      ? t("dash.detail.verified", "Verified")
      : data.seller.verification === "reported"
        ? t("dash.detail.reported", "Reported")
        : t("dash.common.unknown", "Unknown");
  (document.getElementById("detail-chip-status") as HTMLElement).textContent = statusText;
  (document.getElementById("detail-chip-platform") as HTMLElement).textContent = data.platform;
  const notFound = t("dash.common.not_found", "Not found");
  (document.getElementById("detail-seller-name") as HTMLElement).textContent =
    data.seller.name || notFound;
  (document.getElementById("detail-seller-username") as HTMLElement).textContent =
    data.seller.handle || notFound;
  (document.getElementById("detail-seller-phone") as HTMLElement).textContent =
    data.seller.phone || notFound;
  (document.getElementById("detail-seller-age") as HTMLElement).textContent =
    data.seller.account_age ? translateAccountAge(data.seller.account_age) : notFound;
  (document.getElementById("detail-seller-location") as HTMLElement).textContent =
    data.seller.location || notFound;
  (document.getElementById("detail-seller-lastactive") as HTMLElement).textContent =
    data.seller.last_active || notFound;

  const chart = document.getElementById("detail-activity-chart") as HTMLElement;
  const activity = data.seller.monthly_activity || new Array(12).fill(0);
  const max = Math.max.apply(null, activity) || 1;
  chart.innerHTML = activity
    .map((v: number) => {
      const heightPx = Math.max(3, Math.round((v / max) * 44));
      return (
        '<div class="flex-1 flex flex-col items-center justify-end relative">' +
        '<span class="text-[9px] text-ink font-bold num absolute top-0">' +
        v +
        "</span>" +
        '<div class="w-full bg-brand/70 rounded-t" style="height:' +
        heightPx +
        'px"></div></div>'
      );
    })
    .join("");

  (document.getElementById("detail-network-summary") as HTMLElement).textContent =
    translateNetworkSummary(data.seller.network_summary) || "";

  const signals = data.signals || [];
  const badCount = signals.filter((s) => s.type !== "good" && s.type !== "info").length;
  const summaryEl = document.getElementById("detail-intel-summary") as HTMLElement;

  if (badCount === 0) {
    summaryEl.className = "flex gap-2.5 p-3.5 rounded-xl text-[12px] mb-5 bg-mint/10 text-mint";
    summaryEl.innerHTML =
      "<span>&#9679;</span><span>" +
      t("dash.detail.all_signals_checked", "All {n} signals checked. No red flags detected.").replace(
        "{n}",
        String(signals.length),
      ) +
      "</span>";
  } else {
    summaryEl.className = "flex gap-2.5 p-3.5 rounded-xl text-[12px] mb-5 bg-amber/10 text-amber";
    summaryEl.innerHTML =
      "<span>&#9679;</span><span>" +
      t("dash.detail.signals_need_attention", "{bad} of {total} signals need your attention.")
        .replace("{bad}", String(badCount))
        .replace("{total}", String(signals.length)) +
      "</span>";
  }

  const priceSignal = signals.find((s) => s.label === "Price analysis");
  const priceSection = document.getElementById("detail-price-vs-market");
  if (priceSection) {
    if (priceSignal) {
      const verdict = priceSignal.value || "unknown";
      const verdictClass = verdict === "normal" ? "text-mint" : "text-amber";
      priceSection.classList.remove("hidden");
      priceSection.innerHTML =
        '<div class="text-[10px] font-extrabold uppercase tracking-wider text-muted mb-1.5">' +
        t("dash.detail.price_vs_market", "Price vs market") +
        "</div>" +
        '<div class="bg-surface border border-line rounded-xl p-4">' +
        '<div class="text-[14px] font-bold ' +
        verdictClass +
        '">' +
        escapeHtml(translateSignalValue(verdict.charAt(0).toUpperCase() + verdict.slice(1))) +
        "</div>" +
        '<div class="text-[12px] text-muted mt-1.5">' +
        (priceSignal.sub || "") +
        "</div></div>";
    } else {
      priceSection.classList.add("hidden");
    }
  }

  const signalsList = document.getElementById("detail-signals-list") as HTMLElement;
  signalsList.innerHTML = signals
    .map((s, idx) => {
      const { realSub, checklist } = parseChecklistSignal(s.sub);
      const dropdownId = "detail-checklist-" + idx;
      return (
        '<div class="bg-surface border border-line rounded-xl p-4 mb-2.5 last:mb-0">' +
        '<div class="flex justify-between items-baseline gap-3">' +
        '<div class="font-semibold text-[13px]">' +
        escapeHtml(translateSignalLabel(s.label)) +
        "</div>" +
        '<div class="text-[13px] font-bold whitespace-nowrap ' +
        signalTextClass(s.type) +
        '">' +
        escapeHtml(translateSignalValue(s.value)) +
        "</div></div>" +
        '<div class="text-[12px] text-muted mt-1.5">' +
        (realSub ? escapeHtml(translateCompletenessSub(realSub)) : "") +
        "</div>" +
        buildChecklistDropdown(dropdownId, checklist) +
        "</div>"
      );
    })
    .join("");

  signals.forEach((_, idx) => attachChecklistListener("detail-checklist-" + idx));

  const socialPresenceEl = document.getElementById("detail-social-presence");
  if (socialPresenceEl) {
    socialPresenceEl.innerHTML = buildSocialPresenceSection(data.social_candidates || []);
  }

  const riskFactorsSection = document.getElementById("detail-risk-factors");
  const riskFactors = data.risk_factors || [];
  if (riskFactorsSection) {
    if (riskFactors.length > 0) {
      const severityColors: Record<string, string> = {
        hard: "text-coral",
        compound: "text-amber",
        soft: "text-muted",
      };
      const severityLabels: Record<string, string> = {
        hard: t("dash.detail.severity_confirmed", "Confirmed"),
        compound: t("dash.detail.severity_pattern_match", "Pattern match"),
        soft: t("dash.detail.severity_worth_noting", "Worth noting"),
      };
      const capitalizeFirst = (str: string): string =>
        str ? str.charAt(0).toUpperCase() + str.slice(1) : str;

      // Real, fixed set confirmed from derive_risk_factors() in
      // services/risk_factors.rs - "soft" factors (name ends in
      // "_flagged") reuse the originating signal's own sub text and
      // are NOT translated here, since that text may itself be
      // Claude-generated (e.g. Contact info's evidence).
      const RISK_FACTOR_NAMES: Record<string, string> = {
        confirmed_legitimacy_concern: t("dash.rf.name.legitimacy_concern", "Confirmed legitimacy concern"),
        network_confirmed_high_risk_seller: t("dash.rf.name.high_risk_seller", "Network confirmed high risk seller"),
        likely_counterfeit_or_nonexistent_product: t("dash.rf.name.counterfeit", "Likely counterfeit or nonexistent product"),
        advance_fee_scam_pattern: t("dash.rf.name.advance_fee", "Advance fee scam pattern"),
        newly_created_high_risk_entity: t("dash.rf.name.newly_created", "Newly created high risk entity"),
      };
      const RISK_FACTOR_DESCRIPTIONS: Record<string, string> = {
        "A specific, known fraud pattern or legitimacy concern was identified in this listing.":
          t("dash.rf.desc.legitimacy_concern", "A specific, known fraud pattern or legitimacy concern was identified in this listing."),
        "Safely's own network has previously scored this seller as high-risk.":
          t("dash.rf.desc.high_risk_seller", "Safely's own network has previously scored this seller as high-risk."),
        "A templated, duplicate-style listing combined with unverifiable images suggests the product itself may not genuinely exist or be authentic.":
          t("dash.rf.desc.counterfeit", "A templated, duplicate-style listing combined with unverifiable images suggests the product itself may not genuinely exist or be authentic."),
        "This listing combines pressure/urgency language with a request for payment before delivery - a classic advance-fee scam pattern.":
          t("dash.rf.desc.advance_fee", "This listing combines pressure/urgency language with a request for payment before delivery - a classic advance-fee scam pattern."),
        "A very recently established entity combined with a matched legitimacy concern is a strong, well-known combination seen in scam listings.":
          t("dash.rf.desc.newly_created", "A very recently established entity combined with a matched legitimacy concern is a strong, well-known combination seen in scam listings."),
      };

      function translateRiskFactorName(name: string): string {
        if (RISK_FACTOR_NAMES[name]) return RISK_FACTOR_NAMES[name];

        if (name.endsWith("_flagged")) {
          const rawLabel = name.slice(0, -"_flagged".length).replace(/_/g, " ");
          const matchedEnglishLabel = Object.keys(SIGNAL_LABEL_TRANSLATIONS).find(
            (label) => label.toLowerCase() === rawLabel,
          );
          const translatedLabel = matchedEnglishLabel
            ? translateSignalLabel(matchedEnglishLabel)
            : capitalizeFirst(rawLabel);
          return translatedLabel + " " + t("dash.rf.flagged_suffix", "flagged");
        }

        return capitalizeFirst(name.replace(/_/g, " "));
      }

      riskFactorsSection.classList.remove("hidden");
      riskFactorsSection.innerHTML =
        '<div class="text-[10px] font-extrabold uppercase tracking-wider text-muted mb-2">' +
        t("dash.detail.risk_factors", "Risk factors") +
        "</div>" +
        riskFactors
          .map((f) => {
            const colorClass = severityColors[f.severity] || "text-muted";
            const severityLabel = severityLabels[f.severity] || f.severity;
            const displayName = translateRiskFactorName(f.name);
            const displayDescription = RISK_FACTOR_DESCRIPTIONS[f.description] || f.description;
            return (
              '<div class="bg-surface border border-line rounded-xl p-4 mb-2.5 last:mb-0">' +
              '<div class="flex justify-between items-baseline gap-3">' +
              '<div class="font-semibold text-[13px]">' +
              escapeHtml(displayName) +
              "</div>" +
              '<div class="text-[13px] font-bold whitespace-nowrap ' +
              colorClass +
              '">' +
              severityLabel +
              "</div></div>" +
              '<div class="text-[12px] text-muted mt-1.5">' +
              escapeHtml(displayDescription) +
              "</div></div>"
            );
          })
          .join("");
    } else {
      riskFactorsSection.classList.add("hidden");
      riskFactorsSection.innerHTML = "";
    }
  }

  const filedBlock = document.getElementById("detail-report-filed") as HTMLElement;
  const emptyBlock = document.getElementById("detail-report-empty") as HTMLElement;
  const reports = data.reports || [];

  if (reports.length > 0) {
    filedBlock.classList.remove("hidden");
    emptyBlock.classList.add("hidden");
    filedBlock.innerHTML = reports
      .map(
        (r) =>
          '<div class="bg-surface border border-line rounded-xl p-4 mb-2.5 last:mb-0">' +
          '<div class="text-[10px] font-extrabold uppercase tracking-wider text-muted mb-1.5">Report reason</div>' +
          '<div class="text-[14px] font-semibold">' +
          r.report_type +
          "</div>" +
          '<div class="text-[12px] text-muted mt-2">' +
          t("dash.detail.submitted", "Submitted") +
          " " +
          formatDate(r.reported_at) +
          "</div>" +
          "</div>",
      )
      .join("");
  } else {
    filedBlock.classList.add("hidden");
    filedBlock.innerHTML = "";
    emptyBlock.classList.remove("hidden");
  }

  switchDetailTab("risk");
}

let currentAnalysisId: string | null = null;

async function downloadEvidencePdf(): Promise<void> {
  if (!currentAnalysisId) return;
  const btn = document.getElementById("detail-download-pdf") as HTMLButtonElement | null;
  if (btn) {
    btn.disabled = true;
    btn.style.opacity = "0.5";
  }
  try {
    const res = await fetch(API_BASE + "/history/" + currentAnalysisId + "/pdf", {
      headers: (window as any).safelyAuth.authHeader(),
    });
    if (!res.ok) {
      console.error("Safely: failed to download the real PDF report");
      return;
    }
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = "safely-evidence-" + currentAnalysisId + ".pdf";
    document.body.appendChild(a);
    a.click();
    a.remove();
    URL.revokeObjectURL(url);
  } catch (e) {
    console.error("Safely: PDF download failed", e);
  } finally {
    if (btn) {
      btn.disabled = false;
      btn.style.opacity = "1";
    }
  }
}

document.getElementById("detail-download-pdf")?.addEventListener("click", downloadEvidencePdf);

const PLATFORM_ORDER = [
  "Facebook", "LinkedIn", "TikTok", "Instagram", "Reddit", "Trustpilot",
  "Reviews", "Contact (Facebook)", "Contact (LinkedIn)", "Contact (Web)",
];

function buildSocialPresenceSection(results: PlatformCheckResult[]): string {
  if (!results || results.length === 0) return "";

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
    Object.keys(grouped).filter((p) => !PLATFORM_ORDER.includes(p)),
  );

  const groupsHTML = order
    .map((platform, index) => {
      const links = grouped[platform];
      const isLast = index === order.length - 1;
      const body =
        links.length === 0
          ? '<div class="text-[12px] text-muted py-1">' + t("dash.common.not_found", "Not found") + "</div>"
          : links
              .map(
                (link) =>
                  '<div class="flex items-start gap-2 py-1.5">' +
                  '<span class="text-muted flex-shrink-0 mt-0.5">&#8226;</span>' +
                  '<a href="' +
                  escapeHtml(link.url) +
                  '" target="_blank" rel="noopener noreferrer" class="text-[12px] text-ink hover:text-brand flex-1 min-w-0 no-underline">' +
                  escapeHtml(link.title) +
                  "</a></div>",
              )
              .join("");
      return (
        '<div class="py-2.5' +
        (isLast ? "" : " border-b border-line") +
        '">' +
        '<div class="text-[10px] font-extrabold uppercase tracking-wider text-muted mb-1.5">' +
        escapeHtml(platform) +
        "</div>" +
        body +
        "</div>"
      );
    })
    .join("");

  return (
    '<div class="text-[10px] font-extrabold uppercase tracking-wider text-muted mb-2 mt-5">' +
    t("dash.detail.social_presence", "Social presence check") +
    "</div>" +
    '<div class="bg-surface border border-line rounded-xl p-4 mb-5">' +
    groupsHTML +
    "</div>"
  );
}

function escapeHtml(str: string): string {
  const div = document.createElement("div");
  div.textContent = str;
  return div.innerHTML;
}

function parseChecklistSignal(sub: string | null): { realSub: string; checklist: [string, boolean][] } {
  if (!sub) return { realSub: "", checklist: [] };
  const marker = "###CHECKLIST###";
  const idx = sub.indexOf(marker);
  if (idx === -1) return { realSub: sub, checklist: [] };
  const realSub = sub.slice(0, idx);
  const checklistRaw = sub.slice(idx + marker.length);
  const checklist: [string, boolean][] = checklistRaw
    .split(";")
    .filter(Boolean)
    .map((entry) => {
      const [name, present] = entry.split("|");
      return [name, present === "true"];
    });
  return { realSub, checklist };
}

function buildChecklistDropdown(id: string, checklist: [string, boolean][]): string {
  if (checklist.length === 0) return "";
  const rows = checklist
    .map(([name, present]) => {
      const icon = present
        ? '<span class="text-mint flex-shrink-0">&#10003;</span>'
        : '<span class="text-muted flex-shrink-0">&#10005;</span>';
      return (
        '<div class="flex items-center gap-2 py-1">' +
        icon +
        '<span class="text-[12px] text-ink">' +
        escapeHtml(translateChecklistField(name)) +
        "</span></div>"
      );
    })
    .join("");
  return (
    '<button id="' +
    id +
    '-toggle" type="button" class="w-full text-left bg-transparent border-0 cursor-pointer p-0 pt-2 text-[11px] font-semibold text-muted flex justify-between items-center">' +
    '<span id="' +
    id +
    '-toggle-text">' + t("dash.detail.click_see_checks", "Click to see checks") + '</span><span id="' +
    id +
    '-arrow">&#9662;</span>' +
    "</button>" +
    '<div id="' +
    id +
    '-dropdown" class="hidden mt-1.5">' +
    rows +
    "</div>"
  );
}

function attachChecklistListener(id: string): void {
  const toggle = document.getElementById(id + "-toggle");
  const dropdown = document.getElementById(id + "-dropdown");
  const arrow = document.getElementById(id + "-arrow");
  const toggleText = document.getElementById(id + "-toggle-text");
  if (toggle && dropdown && arrow && toggleText) {
    toggle.addEventListener("click", () => {
      const isOpen = !dropdown.classList.contains("hidden");
      dropdown.classList.toggle("hidden", isOpen);
      arrow.innerHTML = isOpen ? "&#9662;" : "&#9652;";
      toggleText.textContent = isOpen
        ? t("dash.detail.click_see_checks", "Click to see checks")
        : t("dash.detail.click_hide_checks", "Click to hide checks");
    });
  }
}

function switchDetailTab(tab: string): void {
  ["risk", "intel", "report"].forEach((name) => {
    const content = document.getElementById("detail-tab-content-" + name);
    if (content) content.classList.toggle("hidden", name !== tab);
  });
  document.querySelectorAll<HTMLElement>(".detail-tab-btn").forEach((btn) => {
    const active = btn.dataset.detailTab === tab;
    btn.classList.toggle("bg-surface3", active);
    btn.classList.toggle("text-ink", active);
  });
}

function closeDetailPanel(): void {
  const panel = document.getElementById("detail-view");
  if (panel) panel.classList.add("hidden");
}
