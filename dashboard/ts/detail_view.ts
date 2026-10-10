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

// The Portuguese words here are the same as the extension's
// (extension/ts/core/i18n.ts), so a result reads the same in both -
// change both together.
//
// Only fixed words are translated here: check names, results and
// field names. The reason under each check and the risk factor
// explanations come already translated from the backend (the
// ?language= on the detail request).

const SIGNAL_LABEL_TRANSLATIONS: Record<string, string> = {
  "Domain check": "dash.label.domain_check",
  "Price analysis": "dash.label.price_analysis",
  "Urgency language": "dash.label.urgency_language",
  "Advance payment request": "dash.label.advance_payment",
  "Account age": "dash.label.account_age",
  "Duplicate listing": "dash.label.duplicate_listing",
  "Listing detail": "dash.label.listing_detail",
  "Regulated product": "dash.label.regulated_product",
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
  "Company details": "dash.label.company_details",
  "Product range": "dash.label.product_range",
  Certificates: "dash.label.certificates",
  "Social presence check": "dash.detail.social_presence",
  // Title of the risk factor shown when Safely could check too little.
  "Not enough information": "dash.label.not_enough_information",
};

function translateSignalLabel(label: string): string {
  const key = SIGNAL_LABEL_TRANSLATIONS[label];
  return key ? t(key, label) : label;
}

// Exact-match lookups, so anything genuinely novel (an AI-written
// value we haven't seen) safely falls through to the original English
// text rather than showing blank or wrong.
const SIGNAL_VALUE_TRANSLATIONS: Record<string, string> = {
  Verified: "dash.value.verified",
  Unverified: "dash.value.unverified",
  "Not verified": "dash.value.not_verified",
  "not verified": "dash.value.not_verified",
  "Not found": "dash.value.not_found",
  "None found": "dash.value.none_found",
  Detected: "dash.value.detected",
  Confirmed: "dash.value.confirmed",
  "Not confirmed": "dash.value.not_confirmed",
  Suspicious: "dash.value.suspicious",
  Unverifiable: "dash.value.unverifiable",
  Unknown: "dash.value.unknown",
  unknown: "dash.value.unknown",
  Normal: "dash.value.normal",
  normal: "dash.value.normal",
  Original: "dash.value.original",
  original: "dash.value.original",
  "Candidates found": "dash.value.candidates_found",
  "Scam mentions found": "dash.value.scam_mentions_found",
  "No presence found": "dash.value.no_presence_found",
  "Checked by Alibaba": "dash.value.checked_by_alibaba",
  "Paid membership": "dash.value.paid_membership",
  "No recent orders": "dash.value.no_recent_orders",
  "Risky option listed": "dash.value.risky_option_listed",
  "Commodity scam pattern": "dash.value.commodity_scam_pattern",
  "Often used in scams": "dash.value.often_used_in_scams",
  "Look copied": "dash.value.look_copied",
  "Out of date": "dash.value.out_of_date",
  "Not listed": "dash.value.not_listed",
  "Not checked": "dash.value.not_checked",
  Unregistered: "dash.value.unregistered",
  Registered: "dash.value.registered",
  "Fully confirmed": "dash.value.fully_confirmed",
  "Name confirmed only": "dash.value.name_confirmed_only",
  Unconfirmed: "dash.value.unconfirmed",
  "Not offered": "dash.value.not_offered",
  "No badge": "dash.value.no_badge",
  "Not provided": "dash.value.not_provided",
  "Invalid date": "dash.value.invalid_date",
  "Not shown on this platform": "dash.value.not_shown",
  Specific: "dash.value.specific",
  Vague: "dash.value.vague",
  "Licence needed": "dash.value.licence_needed",
  "Licence needed, not the maker": "dash.value.licence_needed_not_maker",
  "Untraceable payment": "dash.value.untraceable_payment",
  "Full prepayment": "dash.value.full_prepayment",
  "Website found": "dash.value.website_found",
  "No website found": "dash.value.no_website_found",
  "Possible website found": "dash.value.possible_website_found",
  "No store page found": "dash.value.no_store_page_found",
  "Couldn't be loaded": "dash.value.couldnt_be_loaded",
  "New to Safely": "dash.value.new_to_safely",
  "Checked once before": "dash.value.checked_once",
};

