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
  "Checked by Made-in-China": "dash.value.checked_by_madeinchina",
  "Licence checked": "dash.value.licence_checked",
  "Checked by Global Sources": "dash.value.checked_by_globalsources",
  "Trade show exhibitor": "dash.value.trade_show_exhibitor",
  "Open a product to check this": "dash.value.open_a_product",
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
  no_delivery: ["dash.report.no_delivery", "Took payment and never shipped"],
  fake_item: ["dash.report.fake_item", "Goods were fake or not as described"],
  wrong_item: ["dash.report.wrong_item", "Sent the wrong goods"],
  non_responsive: ["dash.report.non_responsive", "Stopped replying after payment"],
  scam: ["dash.report.scam", "Something else looked like a scam"],
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

// ---- Result page -------------------------------------------------
// Same layout and words as the extension's panel (extension/ts/tabs/
// risk.ts and intelligence.ts): a dark score card, "What we found",
// the supplier, and a Details tab with the checks - change both
// together.

const CHECK_ICON =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 12.5l4.5 4.5L19 7.5"/></svg>';
const CROSS_ICON =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" aria-hidden="true"><path d="M7 7l10 10M17 7L7 17"/></svg>';
const CHEVRON =
  '<svg class="d-chev" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M6 9l6 6 6-6"/></svg>';

// Rows shown before "Show more".
const FIRST_FINE = 2;
const FIRST_PLATFORMS = 3;
const MAX_FINDINGS = 3;

const NOT_ENOUGH_INFORMATION = "not_enough_information";

const PLATFORM_NAMES: Record<string, string> = {
  b2brazil: "B2Brazil",
  alibaba: "Alibaba",
  tradewheel: "TradeWheel",
  exporthub: "ExportHub",
  b2bmap: "B2BMap",
  thomasnet: "ThomasNet",
  kompass: "Kompass",
  wholesalecentral: "Wholesale Central",
  madeinchina: "Made-in-China",
  globalsources: "Global Sources",
};

function platformName(platform: string | null | undefined): string {
  if (!platform) return "";
  return PLATFORM_NAMES[platform.toLowerCase()] || platform;
}

function capitalizeFirst(str: string): string {
  return str ? str.charAt(0).toUpperCase() + str.slice(1) : str;
}

function isFlagged(type: string): boolean {
  return type !== "good" && type !== "info";
}

// Values that mean "we don't know": rows with these are not shown.
function hasValue(value: string | null | undefined): value is string {
  if (!value) return false;
  return ["", "not found", "unknown", "none", "n/a"].indexOf(value.trim().toLowerCase()) === -1;
}

// One row. With extra text it opens on click; extra rows stay hidden
// until "Show more".
function detailRow(opts: {
  mark: string;
  title: string;
  value?: string;
  moreHTML?: string;
  extra?: boolean;
  open?: boolean;
  quiet?: boolean;
}): string {
  const canOpen = !!opts.moreHTML;
  return (
    '<div class="d-row' +
    (opts.extra ? " d-extra" : "") +
    (canOpen && opts.open ? " is-open" : "") +
    '">' +
    (canOpen
      ? '<button type="button" class="d-row-head" data-open aria-expanded="' +
        (opts.open ? "true" : "false") +
        '">'
      : '<div class="d-row-head">') +
    opts.mark +
    '<span class="d-title' +
    (opts.quiet ? " is-quiet" : "") +
    '">' +
    escapeHtml(opts.title) +
    (opts.value ? "<small>" + escapeHtml(opts.value) + "</small>" : "") +
    "</span>" +
    (canOpen ? CHEVRON + "</button>" : "</div>") +
    (canOpen
      ? '<div class="d-more-wrap"><div><div class="d-text">' + opts.moreHTML + "</div></div></div>"
      : "") +
    "</div>"
  );
}

function moreButton(hidden: number): string {
  if (hidden <= 0) return "";
  const label = t("dash.detail.show_more", "Show {n} more").replace("{n}", String(hidden));
  return (
    '<button type="button" class="d-more" data-more="' +
    escapeAttr(label) +
    '"><span>' +
    escapeHtml(label) +
    "</span>" +
    CHEVRON +
    "</button>"
  );
}

function mark(kind: "flag" | "ok" | "info" | "none"): string {
  if (kind === "flag") return '<span class="d-mark d-mark-flag">!</span>';
  if (kind === "info") return '<span class="d-mark d-mark-ok">i</span>';
  if (kind === "none") return '<span class="d-mark d-mark-none">' + CROSS_ICON + "</span>";
  return '<span class="d-mark d-mark-ok">' + CHECK_ICON + "</span>";
}

// ✓ for what the supplier gave, ✕ for what is missing.
function checklistHTML(checklist: [string, boolean][]): string {
  if (checklist.length === 0) return "";
  return (
    "<ul>" +
    checklist
      .map(
        ([name, present]) =>
          "<li" +
          (present ? "" : ' class="is-missing"') +
          ">" +
          (present ? CHECK_ICON : CROSS_ICON) +
          escapeHtml(translateChecklistField(name)) +
          "</li>",
      )
      .join("") +
    "</ul>"
  );
}

// ---- Score card ----------------------------------------------------

function buildScoreCard(data: DetailResponse): string {
  const signals = data.signals || [];
  const flags = signals.filter((s) => isFlagged(s.type)).length;
  const level = data.risk_level;
  const tooLittle = (data.risk_factors || []).some((f) => f && f.name === NOT_ENOUGH_INFORMATION);

  const labels: Record<string, string> = {
    low: t("dash.risk.low", "Low risk"),
    caution: t("dash.risk.caution", "Caution"),
    high: t("dash.risk.high", "High risk"),
  };
  const label = tooLittle
    ? t("dash.label.not_enough_information", "Not enough information")
    : labels[level] || labels.high;

  // Never "no warning signs" while a check needs a look.
  const say = tooLittle
    ? t("dash.detail.say_too_little", "Check this supplier carefully yourself")
    : level === "low"
      ? flags === 0
        ? t("dash.detail.say_low_clean", "No warning signs found.")
        : t("dash.detail.say_low", "No serious warning signs.")
      : level === "caution"
        ? t("dash.detail.say_caution", "Check these points before you pay.")
        : t("dash.detail.say_high", "Don't pay this supplier yet.");

  const sub =
    flags === 0
      ? t("dash.detail.sub_all_fine", "All {total} checks look fine.").replace(
          "{total}",
          String(signals.length),
        )
      : flags === 1
        ? t("dash.detail.sub_one", "1 of {total} checks needs a look.").replace(
            "{total}",
            String(signals.length),
          )
        : t("dash.detail.sub_many", "{n} of {total} checks need a look.")
            .replace("{n}", String(flags))
            .replace("{total}", String(signals.length));

  const score = Math.max(0, Math.min(100, data.risk_score || 0));
  const r = 42;
  const c = 2 * Math.PI * r;
  return (
    '<div class="verdict-card"><div class="verdict-ring"><svg viewBox="0 0 100 100">' +
    '<circle cx="50" cy="50" r="' +
    r +
    '" fill="none" stroke="var(--card-track)" stroke-width="8"/>' +
    '<circle class="verdict-ring-arc" cx="50" cy="50" r="' +
    r +
    '" fill="none" stroke="#ffb938" stroke-width="8" stroke-linecap="round" stroke-dasharray="' +
    ((c * score) / 100).toFixed(1) +
    " " +
    c.toFixed(1) +
    '"/></svg><div class="verdict-number">' +
    score +
    '</div></div><div><div class="verdict-level' +
    (level === "low" && !tooLittle ? " is-calm" : "") +
    '">' +
    escapeHtml(label) +
    '</div><div class="verdict-say">' +
    escapeHtml(say) +
    '</div><div class="verdict-sub">' +
    escapeHtml(sub) +
    "</div></div></div>"
  );
}

// ---- What we found ---------------------------------------------------

const SEVERITY_ORDER = ["hard", "compound", "soft"];

function severityLabel(severity: string): string {
  // Not "Confirmed": the same check's row can read "Not confirmed", and
  // the two looked like they contradicted each other.
  const labels: Record<string, string> = {
    hard: t("dash.detail.severity_serious", "Serious"),
    compound: t("dash.detail.severity_pattern_match", "Pattern match"),
    soft: t("dash.detail.severity_worth_noting", "Worth noting"),
  };
  return labels[severity] || labels.soft;
}