// Results that carry a number ("3/9 fields provided", "Checked 4 times
// before", "Gold member").
function translatePatternValue(value: string): string {
  const fields = value.match(/^(\d+)\/(\d+) fields provided$/);
  if (fields) {
    return t("dash.value.fields_provided", "{filled}/{total} fields provided")
      .replace("{filled}", fields[1])
      .replace("{total}", fields[2]);
  }
  const checked = value.match(/^Checked (\d+) times before$/);
  if (checked) {
    return t("dash.value.checked_times", "Checked {n} times before").replace("{n}", checked[1]);
  }
  const prior = value.match(/^(\d+) prior checks?$/);
  if (prior) {
    return t("dash.value.prior_checks", "{n} prior checks").replace("{n}", prior[1]);
  }
  const rating = value.match(/^([\d.]+) rating, (\d+) listings$/);
  if (rating) {
    return t("dash.value.rating_listings", "{rating} rating, {count} listings")
      .replace("{rating}", rating[1])
      .replace("{count}", rating[2]);
  }
  const ordersRating = value.match(/^(\d+) orders?, ([\d.]+) rating$/);
  if (ordersRating) {
    return t("dash.value.orders_rating", "{n} orders, {rating} rating")
      .replace("{n}", ordersRating[1])
      .replace("{rating}", ordersRating[2]);
  }
  const orders = value.match(/^(\d+) orders?$/);
  if (orders) {
    return t("dash.value.orders", "{n} orders").replace("{n}", orders[1]);
  }
  const member = value.match(/^(.+) member$/);
  if (member) {
    return t("dash.value.member", "{tier} member").replace("{tier}", member[1]);
  }
  return value;
}

function translateSignalValue(value: string): string {
  const key = SIGNAL_VALUE_TRANSLATIONS[value];
  if (key) return t(key, value);
  const ageTranslated = translateAccountAge(value);
  if (ageTranslated !== value) return ageTranslated;
  return translatePatternValue(value);
}

// The seller's fraud-report line, written from the report count, so it
// reads exactly the same as in the extension. In English the server's
// own sentence is kept.
function networkSummaryText(count: number, serverText: string | null): string {
  if (count <= 0) {
    return t(
      "dash.network.clean",
      serverText || "Clean record on Safely network. No fraud reports found.",
    );
  }
  if (count === 1) {
    return t(
      "dash.network.one_report",
      serverText || "1 fraud report found on Safely network. Proceed with caution.",
    );
  }
  return t(
    "dash.network.many_reports",
    serverText || "{n} fraud reports found on Safely network. High risk seller.",
  ).replace("{n}", String(count));
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
  Incoterms: "dash.field.incoterms",
  "Packaging details": "dash.field.packaging_details",
};

function translateChecklistField(name: string): string {
  const key = CHECKLIST_FIELD_TRANSLATIONS[name];
  return key ? t(key, name) : name;
}

function ageYears(y: string): string {
  return y === "1"
    ? t("dash.age.one_year", "1 year")
    : t("dash.age.years", "{y} years").replace("{y}", y);
}

function ageMonths(m: string): string {
  return m === "1"
    ? t("dash.age.one_month", "1 month")
    : t("dash.age.months", "{m} months").replace("{m}", m);
}

// "About 11 years", "2 years 3 months", "This month"... Anything else
// is returned unchanged.
function translateAccountAge(value: string): string {
  if (value === "Unknown") {
    return t("dash.common.unknown", "Unknown");
  }
  if (value === "This month") {
    return t("dash.age.this_month", "This month");
  }
  if (value === "Founded this year") {
    return t("dash.age.founded_this_year", "Founded this year");
  }
  const about = value.match(/^About (\d+) years?$/);
  if (about) {
    return about[1] === "1"
      ? t("dash.age.about_one_year", "About 1 year")
      : t("dash.age.about_years", "About {y} years").replace("{y}", about[1]);
  }
  const both = value.match(/^(\d+) years? (?:and )?(\d+) months?$/);
  if (both) {
    return ageYears(both[1]) + t("dash.age.joiner", " ") + ageMonths(both[2]);
  }
  const yearsOnly = value.match(/^(\d+) years?$/);
  if (yearsOnly) return ageYears(yearsOnly[1]);
  const monthsOnly = value.match(/^(\d+) months?$/);
  if (monthsOnly) return ageMonths(monthsOnly[1]);
  return value;
}