// The title is the checks the factor comes from ("Advance payment
// request + Account age"), exactly like the extension.
function riskFactorTitle(f: DetailRiskFactor): string {
  if (f.contributing_signals && f.contributing_signals.length > 0) {
    return f.contributing_signals.map((label) => translateSignalLabel(label)).join(" + ");
  }
  return translateSignalLabel(capitalizeFirst(f.name.replace(/_/g, " ")));
}

// Always up to three rows, problems first: the risk factors (most
// serious first) or, without them, the flagged checks; then checks
// that passed fill the rest.
function buildFindings(data: DetailResponse): string {
  const signals = data.signals || [];
  const factors = (data.risk_factors || [])
    .filter((f) => f && f.name !== NOT_ENOUGH_INFORMATION)
    .slice()
    .sort((a, b) => SEVERITY_ORDER.indexOf(a.severity) - SEVERITY_ORDER.indexOf(b.severity));

  interface Finding {
    flag: boolean;
    title: string;
    value: string;
    text: string;
    checklist: [string, boolean][];
  }
  const fromSignal = (s: DetailSignal): Finding => {
    const { realSub, checklist } = parseChecklistSignal(s.sub);
    return {
      flag: isFlagged(s.type),
      title: translateSignalLabel(s.label),
      value: translateSignalValue(s.value),
      text: capitalizeFirst(realSub.trim()),
      checklist,
    };
  };
  const problems: Finding[] =
    factors.length > 0
      ? factors.map((f) => {
          const { realSub, checklist } = parseChecklistSignal(f.description);
          return {
            flag: true,
            title: riskFactorTitle(f),
            value: severityLabel(f.severity),
            text: capitalizeFirst(realSub.trim()),
            checklist,
          };
        })
      : signals.filter((s) => isFlagged(s.type)).map(fromSignal);
  const passed = signals.filter((s) => s.type === "good").map(fromSignal);
  const list = problems.concat(passed).slice(0, MAX_FINDINGS);
  if (list.length === 0) return "";

  return (
    '<div class="d-heading">' +
    escapeHtml(t("dash.detail.what_we_found", "What we found")) +
    '</div><div class="d-rows">' +
    list
      .map((f, i) =>
        detailRow({
          mark: mark(f.flag ? "flag" : "ok"),
          title: f.title,
          value: f.value,
          moreHTML: f.text || f.checklist.length ? escapeHtml(f.text) + checklistHTML(f.checklist) : "",
          open: i === 0 && f.flag,
        }),
      )
      .join("") +
    "</div>"
  );
}

// ---- Supplier --------------------------------------------------------

function fact(label: string, value: string): string {
  return (
    '<div class="d-fact"><small>' +
    escapeHtml(label) +
    "</small><div>" +
    escapeHtml(value) +
    "</div></div>"
  );
}

// B2B suppliers show the company's age ("About 21 years", from the
// founding year), not how long they have been on the platform, so the
// label says which - "On Global Sources for: About 21 years" was wrong.
// Same rule as the extension (extension/ts/tabs/risk.ts) - change both
// together.
function isCompanyAge(value: string): boolean {
  return value === "Founded this year" || /^About \d+ years?$/.test(value);
}

function buildFacts(data: DetailResponse): string {
  const seller = data.seller;
  const platform = platformName(data.platform);
  const facts: string[] = [];
  if (hasValue(seller.account_age)) {
    facts.push(
      fact(
        isCompanyAge(seller.account_age)
          ? t("dash.detail.company_age", "Company age")
          : platform
            ? t("dash.detail.on_platform_for", "On {platform} for").replace("{platform}", platform)
            : t("dash.detail.account_age", "Account age"),
        translateAccountAge(seller.account_age),
      ),
    );
  }
  facts.push(
    fact(t("dash.detail.reports_on_safely", "Reports on Safely"), String(data.fraud_report_count || 0)),
  );
  if (hasValue(seller.phone)) facts.push(fact(t("dash.detail.phone", "Phone"), seller.phone));
  if (seller.verification === "verified" || seller.verification === "reported") {
    facts.push(
      fact(
        t("dash.detail.status", "Status"),
        seller.verification === "verified"
          ? t("dash.detail.verified", "Verified")
          : t("dash.detail.reported", "Reported"),
      ),
    );
  }
  if (hasValue(seller.handle)) facts.push(fact(t("dash.detail.contact", "Contact"), seller.handle));
  if (hasValue(seller.last_active)) {
    facts.push(fact(t("dash.detail.last_active", "Last active"), seller.last_active));
  }
  return facts.join("");
}

// Visits by month, January to December. This month is dark, the rest
// grey. Hidden when there were no visits at all.
function buildVisits(months: number[] | null): string {
  const values = (months || []).slice(0, 12).map((v) => Math.max(0, Number(v) || 0));
  const total = values.reduce((sum, v) => sum + v, 0);
  if (total === 0) return "";
  const max = Math.max.apply(null, values);
  const thisMonth = new Date().getMonth();
  return (
    '<div class="d-bars">' +
    values
      .map(
        (v, i) =>
          "<i" +
          (i === thisMonth ? ' class="is-now"' : "") +
          ' style="height:' +
          (v === 0 ? 2 : Math.max(4, Math.round((v / max) * 42))) +
          'px"></i>',
      )
      .join("") +
    '</div><div class="d-bars-label"><span>' +
    escapeHtml(t("dash.detail.visits_by_month", "Safely visits by month")) +
    "</span><span>" +
    total +
    "</span></div>"
  );
}

// ---- Details tab -----------------------------------------------------

function signalRow(s: DetailSignal, extra: boolean): string {
  const type = isFlagged(s.type) ? "flag" : s.type === "info" ? "info" : "ok";
  const { realSub, checklist } = parseChecklistSignal(s.sub);
  const text = capitalizeFirst(realSub.trim());
  return detailRow({
    mark: mark(type),
    title: translateSignalLabel(s.label),
    value: translateSignalValue(s.value),
    moreHTML: text || checklist.length ? escapeHtml(text) + checklistHTML(checklist) : "",
    extra,
  });
}

function buildDetailsTab(data: DetailResponse): string {
  const signals = data.signals || [];
  const flagged = signals.filter((s) => isFlagged(s.type));
  const fine = signals.filter((s) => !isFlagged(s.type));

  let html =
    '<div class="d-counts"><div class="d-count' +
    (flagged.length > 0 ? " stat-hot" : "") +
    '"><b>' +
    flagged.length +
    "</b><small>" +
    escapeHtml(
      flagged.length === 1
        ? t("dash.detail.needs_a_look_count", "needs a look")
        : t("dash.detail.need_a_look_count", "need a look"),
    ) +
    '</small></div><div class="d-count"><b>' +
    fine.length +
    "</b><small>" +
    escapeHtml(
      fine.length === 1
        ? t("dash.detail.looks_fine_count", "looks fine")
        : t("dash.detail.look_fine_count", "look fine"),
    ) +
    "</small></div></div>";

  if (flagged.length > 0) {
    html +=
      '<div class="d-heading">' +
      escapeHtml(t("dash.detail.needs_a_look", "Needs a look")) +
      '</div><div class="d-rows">' +
      flagged.map((s) => signalRow(s, false)).join("") +
      "</div>";
  }
  if (fine.length > 0) {
    html +=
      '<div class="d-heading">' +
      escapeHtml(t("dash.detail.looks_fine", "Looks fine")) +
      '</div><div class="d-rows">' +
      fine.map((s, i) => signalRow(s, i >= FIRST_FINE)).join("") +
      moreButton(fine.length - FIRST_FINE) +
      "</div>";
  }
  return html + buildPresence(data.social_candidates || []);
}

const PLATFORM_ORDER = [
  "Facebook", "LinkedIn", "TikTok", "Instagram", "Reddit", "Trustpilot",
  "Reviews", "Contact (Facebook)", "Contact (LinkedIn)", "Contact (Web)",
];