const REPORT_REASON_TRANSLATIONS: Record<string, [string, string]> = {
  scam: ["dash.report.scam", "Scam"],
  fake_item: ["dash.report.fake_item", "Fake item"],
  no_delivery: ["dash.report.no_delivery", "No delivery"],
  wrong_item: ["dash.report.wrong_item", "Wrong item"],
  non_responsive: ["dash.report.non_responsive", "Non responsive"],
};

function translateReportReason(reason: string): string {
  const entry = REPORT_REASON_TRANSLATIONS[reason];
  return entry ? t(entry[0], entry[1]) : reason;
}

const SOCIAL_PLATFORM_TRANSLATIONS: Record<string, string> = {
  Reviews: "dash.social.reviews",
  "Contact (Facebook)": "dash.social.contact_facebook",
  "Contact (LinkedIn)": "dash.social.contact_linkedin",
  "Contact (Web)": "dash.social.contact_web",
};

function translateSocialPlatform(platform: string): string {
  const key = SOCIAL_PLATFORM_TRANSLATIONS[platform];
  return key ? t(key, platform) : platform;
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
    const lang = localStorage.getItem("safely_lang") || "en";
    const res = await fetch(
      API_BASE + "/history/" + analysisId + "?language=" + encodeURIComponent(lang),
      {
        headers: (window as any).safelyAuth.authHeader(),
      },
    );
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

// ---- Signal mix ring ---------------------------------------------
// Same ring as the extension's Intelligence tab (intelligence.ts) -
// change both together. At rest it is one colour: the kind of result
// most cards have. Hovering it splits it into one coloured part per
// kind (good / info / warning / red flag); hovering a part shows its
// share in the middle and highlights its cards in the list below.
// Leaving the ring brings back the dominant colour.

type MixType = "good" | "info" | "caution" | "bad";

// Worst first: on a tie, the more serious kind is the dominant one.
const MIX_ORDER: MixType[] = ["bad", "caution", "info", "good"];

const MIX_COLORS: Record<MixType, string> = {
  good: "#35d0a6",
  info: "#6fb3ef",
  caution: "#f2b84c",
  bad: "#ff5d5d",
};

function mixName(type: MixType): string {
  const names: Record<MixType, [string, string]> = {
    good: ["dash.mix.good", "Good"],
    info: ["dash.mix.info", "Info"],
    caution: ["dash.mix.warning", "Warning"],
    bad: ["dash.mix.red_flag", "Red flag"],
  };
  return t(names[type][0], names[type][1]);
}

function mixType(type: string): MixType {
  return type === "good" || type === "info" || type === "caution" ? type : "bad";
}

interface MixPart {
  type: MixType;
  count: number;
  pct: number;
}

// Whole percentages that always add up to 100 (largest remainder).
function mixParts(signals: DetailSignal[]): MixPart[] {
  const total = signals.length;
  if (total === 0) return [];
  const counts: Record<MixType, number> = { good: 0, info: 0, caution: 0, bad: 0 };
  signals.forEach((s) => (counts[mixType(s.type)] += 1));
  const parts = (["good", "info", "caution", "bad"] as MixType[])
    .filter((k) => counts[k] > 0)
    .map((k) => {
      const exact = (counts[k] * 100) / total;
      return { type: k, count: counts[k], pct: Math.floor(exact), rest: exact % 1 };
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

function dominantPart(parts: MixPart[]): MixPart {
  return parts.reduce((best, p) =>
    p.count > best.count ||
    (p.count === best.count && MIX_ORDER.indexOf(p.type) < MIX_ORDER.indexOf(best.type))
      ? p
      : best,
  );
}

// Labels only B2B (supplier) scans have - the info points apply to
// B2B scores only.
const MIX_B2B_ONLY_LABELS = [
  "Company profile completeness",
  "Listing completeness",
  "Registration consistency",
];

const MIX_STYLE =
  "<style>" +
  ".smx{display:flex;align-items:center;gap:20px;padding:16px;margin-bottom:12px;border-radius:14px;border:1px solid rgba(127,127,127,0.2);}" +
  ".smx-ring{width:132px;height:132px;flex-shrink:0;cursor:pointer;-webkit-tap-highlight-color:transparent;}" +
  ".smx-ring svg{width:100%;height:100%;overflow:visible;}" +
  ".smx-seg{fill:none;stroke-width:12;stroke:var(--dom);stroke-dasharray:var(--whole);transition:stroke .45s ease,stroke-dasharray .45s ease,stroke-width .2s ease,opacity .2s ease;}" +
  ".smx-open .smx-seg{stroke:var(--c);stroke-dasharray:var(--split);}" +
  ".smx-open .smx-seg.smx-dim{opacity:.35;}" +
  ".smx-open .smx-seg.smx-hot{stroke-width:16;}" +
  ".smx-glow{fill:none;stroke:var(--dom);stroke-width:22;opacity:.10;transition:opacity .45s ease,stroke .45s ease;}" +
  ".smx-open .smx-glow{opacity:0;}" +
  ".smx-pct{font-family:'JetBrains Mono',monospace;font-weight:700;font-size:24px;transition:fill .3s ease;}" +
  ".smx-name{font-weight:700;font-size:9px;letter-spacing:.6px;text-transform:uppercase;transition:fill .3s ease;}" +
  ".smx-count{font-size:8px;fill:#8a8a93;}" +
  ".smx-side{flex:1;min-width:0;}" +
  ".smx-title{font-size:10px;font-weight:800;color:#8a8a93;text-transform:uppercase;letter-spacing:.08em;}" +
  ".smx-lead{font-size:15px;font-weight:800;margin:3px 0 8px;}" +
  ".smx-row{display:flex;align-items:center;gap:8px;padding:5px 8px;margin:0 -8px;border-radius:8px;font-size:12px;cursor:default;transition:background .2s ease,opacity .2s ease;}" +
  ".smx-row.smx-hot{background:rgba(127,127,127,0.12);}" +
  ".smx-row.smx-dim{opacity:.45;}" +
  ".smx-dot{width:8px;height:8px;border-radius:50%;flex-shrink:0;}" +
  ".smx-row b{margin-left:auto;font-family:'JetBrains Mono',monospace;font-size:11px;}" +
  ".smx-hint{font-size:11px;color:#8a8a93;margin-top:8px;line-height:1.4;}" +
  "[data-smx-type]{transition:opacity .2s ease;}" +
  "</style>";

function buildSignalMix(signals: DetailSignal[]): string {
  const parts = mixParts(signals);
  if (parts.length === 0) return "";
  const dom = dominantPart(parts);
  const r = 46;
  const c = 2 * Math.PI * r;
  // Gap between parts when split; none when there is only one part.
  const gap = parts.length > 1 ? 3 : 0;
  let start = 0;
  const segs = parts
    .map((p) => {
      const len = (p.count / signals.length) * c;
      // At rest every part runs a hair past its end, so the ring reads
      // as one solid colour with no seams.
      const whole = Math.min(len + 1, c) + " " + c;
      const split = Math.max(len - gap, 0.5) + " " + c;
      const seg =
        '<circle class="smx-seg" data-smx="' +
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
        escapeHtml(mixName(p.type) + ": " + p.pct + "%") +
        "</title></circle>";
      start += len;
      return seg;
    })
    .join("");

  const rows = parts
    .map(
      (p) =>
        '<div class="smx-row" data-smx="' +
        p.type +
        '"><span class="smx-dot" style="background:' +
        MIX_COLORS[p.type] +
        '"></span><span>' +
        escapeHtml(mixName(p.type)) +
        " · " +
        p.count +
        '</span><b style="color:' +
        MIX_COLORS[p.type] +
        '">' +
        p.pct +
        "%</b></div>",
    )
    .join("");

  const isB2b = signals.some((s) => MIX_B2B_ONLY_LABELS.includes(s.label));
  const hint =
    isB2b && parts.some((p) => p.type === "info")
      ? t(
          "dash.mix.info_points",
          "Info cards are not warnings, but each adds 5 points to the score (15 at most).",
        )
      : t("dash.mix.hint", "Hover the ring to see each part.");

  return (
    MIX_STYLE +
    '<div class="smx" id="detail-smx" style="--dom:' +
    MIX_COLORS[dom.type] +
    '">' +
    '<div class="smx-ring" id="detail-smx-ring" role="img" aria-label="' +
    escapeHtml(t("dash.mix.title", "Signal mix")) +
    '"><svg viewBox="0 0 120 120">' +
    '<circle class="smx-glow" cx="60" cy="60" r="' +
    r +
    '"/>' +
    '<circle cx="60" cy="60" r="' +
    r +
    '" fill="none" stroke="rgba(127,127,127,0.18)" stroke-width="12"/>' +
    segs +
    '<text id="detail-smx-pct" class="smx-pct" x="60" y="62" text-anchor="middle"></text>' +
    '<text id="detail-smx-name" class="smx-name" x="60" y="76" text-anchor="middle"></text>' +
    '<text id="detail-smx-count" class="smx-count" x="60" y="87" text-anchor="middle"></text>' +
    "</svg></div>" +
    '<div class="smx-side">' +
    '<div class="smx-title">' +
    escapeHtml(t("dash.mix.title", "Signal mix")) +
    "</div>" +
    '<div class="smx-lead" style="color:' +
    MIX_COLORS[dom.type] +
    '">' +
    escapeHtml(t("dash.mix.mostly", "Mostly: {type}").replace("{type}", mixName(dom.type))) +
    "</div>" +
    rows +
    '<div class="smx-hint">' +
    escapeHtml(hint) +
    "</div></div></div>"
  );
}

function attachSignalMixListeners(signals: DetailSignal[]): void {
  const box = document.getElementById("detail-smx");
  const ring = document.getElementById("detail-smx-ring");
  const pctEl = document.getElementById("detail-smx-pct");
  const nameEl = document.getElementById("detail-smx-name");
  const countEl = document.getElementById("detail-smx-count");
  if (!box || !ring || !pctEl || !nameEl || !countEl) return;

  const parts = mixParts(signals);
  if (parts.length === 0) return;
  const dom = dominantPart(parts);
  const list = document.getElementById("detail-signals-list") || document;

  function show(p: MixPart): void {
    pctEl!.textContent = p.pct + "%";
    pctEl!.setAttribute("fill", MIX_COLORS[p.type]);
    nameEl!.textContent = mixName(p.type);
    nameEl!.setAttribute("fill", MIX_COLORS[p.type]);
    countEl!.textContent = t("dash.mix.count", "{n} of {total} cards")
      .replace("{n}", String(p.count))
      .replace("{total}", String(signals.length));
  }

  // Highlights one kind everywhere: its ring part, its legend row and
  // its cards in the list. null clears the highlight.
  function focus(type: MixType | null): void {
    box!.querySelectorAll<Element>("[data-smx]").forEach((el) => {
      const mine = el.getAttribute("data-smx") === type;
      el.classList.toggle("smx-hot", type !== null && mine);
      el.classList.toggle("smx-dim", type !== null && !mine);
    });
    list.querySelectorAll<HTMLElement>("[data-smx-type]").forEach((card) => {
      card.style.opacity =
        type === null || card.getAttribute("data-smx-type") === type ? "" : "0.35";
    });
    const part = parts.find((p) => p.type === type);
    show(part || dom);
  }

  function open(): void {
    box!.classList.add("smx-open");
  }

  function close(): void {
    box!.classList.remove("smx-open");
    focus(null);
  }

  show(dom);

  ring.addEventListener("mouseenter", open);
  ring.addEventListener("mouseleave", close);
  box.querySelectorAll<Element>("[data-smx]").forEach((el) => {
    const type = el.getAttribute("data-smx") as MixType;
    el.addEventListener("mouseenter", () => {
      open();
      focus(type);
    });
    el.addEventListener("mouseleave", () => {
      if (el.classList.contains("smx-row")) close();
      else focus(null);
    });
  });
  // Touch screens have no hover: a tap opens the ring and picks the
  // tapped part, a tap on the middle closes it again.
  ring.addEventListener("click", (e) => {
    const target = (e.target as Element).closest("[data-smx]");
    if (target) {
      open();
      focus(target.getAttribute("data-smx") as MixType);
    } else if (box.classList.contains("smx-open")) {
      close();
    } else {
      open();
    }
  });
}

// The ring sits right above the signal list. Its box is made here the
// first time, so index.html needs no change.
function renderSignalMix(signals: DetailSignal[]): void {
  const list = document.getElementById("detail-signals-list");
  if (!list || !list.parentElement) return;
  let mix = document.getElementById("detail-signal-mix");
  if (!mix) {
    mix = document.createElement("div");
    mix.id = "detail-signal-mix";
    list.parentElement.insertBefore(mix, list);
  }
  mix.innerHTML = buildSignalMix(signals);
  attachSignalMixListeners(signals);
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
    networkSummaryText(data.fraud_report_count || 0, data.seller.network_summary);

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
        escapeHtml(parseChecklistSignal(priceSignal.sub).realSub) +
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
        '<div class="bg-surface border border-line rounded-xl p-4 mb-2.5 last:mb-0" data-smx-type="' +
        mixType(s.type) +
        '">' +
        '<div class="flex justify-between items-baseline gap-3">' +
        '<div class="font-semibold text-[13px]">' +
        escapeHtml(translateSignalLabel(s.label)) +
        "</div>" +
        '<div class="text-[13px] font-bold whitespace-nowrap" style="color:' +
        MIX_COLORS[mixType(s.type)] +
        '">' +
        escapeHtml(translateSignalValue(s.value)) +
        "</div></div>" +
        '<div class="text-[12px] text-muted mt-1.5">' +
        escapeHtml(realSub) +
        "</div>" +
        buildChecklistDropdown(dropdownId, checklist) +
        "</div>"
      );
    })
    .join("");

  signals.forEach((_, idx) => attachChecklistListener("detail-checklist-" + idx));
  renderSignalMix(signals);

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
      // Not "Confirmed": the same check's row above can read "Not
      // confirmed", and the two looked like they contradicted each
      // other. Same words as the extension.
      const severityLabels: Record<string, string> = {
        hard: t("dash.detail.severity_serious", "Serious"),
        compound: t("dash.detail.severity_pattern_match", "Pattern match"),
        soft: t("dash.detail.severity_worth_noting", "Worth noting"),
      };
      const capitalizeFirst = (str: string): string =>
        str ? str.charAt(0).toUpperCase() + str.slice(1) : str;

      // The title is the checks the factor comes from ("Advance
      // payment request + Account age"), exactly like the extension.
      function riskFactorTitle(f: DetailRiskFactor): string {
        if (f.contributing_signals && f.contributing_signals.length > 0) {
          return f.contributing_signals.map((label) => translateSignalLabel(label)).join(" + ");
        }
        return translateSignalLabel(capitalizeFirst(f.name.replace(/_/g, " ")));
      }

      riskFactorsSection.classList.remove("hidden");
      riskFactorsSection.innerHTML =
        '<div class="text-[10px] font-extrabold uppercase tracking-wider text-muted mb-2">' +
        t("dash.detail.risk_factors", "Risk factors") +
        "</div>" +
        riskFactors
          .map((f, idx) => {
            const colorClass = severityColors[f.severity] || "text-muted";
            const severityLabel = severityLabels[f.severity] || f.severity;

            // Same ###CHECKLIST### parsing the signals list above
            // already uses - a risk factor's description is very
            // often the exact same text as the signal.sub it was
            // derived from (e.g. "Listing completeness").
            const { realSub, checklist } = parseChecklistSignal(f.description);
            const dropdownId = "detail-rf-checklist-" + idx;

            return (
              '<div class="bg-surface border border-line rounded-xl p-4 mb-2.5 last:mb-0">' +
              '<div class="flex justify-between items-baseline gap-3">' +
              '<div class="font-semibold text-[13px]">' +
              escapeHtml(riskFactorTitle(f)) +
              "</div>" +
              '<div class="text-[13px] font-bold whitespace-nowrap ' +
              colorClass +
              '">' +
              escapeHtml(severityLabel) +
              "</div></div>" +
              '<div class="text-[12px] text-muted mt-1.5">' +
              escapeHtml(capitalizeFirst(realSub)) +
              "</div>" +
              buildChecklistDropdown(dropdownId, checklist) +
              "</div>"
            );
          })
          .join("");

      riskFactors.forEach((_, idx) => attachChecklistListener("detail-rf-checklist-" + idx));
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
          '<div class="text-[10px] font-extrabold uppercase tracking-wider text-muted mb-1.5">' +
          t("dash.detail.report_reason", "Report reason") +
          "</div>" +
          '<div class="text-[14px] font-semibold">' +
          escapeHtml(translateReportReason(r.report_type)) +
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
        escapeHtml(translateSocialPlatform(platform)) +
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