// One row per platform where something was found; it opens to show
// every link. The first 3 show, the rest sit under "Show more".
// Platforms with nothing found share one line at the end.
function buildPresence(results: PlatformCheckResult[]): string {
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
    Object.keys(grouped).filter((p) => PLATFORM_ORDER.indexOf(p) === -1),
  );
  const found = order.filter((p) => grouped[p].length > 0);
  const missing = order.filter((p) => grouped[p].length === 0);

  const rows = found
    .map((platform, i) => {
      const links = grouped[platform];
      return detailRow({
        mark: mark("ok"),
        title: translateSocialPlatform(platform),
        value:
          links.length === 1
            ? t("dash.detail.one_link", "1 link")
            : t("dash.detail.links", "{n} links").replace("{n}", String(links.length)),
        moreHTML: links
          .map(
            (link) =>
              '<a href="' +
              escapeAttr(link.url) +
              '" target="_blank" rel="noopener noreferrer">' +
              escapeHtml(link.title || link.url) +
              "</a>",
          )
          .join(""),
        extra: i >= FIRST_PLATFORMS,
      });
    })
    .join("");
  const none =
    missing.length === 0
      ? ""
      : detailRow({
          mark: mark("none"),
          title: t("dash.detail.not_found_on", "Not found on {list}").replace(
            "{list}",
            missing.map((p) => translateSocialPlatform(p)).join(", "),
          ),
          quiet: true,
        });

  return (
    '<div class="d-heading">' +
    escapeHtml(t("dash.detail.online_presence", "Online presence")) +
    '</div><div class="d-rows">' +
    rows +
    moreButton(found.length - FIRST_PLATFORMS) +
    none +
    "</div>"
  );
}

// ---- Report tab ------------------------------------------------------

function buildReports(reports: DetailReport[]): string {
  return (
    '<div class="d-rows">' +
    reports
      .map((r) =>
        detailRow({
          mark: mark("flag"),
          title: translateReportReason(r.report_type),
          value: t("dash.detail.submitted", "Submitted") + " " + formatDate(r.reported_at),
        }),
      )
      .join("") +
    "</div>"
  );
}

function renderDetailBody(data: DetailResponse): void {
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

  (document.getElementById("detail-verdict") as HTMLElement).innerHTML = buildScoreCard(data);
  (document.getElementById("detail-findings") as HTMLElement).innerHTML = buildFindings(data);

  const seller = data.seller;
  (document.getElementById("detail-seller-name") as HTMLElement).textContent = hasValue(seller.name)
    ? seller.name
    : platformName(data.platform) || t("dash.detail.supplier", "Supplier");
  const locationEl = document.getElementById("detail-seller-location") as HTMLElement;
  locationEl.textContent = hasValue(seller.location) ? seller.location : "";
  locationEl.classList.toggle("hidden", !hasValue(seller.location));
  (document.getElementById("detail-facts") as HTMLElement).innerHTML = buildFacts(data);
  (document.getElementById("detail-visits") as HTMLElement).innerHTML = buildVisits(
    seller.monthly_activity,
  );

  (document.getElementById("detail-tab-content-intel") as HTMLElement).innerHTML =
    buildDetailsTab(data);

  const filedBlock = document.getElementById("detail-report-filed") as HTMLElement;
  const emptyBlock = document.getElementById("detail-report-empty") as HTMLElement;
  const reports = data.reports || [];
  filedBlock.innerHTML = reports.length > 0 ? buildReports(reports) : "";
  filedBlock.classList.toggle("hidden", reports.length === 0);
  emptyBlock.classList.toggle("hidden", reports.length > 0);

  switchDetailTab("risk");
}

// Rows that open and "Show more" / "Show less", for every list on the
// result page.
document.getElementById("detail-view")?.addEventListener("click", (e: MouseEvent) => {
  const target = e.target as HTMLElement;
  const opener = target.closest("[data-open]");
  if (opener) {
    const row = opener.closest(".d-row");
    if (row) {
      const open = row.classList.toggle("is-open");
      opener.setAttribute("aria-expanded", open ? "true" : "false");
    }
    return;
  }
  const more = target.closest("[data-more]") as HTMLElement | null;
  if (more) {
    const list = more.closest(".d-rows");
    if (!list) return;
    const all = list.classList.toggle("show-all");
    const label = more.querySelector("span");
    if (label) {
      label.textContent = all ? t("dash.detail.show_less", "Show less") : more.dataset.more || "";
    }
  }
});

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

function switchDetailTab(tab: string): void {
  ["risk", "intel", "report"].forEach((name) => {
    const content = document.getElementById("detail-tab-content-" + name);
    if (content) content.classList.toggle("hidden", name !== tab);
  });
  document.querySelectorAll<HTMLElement>(".detail-tab-btn").forEach((btn) => {
    const active = btn.dataset.detailTab === tab;
    btn.classList.toggle("is-active", active);
  });
}

function closeDetailPanel(): void {
  const panel = document.getElementById("detail-view");
  if (panel) panel.classList.add("hidden");
}
