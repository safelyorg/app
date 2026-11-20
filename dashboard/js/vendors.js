"use strict";
// The "Vendors" page (/vendors). The buyer invites vendors to fill in a
// short form (safely.sh/v/<link>); Safely checks the answers and only
// the team sees the result (backend services/features/vendors.rs).
//
// GET /api/v1/vendors gives: paid, buyer_company, counts, items.
// GET /api/v1/vendors/<id> gives one vendor's answers and result.
// Free: a locked card. /vendors/<id> is one vendor's page (the link in
// the "check ready" email).
// The answers' names, in the form's order.
const VENDOR_FIELDS = [
    ["company_name", "dash.vendors.field.company_name", "Company name"],
    ["country", "dash.vendors.field.country", "Country"],
    ["company_number", "dash.vendors.field.company_number", "Company number"],
    ["email", "dash.vendors.field.email", "Work email"],
    ["phone", "dash.vendors.field.phone", "Phone"],
    ["website", "dash.vendors.field.website", "Website"],
    ["address", "dash.vendors.field.address", "Company address"],
    ["bank_holder", "dash.vendors.field.bank_holder", "Bank account name"],
    ["bank_account", "dash.vendors.field.bank_account", "Bank account number (IBAN)"],
    ["bank_code", "dash.vendors.field.bank_code", "Bank code (SWIFT/BIC or routing)"],
];
// The checks' titles, translated (the details stay in English, like
// the outside records).
const VENDOR_CARD_TITLES = {
    email_code: ["dash.vendors.card.email_code", "Email confirmed"],
    free_email: ["dash.vendors.card.free_email", "Email address"],
    bank_name: ["dash.vendors.card.bank_name", "Bank account name"],
    known_all: ["dash.vendors.card.known_all", "What you already knew"],
    website_missing: ["dash.vendors.card.website_missing", "Website"],
    email_website: ["dash.vendors.card.email_website", "Email and website"],
    website_age: ["dash.vendors.card.website_age", "Website age"],
    email_domain_age: ["dash.vendors.card.email_domain_age", "Email domain age"],
    scam_reports: ["dash.vendors.card.scam_reports", "Scam reports"],
    social_presence: ["dash.detail.online_presence", "Online presence"],
    phone_scam: ["dash.vendors.card.phone_scam", "Phone in scam reports"],
    phone_online: ["dash.vendors.card.phone_online", "Phone number online"],
    web_warnings: ["dash.vendors.card.web_warnings", "Scam warnings online"],
    phone_country: ["dash.vendors.card.phone_country", "Phone country"],
    address_mailbox: ["dash.vendors.card.address_mailbox", "Address type"],
    address_map: ["dash.vendors.card.address_map", "Address on the map"],
    website_content: ["dash.vendors.card.website_content", "Website content"],
    bank_account: ["dash.vendors.card.bank_account", "Bank account number"],
    bank_code: ["dash.vendors.card.bank_code", "Bank code"],
    same_account: ["dash.vendors.card.same_account", "Bank account used by another vendor"],
    same_email: ["dash.vendors.card.same_email", "Email used for another company"],
    holder_sanctions: ["dash.vendors.card.holder_sanctions", "Bank account name on sanctions lists"],
    doc_registration: ["dash.vendors.card.doc_registration", "Registration document"],
    doc_bank: ["dash.vendors.card.doc_bank", "Bank document"],
    company_number: ["dash.vendors.card.company_number", "Company number"],
    no_website_free_email: ["dash.vendors.card.no_website_free_email", "No website and a free email"],
};
// The kinds of file a vendor can upload (backend vendor_checks.rs).
const VENDOR_DOC_KINDS = {
    registration: ["dash.vendors.doc.registration", "Company registration"],
    bank_letter: ["dash.vendors.doc.bank_letter", "Bank letter"],
};
// Report reasons, the same as the extension's (detail_view.ts
// REPORT_REASON_TRANSLATIONS).
const VENDOR_REPORT_REASONS = ["no_delivery", "fake_item", "wrong_item", "non_responsive", "scam"];
// "Before you pay" for a vendor: the supplier steps (detail_view.ts
// PAY_CHECKS).
const VENDOR_PAY_CHECKS = ["company_account", "confirm_bank", "licence_bank", "sample", "video_call"];
let vendorList = null;
// The list's choices (kept while the page is open).
let vendorWho = "me";
let vendorRisk = "all";
let vendorSearch = "";
let vendorOpenId = null;
function vEsc(text) {
    return String(text ?? "")
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
}
function vendorFieldName(field) {
    const found = VENDOR_FIELDS.find((f) => f[0] === field);
    return found ? t(found[1], found[2]) : field;
}
function vPill(cls, text) {
    return ('<span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11.5px] font-bold whitespace-nowrap ' +
        cls +
        '">' +
        vEsc(text) +
        "</span>");
}
const V_GREY = "bg-surface2 text-muted";
const V_GOOD = "bg-mint/10 text-mint";
const V_BAD = "bg-coral/10 text-coral";
const V_WARN = "bg-[#fef3c7] text-[#92400e]";
function vendorStatusPill(status) {
    switch (status) {
        case "invited":
            return vPill(V_GREY, t("dash.vendors.status.invited", "Invited"));
        case "opened":
            return vPill(V_GREY, t("dash.vendors.status.opened", "Opened the form"));
        case "checking":
            return vPill(V_GREY, t("dash.vendors.status.checking", "Checking…"));
        case "no_scans":
            return vPill(V_WARN, t("dash.vendors.status.no_scans", "Waiting for scans"));
        case "ready":
            return vPill("bg-ink text-bg", t("dash.vendors.status.ready", "Ready to review"));
        case "more_info":
            return vPill(V_WARN, t("dash.vendors.status.more_info", "Asked for changes"));
        case "approved":
            return vPill(V_GOOD, "✓ " + t("dash.vendors.status.approved", "Approved"));
        case "rejected":
            return vPill(V_BAD, t("dash.vendors.status.rejected", "Rejected"));
        case "canceled":
            return vPill(V_GREY, t("dash.vendors.status.canceled", "Canceled"));
        case "expired":
            return vPill(V_GREY, t("dash.vendors.status.expired", "Link expired"));
        default:
            return vPill(V_GREY, status);
    }
}
function vendorRiskPill(level, score) {
    if (!level)
        return '<span class="text-muted">—</span>';
    const n = score === null ? "" : " · " + score;
    if (level === "high")
        return vPill(V_BAD, t("dash.filter.high", "High") + n);
    if (level === "caution")
        return vPill(V_WARN, t("dash.filter.caution", "Caution") + n);
    return vPill(V_GOOD, t("dash.filter.low", "Low") + n);
}
function vendorStat(big, label, hot = false) {
    return ('<div class="' +
        (hot ? "stat-hot " : "") +
        'bg-surface border border-line rounded-2xl px-4 py-3.5">' +
        '<span class="block font-display text-[24px] leading-tight">' +
        vEsc(big) +
        "</span>" +
        '<span class="text-[12px] text-muted">' +
        vEsc(label) +
        "</span></div>");
}
const V_BTN = "px-3.5 py-2 rounded-lg bg-surface2 border border-line text-[12.5px] font-bold hover:bg-surface3 transition";
const V_BTN_DARK = "px-3.5 py-2 rounded-lg bg-ink text-bg text-[12.5px] font-bold hover:opacity-90 transition";
const V_INPUT = "w-full h-10 px-3 rounded-lg border border-line bg-surface text-[13px] outline-none focus:border-ink";
function vField(id, label, value = "", placeholder = "") {
    return ('<label class="block"><span class="block text-[12px] font-semibold text-muted mb-1.5">' +
        vEsc(label) +
        '</span><input id="' +
        id +
        '" class="' +
        V_INPUT +
        '" value="' +
        vEsc(value) +
        '" placeholder="' +
        vEsc(placeholder) +
        '" /></label>');
}
// The invite box: one vendor, or a list (CSV).
function buildInviteBox(buyer) {
    const optional = t("dash.vendors.optional", "optional");
    return ('<div id="vendor-invite" class="hidden bg-surface border border-line rounded-2xl p-5 mb-6">' +
        '<div class="flex items-center justify-between gap-3 mb-4">' +
        '<div class="text-[15px] font-bold">' +
        vEsc(t("dash.vendors.invite_title", "Invite vendors")) +
        "</div>" +
        '<div class="flex gap-1 bg-surface2 rounded-lg p-1 text-[12px] font-bold">' +
        '<button type="button" data-vendor-mode="one" class="px-3 py-1.5 rounded-md bg-surface">' +
        vEsc(t("dash.vendors.one_vendor", "One vendor")) +
        '</button><button type="button" data-vendor-mode="list" class="px-3 py-1.5 rounded-md text-muted">' +
        vEsc(t("dash.vendors.upload_list", "Upload a list")) +
        "</button></div></div>" +
        '<div class="grid sm:grid-cols-2 gap-3">' +
        vField("vi-buyer", t("dash.vendors.your_company", "Your company name (the vendor sees it)"), buyer) +
        '<label class="block"><span class="block text-[12px] font-semibold text-muted mb-1.5">' +
        vEsc(t("dash.vendors.form_language", "Form language")) +
        '</span><select id="vi-language" class="' +
        V_INPUT +
        '"><option value="en">English</option><option value="pt">Português</option></select></label>' +
        "</div>" +
        '<div data-vendor-panel="one" class="grid sm:grid-cols-2 gap-3 mt-3">' +
        vField("vi-name", t("dash.vendors.vendor_company", "Vendor's company name")) +
        vField("vi-email", t("dash.vendors.vendor_email", "Vendor's email")) +
        vField("vi-country", t("dash.vendors.field.country", "Country") + " (" + optional + ")") +
        vField("vi-phone", t("dash.vendors.field.phone", "Phone") + " (" + optional + ")") +
        vField("vi-website", t("dash.vendors.field.website", "Website") + " (" + optional + ")") +
        '<p class="text-[11.5px] text-muted self-end pb-1">' +
        vEsc(t("dash.vendors.known_hint", "What you already know is compared with the vendor's answers.")) +
        "</p></div>" +
        '<div data-vendor-panel="list" class="hidden mt-3">' +
        '<p class="text-[12.5px] text-muted mb-2">' +
        vEsc(t("dash.vendors.csv_help", "A CSV file with the columns name, email, country, phone, website (only name and email are needed). Up to 200 vendors.")) +
        '</p><input type="file" id="vi-file" accept=".csv,text/csv" class="text-[12.5px]" />' +
        '<textarea id="vi-csv" rows="5" class="mt-2 w-full p-3 rounded-lg border border-line bg-surface text-[12.5px] font-mono outline-none focus:border-ink" placeholder="name,email,country\nNorthfield Metals,sales@northfield.com,United States"></textarea>' +
        '<p id="vi-csv-count" class="text-[12px] text-muted mt-1"></p></div>' +
        '<label class="flex items-center gap-2 mt-4 text-[13px]"><input type="checkbox" id="vi-send" checked /> ' +
        vEsc(t("dash.vendors.send_email", "Safely emails the link to the vendor")) +
        "</label>" +
        '<div class="flex flex-wrap items-center gap-2 mt-4">' +
        '<button type="button" data-vendor-send class="' +
        V_BTN_DARK +
        '">' +
        vEsc(t("dash.vendors.send_invite", "Send invite")) +
        '</button><button type="button" data-vendor-close-invite class="' +
        V_BTN +
        '">' +
        vEsc(t("dash.common.cancel", "Cancel")) +
        "</button>" +
        '<span class="text-[12px] text-muted">' +
        vEsc(t("dash.vendors.scan_note", "Each vendor uses 1 scan, when they send the form.")) +
        "</span></div>" +
        '<div id="vi-result" class="mt-4"></div></div>');
}
// What the invite did: new links (Copy / WhatsApp), vendors already
// invited (Open), and emails Safely did not send because the address
// got too many today.
function buildInviteResult(invited, vendors) {
    const fresh = invited.filter((i) => !i.already && i.link);
    const already = invited.filter((i) => i.already);
    const limited = fresh.filter((i) => i.email_limited);
    const sent = fresh.filter((i) => i.email_sent).length;
    const phoneOf = (email) => {
        const v = vendors.find((x) => (x.email || "").trim().toLowerCase() === email);
        return v ? v.phone : undefined;
    };
    let html = "";
    if (fresh.length) {
        html +=
            '<div class="text-[13px] font-bold mb-1">' +
                vEsc(t("dash.vendors.invited_done", "{n} invited. {sent} emailed by Safely.")
                    .replace("{n}", String(fresh.length))
                    .replace("{sent}", String(sent))) +
                '</div><p class="text-[12px] text-muted mb-2">' +
                vEsc(t("dash.vendors.links_once", "Copy the links now if you want to send them yourself: they are shown only once.")) +
                "</p>" +
                fresh.map((i) => vendorLinkBox(i.name, i.link, phoneOf(i.email))).join("");
    }
    if (limited.length) {
        html +=
            '<div class="mt-3 rounded-xl bg-[#fef3c7] text-[#92400e] p-3 text-[12.5px]">' +
                vEsc(t("dash.vendors.email_limited", "Not emailed to {list}: this address already got the most Safely emails for today. Send the link yourself (Copy or WhatsApp).").replace("{list}", limited.map((i) => i.email).join(", "))) +
                "</div>";
    }
    if (already.length) {
        html +=
            '<div class="mt-3 text-[13px] font-bold mb-1">' +
                vEsc(t("dash.vendors.already_title", "Already invited")) +
                '</div><p class="text-[12px] text-muted mb-1">' +
                vEsc(t("dash.vendors.already_text", "These vendors already have an open check. Open it instead of inviting again; after your decision you can invite them again.")) +
                "</p>" +
                already
                    .map((i) => '<div class="flex flex-wrap items-center gap-2 py-2 border-b border-line last:border-0"><span class="text-[12.5px] font-semibold flex-1 min-w-0 truncate">' +
                    vEsc(i.name) +
                    ' <span class="text-muted font-normal">' +
                    vEsc(i.email) +
                    '</span></span><button type="button" data-vendor-open="' +
                    vEsc(i.id) +
                    '" class="' +
                    V_BTN +
                    '">' +
                    vEsc(t("dash.watch.open", "Open")) +
                    "</button></div>")
                    .join("");
    }
    return html;
}
// A link to copy or send on WhatsApp.
function vendorLinkBox(name, link, phone) {
    const text = t("dash.vendors.whatsapp_text", "Hi! Please fill in this short supplier form for us: {link}").replace("{link}", link);
    const digits = (phone || "").replace(/\D/g, "");
    const wa = "https://wa.me/" + digits + "?text=" + encodeURIComponent(text);
    return ('<div class="flex flex-wrap items-center gap-2 py-2 border-b border-line last:border-0">' +
        '<span class="text-[12.5px] font-semibold min-w-[140px]">' +
        vEsc(name) +
        '</span><code class="flex-1 min-w-0 truncate text-[11.5px] text-muted bg-surface2 px-2 py-1.5 rounded">' +
        vEsc(link) +
        '</code><button type="button" data-vendor-copy="' +
        vEsc(link) +
        '" class="' +
        V_BTN +
        '">' +
        vEsc(t("dash.vendors.copy", "Copy link")) +
        '</button><a target="_blank" rel="noopener" href="' +
        vEsc(wa) +
        '" class="' +
        V_BTN +
        '">WhatsApp</a></div>');
}
// The whole list page.
function buildVendorsPage(list) {
    // Free with no vendors: the locked card. Free with vendors from a
    // paid plan: they stay listed, without inviting.
    if (!list.paid && !list.items.length) {
        return ('<div class="rounded-2xl border border-dashed border-line p-8 text-center bg-surface">' +
            '<div class="text-[15px] font-bold">' +
            vEsc(t("dash.vendors.locked_title", "Vendor checks are on the Team and Enterprise plans")) +
            "</div>" +
            '<div class="text-[13px] text-muted mt-1.5 max-w-md mx-auto leading-relaxed">' +
            vEsc(t("dash.vendors.locked_text", "Send new vendors a short form. Safely checks their answers (bank account name, email, website age, sanctions lists, company registers, scam reports) and only your team sees the result.")) +
            "</div>" +
            '<button type="button" data-vendor-plans class="mt-4 ' +
            V_BTN_DARK +
            '">' +
            vEsc(t("dash.detail.see_plans", "See plans")) +
            "</button></div>");
    }
    const c = list.counts || {};
    const allTeam = vendorWho === "team";
    let html = (list.paid
        ? '<div class="flex flex-wrap gap-2 mb-5"><button type="button" data-vendor-invite class="' +
            V_BTN_DARK +
            '">+ ' +
            vEsc(t("dash.vendors.invite", "Invite vendor")) +
            "</button></div>" +
            buildInviteBox(list.buyer_company || "")
        : '<div data-vendor-free class="mb-5 flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-dashed border-line bg-surface px-4 py-3.5"><div class="text-[12.5px] text-muted max-w-xl">' +
            vEsc(t("dash.vendors.free_note", "You're on the Free plan: your vendors show their score and top 3 reasons. Inviting vendors and the full checks are on the Team and Enterprise plans.")) +
            '</div><button type="button" data-vendor-plans class="' +
            V_BTN_DARK +
            '">' +
            vEsc(t("dash.detail.see_plans", "See plans")) +
            "</button></div>") +
        '<div class="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-6">' +
        vendorStat(String(c.waiting || 0), t("dash.vendors.stat_waiting", "waiting for the vendor")) +
        vendorStat(String(c.to_review || 0), t("dash.vendors.stat_review", "ready to review"), true) +
        vendorStat(String(c.approved || 0), t("dash.vendors.stat_approved", "approved")) +
        vendorStat(String(c.flagged || 0), t("dash.vendors.stat_flagged", "high risk")) +
        "</div>";
    // The same bar as History: the title, whose vendors, search, risk.
    const pill = (risk, label) => '<button type="button" data-vendor-risk="' +
        risk +
        '" class="px-2.5 py-1 rounded-md text-[11px] font-bold transition ' +
        (vendorRisk === risk
            ? "bg-surface text-ink shadow-[0_1px_2px_rgba(0,0,0,0.08),0_0_0_1px_rgb(var(--c-line))]"
            : "text-muted") +
        '">' +
        vEsc(label) +
        "</button>";
    html +=
        '<div class="flex justify-between items-center mb-4 gap-3 flex-wrap">' +
            '<div class="flex items-center gap-2.5 w-full sm:w-auto"><h2 class="text-[15px] font-bold tracking-tight">' +
            vEsc(t("dash.vendors.list_title", "Vendor checks")) +
            "</h2>" +
            (list.team ? '<div id="vendor-who" class="relative">' + buildVendorWho(list) + "</div>" : "") +
            '</div><div class="flex gap-2 flex-nowrap items-center"><div class="relative">' +
            '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="absolute left-3 top-1/2 -translate-y-1/2 text-muted"><circle cx="11" cy="11" r="7" /><path d="M21 21l-4-4" /></svg>' +
            '<input id="vendor-search" type="text" value="' +
            vEsc(vendorSearch) +
            '" placeholder="' +
            vEsc(t("dash.history.search", "Search")) +
            '" class="pl-8 pr-3 py-2 bg-surface border border-line rounded-lg text-[12px] font-medium w-32 sm:w-52 outline-none focus:border-brand transition" /></div>' +
            '<div class="flex gap-0.5 bg-surface border border-line rounded-lg p-1">' +
            pill("all", t("dash.filter.all", "All")) +
            pill("low", t("dash.filter.low", "Low")) +
            pill("caution", t("dash.filter.caution", "Caution")) +
            pill("high", t("dash.filter.high", "High")) +
            "</div></div></div>";
    if (!list.items.length) {
        return (html +
            '<div class="rounded-2xl border border-line bg-surface p-8 text-center">' +
            '<div class="text-[14px] font-bold">' +
            vEsc(vendorWho === "me"
                ? t("dash.vendors.empty_title", "No vendors yet")
                : t("dash.vendors.empty_other", "No vendors here yet")) +
            '</div><div class="text-[13px] text-muted mt-1.5">' +
            vEsc(t("dash.vendors.empty_text", 'Click "Invite vendor" to send your first form.')) +
            "</div></div>");
    }
    html +=
        '<div class="bg-surface border border-line rounded-2xl overflow-hidden"><table class="w-full table-fixed">' +
            "<colgroup><col />" +
            (allTeam ? '<col class="hidden sm:table-column" style="width:170px" />' : "") +
            '<col style="width:170px" /><col class="hidden sm:table-column" style="width:120px" /><col style="width:90px" /></colgroup>' +
            '<thead><tr class="text-left text-[12px] font-semibold text-muted bg-surface2 border-b border-line">' +
            '<th class="px-4 py-3">' +
            vEsc(t("dash.vendors.col_vendor", "Vendor")) +
            "</th>" +
            (allTeam
                ? '<th class="hidden sm:table-cell px-4 py-3">' + vEsc(t("dash.vendors.col_invited_by", "Invited by")) + "</th>"
                : "") +
            '<th class="px-4 py-3">' +
            vEsc(t("dash.watch.col_status", "Status")) +
            '</th><th class="hidden sm:table-cell px-4 py-3">' +
            vEsc(t("dash.table.risk", "Risk")) +
            "</th><th></th></tr></thead><tbody>";
    list.items.forEach((item) => {
        const who = item.invited_by_you
            ? t("dash.watch.added_by_you", "You")
            : item.invited_by_name || item.invited_by || "—";
        html +=
            '<tr class="border-b border-line last:border-0" data-vendor-row data-vendor-level="' +
                vEsc(item.risk_level || "none") +
                '" data-vendor-text="' +
                vEsc((item.vendor_name + " " + item.vendor_email + " " + (item.invited_by || "")).toLowerCase()) +
                '">' +
                '<td class="px-4 py-3.5"><div class="text-[13.5px] font-semibold truncate">' +
                vEsc(item.vendor_name) +
                '</div><div class="text-[11.5px] text-muted mt-0.5 truncate">' +
                vEsc(item.vendor_email) +
                " · " +
                vEsc(item.sent_on || item.invited_on) +
                "</div></td>" +
                (allTeam ? '<td class="hidden sm:table-cell px-4 py-3.5 text-[12.5px] truncate">' + vEsc(who) + "</td>" : "") +
                '<td class="px-4 py-3.5">' +
                vendorStatusPill(item.status) +
                '</td><td class="hidden sm:table-cell px-4 py-3.5">' +
                vendorRiskPill(item.risk_level, item.risk_score) +
                '</td><td class="px-4 py-3.5 text-right"><button type="button" data-vendor-open="' +
                vEsc(item.id) +
                '" class="px-2.5 py-1 rounded-lg bg-surface2 border border-line text-[11.5px] font-bold hover:bg-surface3 transition">' +
                vEsc(t("dash.watch.open", "Open")) +
                "</button></td></tr>";
    });
    html +=
        '</tbody></table><div id="vendor-no-match" class="hidden text-center text-muted text-[13px] py-10">' +
            vEsc(t("dash.vendors.no_match", "No vendors match your search.")) +
            "</div></div>";
    return html;
}
// "My vendors" / "All team vendors" / one person, like History's
// dropdown (team_history.ts): the same people and rules.
function vendorWhoLabel(list, who) {
    if (who === "me")
        return t("dash.vendors.who_mine", "My vendors");
    if (who === "team")
        return t("dash.vendors.who_team", "All team vendors");
    const person = (list.people || []).find((p) => p.id === who);
    const email = person ? person.email : "";
    const name = email.indexOf("@") > 0 ? email.slice(0, email.indexOf("@")) : email;
    return t("dash.vendors.who_person", "{name}'s vendors").replace("{name}", name);
}
function buildVendorWho(list) {
    const row = (who, text, badge, dark) => {
        const on = who === vendorWho;
        return ('<button type="button" data-vendor-who="' +
            vEsc(who) +
            '" class="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-left text-[12.5px] hover:bg-surface2 transition' +
            (on ? " bg-surface2 font-semibold" : "") +
            '">' +
            (badge
                ? '<span class="w-5 h-5 rounded-full ' +
                    (dark ? "bg-ink text-bg" : "bg-amber/20 text-ink") +
                    ' text-[10px] font-bold flex items-center justify-center flex-shrink-0">' +
                    vEsc(badge) +
                    "</span>"
                : "") +
            '<span class="truncate">' +
            vEsc(text) +
            "</span>" +
            (on ? '<span class="ml-auto text-brand font-bold">✓</span>' : "") +
            "</button>");
    };
    let menu = row("me", vendorWhoLabel(list, "me"), "", false);
    if (list.can_see_all)
        menu += row("team", vendorWhoLabel(list, "team"), "", false);
    const people = list.people || [];
    if (people.length) {
        menu +=
            '<div class="px-3 pt-2.5 pb-1 mt-1 border-t border-line text-[10.5px] font-semibold uppercase tracking-wide text-muted">' +
                vEsc(list.can_see_all ? t("dash.team_history.members", "Team members") : t("dash.team_history.team", "Team")) +
                "</div>";
        people.forEach((p) => {
            const text = p.is_owner ? p.email + " " + t("dash.team_history.owner_tag", "(owner)") : p.email;
            menu += row(p.id, text, (p.email[0] || "?").toUpperCase(), p.is_owner);
        });
    }
    return ('<button type="button" data-vendor-who-btn class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-surface border border-line text-[12px] font-bold hover:border-brand/40 transition">' +
        vEsc(vendorWhoLabel(list, vendorWho)) +
        '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" class="text-muted"><path d="M6 9l6 6 6-6"/></svg>' +
        '</button><div id="vendor-who-menu" class="hidden absolute left-0 top-full mt-1.5 w-64 max-w-[80vw] bg-surface border border-line rounded-xl p-1.5 shadow-xl z-30">' +
        menu +
        "</div>");
}
// Search and the risk filter: rows are hidden in the page (the list is
// already loaded).
function filterVendorRows() {
    const words = vendorSearch.trim().toLowerCase();
    let shown = 0;
    document.querySelectorAll("#vendors-body tr[data-vendor-row]").forEach((tr) => {
        const el = tr;
        const okRisk = vendorRisk === "all" || el.dataset.vendorLevel === vendorRisk;
        const okText = !words || (el.dataset.vendorText || "").indexOf(words) !== -1;
        const show = okRisk && okText;
        el.classList.toggle("hidden", !show);
        if (show)
            shown++;
    });
    const none = document.getElementById("vendor-no-match");
    if (none)
        none.classList.toggle("hidden", shown > 0 || !document.querySelector("#vendors-body tr[data-vendor-row]"));
}
// One check, as a row of a scan's Details tab (detail_view.ts
// detailRow): the mark, the title, the answer under it, and the
// explanation (with "Open on ..." buttons for its links) when opened.
function vendorCardRow(kind, title, value, detail, extra = false) {
    const type = kind === "bad" || kind === "caution" ? "flag" : kind === "info" ? "info" : "ok";
    const { text, links } = splitLinks(capitalizeFirst((detail || "").trim()));
    return detailRow({
        mark: mark(type),
        title,
        value,
        moreHTML: text || links.length ? escapeHtml(text) + sourceButtons(links) : "",
        extra,
    });
}
const V_FLAGGED = (kind) => kind === "bad" || kind === "caution";
function vendorCardTitle(c) {
    const title = VENDOR_CARD_TITLES[c.key];
    return title ? t(title[0], title[1]) : c.label;
}
// What adds to the score: the same points as the backend
// (vendors.rs score): a warning +25, a "check" +8, a sanctions match
// sets 100.
const VENDOR_SANCTIONS = "Sanctions lists";
function vendorPoints(d) {
    const list = [];
    let sanctioned = false;
    if (!d.result)
        return { list, sanctioned };
    const add = (kind, title) => {
        if (kind === "bad")
            list.push({ title, points: 25, kind });
        else if (kind === "caution")
            list.push({ title, points: 8, kind });
    };
    d.result.cards.forEach((c) => add(c.type, vendorCardTitle(c)));
    d.result.records.forEach((r) => {
        if (r.type === "bad" && r.label === VENDOR_SANCTIONS)
            sanctioned = true;
        add(r.type, translateSignalLabel(r.label));
    });
    list.sort((a, b) => b.points - a.points);
    return { list, sanctioned };
}
const VENDOR_POINTS_SHOWN = 4;
// The dark score card, the same as a scan's result page, plus where
// the score sits (Low / Caution / High) and what added to it.
function vendorScoreCard(d) {
    if (!d.result || d.risk_score === null)
        return "";
    const all = [...d.result.cards.map((c) => c.type), ...d.result.records.map((r) => r.type)];
    const counts = d.result.counts;
    const flags = counts ? counts.flagged : all.filter(V_FLAGGED).length;
    const total = counts ? counts.total : all.length;
    const level = d.risk_level || "low";
    const label = level === "high"
        ? t("dash.risk.high", "High risk")
        : level === "caution"
            ? t("dash.risk.caution", "Caution")
            : t("dash.risk.low", "Low risk");
    const say = level === "high"
        ? t("dash.vendors.say_high", "Don't pay this vendor yet.")
        : level === "caution"
            ? t("dash.detail.say_caution", "Check these points before you pay.")
            : flags === 0
                ? t("dash.detail.say_low_clean", "No warning signs found.")
                : t("dash.detail.say_low", "No serious warning signs.");
    const sub = flags === 0
        ? t("dash.detail.sub_all_fine", "All {total} checks look fine.").replace("{total}", String(total))
        : flags === 1
            ? t("dash.detail.sub_one", "1 of {total} checks needs a look.").replace("{total}", String(total))
            : t("dash.detail.sub_many", "{n} of {total} checks need a look.")
                .replace("{n}", String(flags))
                .replace("{total}", String(total));
    const score = Math.max(0, Math.min(100, d.risk_score));
    const r = 42;
    const c = 2 * Math.PI * r;
    const ringColor = level === "high" ? "#ff6b5b" : level === "caution" ? "#ffb938" : "#d4d4d8";
    // Low 0-19, Caution 20-49, High 50-100, with a dot at the score.
    const zone = (cls, text, on) => '<span class="vd-zone vd-zone-' + cls + (on ? " is-on" : "") + '">' + vEsc(text) + "</span>";
    const scale = '<div class="vd-scale"><div class="vd-scale-track"><i class="vd-z-low"></i><i class="vd-z-caution"></i><i class="vd-z-high"></i>' +
        '<b style="left:' +
        score +
        '%"></b></div><div class="vd-scale-labels">' +
        zone("low", t("dash.vendors.scale_low", "Low 0–19"), level === "low") +
        zone("caution", t("dash.vendors.scale_caution", "Caution 20–49"), level === "caution") +
        zone("high", t("dash.vendors.scale_high", "High 50+"), level === "high") +
        "</div></div>";
    const { list, sanctioned } = vendorPoints(d);
    // Free: what added to the score is on paid plans.
    const locked = vendorIsFree(d)
        ? '<div class="vd-points"><div class="vd-point is-none"><span>' +
            vEsc(t("dash.detail.score_locked", "See what adds to the score on the Team and Enterprise plans.")) +
            "</span></div></div>"
        : "";
    let points = '<div class="vd-points"><div class="vd-points-title">' + vEsc(t("dash.vendors.score_adds", "What adds to the score")) + "</div>";
    if (sanctioned) {
        points +=
            '<div class="vd-point"><i class="vd-dot is-bad"></i><span>' +
                vEsc(t("dash.vendors.score_sanctions", "On a sanctions list: the score is 100.")) +
                "</span><b>100</b></div>";
    }
    if (!list.length && !sanctioned) {
        points += '<div class="vd-point is-none"><span>' + vEsc(t("dash.vendors.score_nothing", "Nothing added to the score.")) + "</span></div>";
    }
    list.slice(0, VENDOR_POINTS_SHOWN).forEach((p) => {
        points +=
            '<div class="vd-point"><i class="vd-dot ' +
                (p.kind === "bad" ? "is-bad" : "is-caution") +
                '"></i><span>' +
                vEsc(p.title) +
                "</span><b>+" +
                p.points +
                "</b></div>";
    });
    if (list.length > VENDOR_POINTS_SHOWN) {
        points +=
            '<div class="vd-point is-none"><span>' +
                vEsc(t("dash.vendors.score_more", "+{n} more in the list below").replace("{n}", String(list.length - VENDOR_POINTS_SHOWN))) +
                "</span></div>";
    }
    points += "</div>";
    return ('<div class="verdict-card vd-verdict"><div class="vd-verdict-top"><div class="verdict-ring"><svg viewBox="0 0 100 100">' +
        '<circle cx="50" cy="50" r="' +
        r +
        '" fill="none" stroke="var(--card-track)" stroke-width="8"/>' +
        '<circle class="verdict-ring-arc" cx="50" cy="50" r="' +
        r +
        '" fill="none" stroke="' +
        ringColor +
        '" stroke-width="8" stroke-linecap="round" stroke-dasharray="' +
        ((c * score) / 100).toFixed(1) +
        " " +
        c.toFixed(1) +
        '"/></svg><div class="verdict-number">' +
        score +
        '</div></div><div><div class="verdict-level' +
        (level === "low" ? " is-calm" : level === "high" ? " is-high" : "") +
        '">' +
        vEsc(label) +
        '</div><div class="verdict-say">' +
        vEsc(say) +
        '</div><div class="verdict-sub">' +
        vEsc(sub) +
        "</div></div></div>" +
        scale +
        (locked || points) +
        "</div>");
}
// Free plan: the short page (score, level, top 3 reasons) - no
// actions, answers, files or full details.
const vendorIsFree = (d) => d.full_result === false;
const V_PANEL = "mt-4 bg-surface border border-line rounded-2xl p-4";
const V_PANEL_TITLE = "text-[11.5px] font-bold uppercase tracking-wide text-muted";
// "Your decision": Approve / Reject / Ask for more info, who decided
// and the note. The note boxes open inside it. Only once there is a
// result.
function vendorDecisionPanel(d) {
    if (!d.result)
        return "";
    const b = (attr, text, dark = false) => '<button type="button" ' + attr + ' class="' + (dark ? V_BTN_DARK : V_BTN) + '">' + vEsc(text) + "</button>";
    const reviewed = ["ready", "approved", "rejected"].includes(d.status);
    const decided = !!d.decided_on && (d.status === "approved" || d.status === "rejected");
    const approved = d.status === "approved";
    let html = '<div id="vendor-decision" class="' + V_PANEL + '"><div class="' + V_PANEL_TITLE + '">' + vEsc(t("dash.vendors.your_decision", "Your decision")) + "</div>";
    if (decided) {
        html +=
            '<div class="mt-3 flex items-start gap-3"><span class="w-7 h-7 rounded-full flex-shrink-0 flex items-center justify-center text-[13px] font-bold ' +
                (approved ? "bg-ink text-bg" : V_BAD) +
                '">' +
                (approved ? "✓" : "✕") +
                '</span><div class="min-w-0"><div class="text-[14px] font-bold">' +
                vEsc(approved ? t("dash.vendors.status.approved", "Approved") : t("dash.vendors.status.rejected", "Rejected")) +
                '</div><div class="text-[12.5px] text-muted mt-0.5">' +
                vEsc(t("dash.vendors.decided", "Decided by {who} on {date}")
                    .replace("{who}", d.decided_by || "—")
                    .replace("{date}", d.decided_on || "")) +
                "</div>" +
                (d.decision_note ? '<div class="text-[13px] mt-1.5 break-words">“' + vEsc(d.decision_note) + "”</div>" : "") +
                "</div></div>";
    }
    else if (reviewed) {
        html +=
            '<div class="mt-2 text-[12.5px] text-muted">' +
                vEsc(t("dash.vendors.not_decided", "Not decided yet. Your team sees your decision and note; the vendor never does.")) +
                "</div>";
    }
    else if (d.status === "more_info") {
        html +=
            '<div class="mt-2 text-[12.5px] text-muted">' +
                vEsc(t("dash.vendors.waiting_fix", "Waiting for the vendor to fix some answers.")) +
                "</div>";
    }
    // Free: the decision is shown, but deciding needs a paid plan.
    if (vendorIsFree(d))
        return html + "</div>";
    const buttons = [];
    if (reviewed) {
        if (d.status !== "approved")
            buttons.push(b('data-vendor-decide="approve"', t("dash.vendors.approve", "Approve"), true));
        if (d.status !== "rejected")
            buttons.push(b('data-vendor-decide="reject"', t("dash.vendors.reject", "Reject")));
    }
    if (reviewed || d.status === "more_info")
        buttons.push(b("data-vendor-ask-open", t("dash.vendors.ask", "Ask for more info")));
    if (buttons.length)
        html += '<div data-vendor-actions class="flex flex-wrap gap-2 mt-3.5">' + buttons.join("") + "</div>";
    html += buildDecideBox();
    html += buildAskBox();
    // After a result, a new link opens the bank details only.
    if (reviewed) {
        html +=
            '<div class="mt-4 pt-3.5 border-t border-line flex flex-wrap items-center justify-between gap-2"><span class="text-[12.5px] text-muted">' +
                vEsc(t("dash.vendors.bank_hint", "Bank details changed? Send the vendor a new link for those only.")) +
                '</span><button type="button" data-vendor-bank class="' +
                V_BTN +
                '">' +
                vEsc(t("dash.vendors.new_link", "New link")) +
                "</button></div>";
    }
    return html + "</div>";
}
// "Form link": while the vendor can still fill the form (or the link
// closed): reminder, new link, cancel. Also "Run the check now".
function vendorLinkPanel(d) {
    const b = (attr, text, dark = false) => '<button type="button" ' + attr + ' class="' + (dark ? V_BTN_DARK : V_BTN) + '">' + vEsc(text) + "</button>";
    const linkOpen = ["invited", "opened", "more_info"].includes(d.status);
    const closed = ["expired", "canceled"].includes(d.status);
    const waitingScans = d.status === "no_scans" || d.status === "checking";
    if (vendorIsFree(d) || (!linkOpen && !closed && !waitingScans))
        return "";
    let html = '<div class="' + V_PANEL + '"><div class="' + V_PANEL_TITLE + '">' + vEsc(waitingScans ? t("dash.vendors.check_title", "The check") : t("dash.vendors.form_link", "Form link")) + "</div>";
    const buttons = [];
    if (linkOpen) {
        html +=
            '<div class="mt-2 text-[12.5px] text-muted">' +
                vEsc(t("dash.vendors.waiting_note", "The link works until {date}. Only you can see the result: the vendor never does.").replace("{date}", d.expires_on)) +
                (d.ask_fields.length
                    ? "<br />" + vEsc(t("dash.vendors.asked_for", "Asked to fix: {list}").replace("{list}", d.ask_fields.map(vendorFieldName).join(", ")))
                    : "") +
                "</div>";
        if (d.send_email)
            buttons.push(b("data-vendor-remind", t("dash.vendors.remind", "Send a reminder"), true));
        buttons.push(b("data-vendor-link", t("dash.vendors.new_link", "New link")));
        buttons.push(b("data-vendor-cancel", t("dash.vendors.cancel", "Cancel the link")));
    }
    else if (closed) {
        html +=
            '<div class="mt-2 text-[12.5px] text-muted">' +
                vEsc(t("dash.vendors.link_closed", "This link is closed. Send a new one so the vendor can fill in the form.")) +
                "</div>";
        buttons.push(b("data-vendor-link", t("dash.vendors.new_link", "New link"), true));
    }
    else {
        html +=
            '<div class="mt-2 rounded-xl bg-[#fef3c7] text-[#92400e] p-3 text-[12.5px]">' +
                vEsc(d.status === "no_scans"
                    ? t("dash.vendors.no_scans_note", "The vendor sent the form, but your plan has no scans left. The check runs by itself when your scans renew.")
                    : t("dash.vendors.checking_note", "The vendor sent the form. Safely is checking the answers.")) +
                "</div>";
        buttons.push(b("data-vendor-retry", t("dash.vendors.retry", "Run the check now"), true));
    }
    html += '<div data-vendor-actions class="flex flex-wrap gap-2 mt-3.5">' + buttons.join("") + "</div>";
    html += buildCancelBox();
    return html + "</div>";
}
// The vendor's website, only when they gave one (or the buyer did).
function vendorWebsite(d) {
    const given = d.answers.find((a) => a.field === "website")?.value || (d.known && d.known.website) || "";
    const raw = given.trim();
    if (!raw)
        return null;
    try {
        const url = new URL(/^https?:\/\//i.test(raw) ? raw : "https://" + raw);
        if ((url.protocol !== "https:" && url.protocol !== "http:") || url.hostname.indexOf(".") === -1)
            return null;
        return url.href;
    }
    catch (e) {
        return null;
    }
}
const V_TOOL = "inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-[12px] font-bold transition flex-shrink-0";
const V_TOOL_LIGHT = V_TOOL + " bg-surface2 border-line hover:bg-surface3";
const V_ICON_OUT = '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 17L17 7"/><path d="M8 7h9v9"/></svg>';
const V_ICON_FLAG = '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 22V4"/><path d="M4 4h12l-2 4 2 4H4"/></svg>';
const V_ICON_PDF = '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3v12m0 0l-4-4m4 4l4-4"/><path d="M4 17v3a2 2 0 002 2h12a2 2 0 002-2v-3"/></svg>';
// The buttons at the top right, like a scan's page: Visit website,
// Watch this supplier, Report as scam, PDF.
function vendorToolbar(d) {
    // Free: no actions (watch, report, PDF are on paid plans).
    if (vendorIsFree(d))
        return "";
    const parts = [];
    const site = vendorWebsite(d);
    if (site) {
        parts.push('<a data-vendor-site href="' +
            vEsc(site) +
            '" title="' +
            vEsc(t("dash.vendors.visit_website", "Visit website")) +
            '" target="_blank" rel="noopener noreferrer" class="' +
            V_TOOL_LIGHT +
            '"><span class="hidden sm:inline">' +
            vEsc(t("dash.vendors.visit_website", "Visit website")) +
            "</span>" +
            V_ICON_OUT +
            "</a>");
    }
    if (d.result) {
        parts.push(d.watching
            ? '<span data-vendor-watching class="' +
                V_TOOL_LIGHT +
                '">' +
                WATCH_CHECK +
                '<span class="hidden sm:inline">' +
                vEsc(t("dash.watch.watching_btn", "Watching")) +
                "</span></span>"
            : '<button type="button" data-vendor-watch title="' +
                vEsc(t("dash.vendors.watch", "Watch this supplier")) +
                '" class="' +
                V_TOOL +
                ' bg-ink text-bg border-ink hover:opacity-90">' +
                WATCH_EYE +
                '<span class="hidden sm:inline">' +
                vEsc(t("dash.vendors.watch", "Watch this supplier")) +
                "</span></button>");
    }
    if (d.sent_on) {
        parts.push(d.reported_by_you
            ? '<span class="' +
                V_TOOL +
                ' bg-coral/10 border-coral/20 text-coral">' +
                V_ICON_FLAG +
                '<span class="hidden sm:inline">' +
                vEsc(t("dash.vendors.reported", "Reported by you")) +
                "</span></span>"
            : '<button type="button" data-vendor-report-open title="' +
                vEsc(t("dash.vendors.report", "Report as scam")) +
                '" class="' +
                V_TOOL +
                ' bg-surface border-coral/40 text-coral hover:bg-coral/10">' +
                V_ICON_FLAG +
                '<span class="hidden sm:inline">' +
                vEsc(t("dash.vendors.report", "Report as scam")) +
                "</span></button>");
    }
    if (d.result) {
        parts.push('<button type="button" data-vendor-pdf class="' + V_TOOL_LIGHT + '">' + V_ICON_PDF + "<span>PDF</span></button>");
    }
    // Phones: on their own row under the name.
    return parts.length ? '<div class="flex items-center gap-2 flex-wrap w-full sm:w-auto">' + parts.join("") + "</div>" : "";
}
// The side card: the vendor's main facts, like a scan's "Supplier".
function vendorFacts(d) {
    const answer = (field) => (d.answers.find((a) => a.field === field)?.value || "").trim();
    const facts = [];
    const number = answer("company_number");
    if (number)
        facts.push(fact(t("dash.vendors.field.company_number", "Company number"), number));
    facts.push(fact(t("dash.detail.reports_on_safely", "Reports on Safely"), String(d.report_count || 0)));
    const email = answer("email") || d.vendor_email;
    facts.push(fact(d.email_confirmed && d.email_confirmed === email
        ? t("dash.vendors.email_confirmed", "Email (confirmed)")
        : t("dash.vendors.field.email", "Work email"), email));
    const phone = answer("phone");
    if (phone)
        facts.push(fact(t("dash.detail.phone", "Phone"), phone));
    const site = vendorWebsite(d);
    if (site)
        facts.push(fact(t("dash.vendors.field.website", "Website"), new URL(site).hostname.replace(/^www\./, "")));
    facts.push(fact(t("dash.vendors.invited_by", "Invited by"), d.invited_by_you ? t("dash.vendors.you", "You") : d.invited_by || "—"));
    if (d.sent_on)
        facts.push(fact(t("dash.vendors.form_sent", "Form sent"), d.sent_on));
    else
        facts.push(fact(t("dash.vendors.link_until", "Link works until"), d.expires_on));
    if (d.checked_on)
        facts.push(fact(t("dash.vendors.checked", "Checked"), d.checked_on));
    const country = answer("country");
    const address = answer("address");
    return ('<div class="text-[13px] font-bold mb-2.5">' +
        vEsc(t("dash.vendors.vendor", "Vendor")) +
        '</div><div class="bg-surface border border-line rounded-2xl p-5"><div class="text-[16px] font-bold leading-snug break-words">' +
        vEsc(d.vendor_name) +
        "</div>" +
        (country || address
            ? '<div class="text-[13px] text-muted mt-0.5 break-words">' + vEsc([address, country].filter(Boolean).join(", ")) + "</div>"
            : "") +
        (d.invited_as && d.invited_as !== d.vendor_name
            ? '<div class="text-[12px] text-muted mt-0.5 break-words">' +
                vEsc(t("dash.vendors.invited_as", "Invited as {name}").replace("{name}", d.invited_as)) +
                "</div>"
            : "") +
        '<div class="grid grid-cols-2 gap-x-3 gap-y-3.5 mt-4 pt-4 border-t border-line">' +
        facts.join("") +
        "</div></div>");
}
// The web search's rows, as a scan saves them. Older vendor results
// only kept the pages found: those are turned into the same rows.
function vendorSocialRows(d) {
    const r = d.result;
    if (!r)
        return [];
    if (r.social_candidates && r.social_candidates.length)
        return r.social_candidates;
    const links = r.online || [];
    const sites = r.presence || Array.from(new Set(links.map((l) => l.platform))).map((platform) => ({ platform, found: true }));
    return sites.map((site) => ({
        platform: site.platform,
        variant_searched: "",
        found: site.found,
        candidates: links.filter((l) => l.platform === site.platform),
    }));
}
// The files the vendor uploaded, each with a download button.
function vendorDocuments(d) {
    const docs = d.documents || [];
    if (!docs.length)
        return "";
    return ('<div class="d-heading">' +
        vEsc(t("dash.vendors.documents", "Documents")) +
        '</div><div class="d-rows">' +
        docs
            .map((doc) => {
            const kind = VENDOR_DOC_KINDS[doc.kind];
            return ('<div class="d-row"><div class="d-row-head">' +
                mark("info") +
                '<span class="d-title">' +
                vEsc(kind ? t(kind[0], kind[1]) : doc.kind) +
                "<small>" +
                vEsc(doc.file_name + " · " + Math.max(1, Math.round(doc.size / 1024)) + " KB · " + doc.uploaded_on) +
                '</small></span><button type="button" data-vendor-doc="' +
                vEsc(doc.id) +
                '" data-vendor-doc-name="' +
                vEsc(doc.file_name) +
                '" class="' +
                V_BTN +
                ' flex-shrink-0">' +
                vEsc(t("dash.vendors.download", "Download")) +
                "</button></div></div>");
        })
            .join("") +
        "</div>");
}
// Report this vendor (marketplace sellers are reported from the
// extension; vendors here). Shows in My Reports.
function buildReportBox() {
    return ('<div id="vendor-report" class="hidden mt-4 bg-surface border border-coral/30 rounded-2xl p-4">' +
        '<div class="text-[13.5px] font-bold mb-1">' +
        vEsc(t("dash.vendors.report_title", "Report this vendor as a scam")) +
        '</div><div class="text-[12px] text-muted mb-3">' +
        vEsc(t("dash.vendors.report_text", "Your report warns other Safely users about this company. Your team sees it's you; others only see the number of reports.")) +
        '</div><div class="flex flex-col gap-2">' +
        VENDOR_REPORT_REASONS.map((reason, i) => '<label class="flex items-center gap-2 text-[13px]"><input type="radio" name="vendor-report-reason" value="' +
            reason +
            '"' +
            (i === 0 ? " checked" : "") +
            " /> " +
            vEsc(translateReportReason(reason)) +
            "</label>").join("") +
        '</div><textarea id="vendor-report-note" rows="3" class="mt-3 w-full p-3 rounded-lg border border-line bg-surface text-[13px] outline-none focus:border-ink" placeholder="' +
        vEsc(t("dash.vendors.report_note", "What happened? (optional)")) +
        '"></textarea><div class="flex gap-2 mt-3"><button type="button" data-vendor-report-send class="px-3.5 py-2 rounded-lg bg-coral text-white text-[12.5px] font-bold hover:opacity-90 transition">' +
        vEsc(t("dash.vendors.report_send", "Send report")) +
        '</button><button type="button" data-vendor-report-close class="' +
        V_BTN +
        '">' +
        vEsc(t("dash.common.cancel", "Cancel")) +
        "</button></div></div>");
}
function vRows(title, rows) {
    if (!rows.length)
        return "";
    return '<div class="d-heading">' + vEsc(title) + '</div><div class="d-rows">' + rows.join("") + "</div>";
}
// One vendor: the score, the decision, the buttons, the checks and the
// answers.
function buildVendorDetail(d) {
    const initial = (d.vendor_name.trim()[0] || "?").toUpperCase();
    let html = '<button type="button" data-vendor-back class="text-[12.5px] font-bold text-muted hover:text-ink mb-4">← ' +
        vEsc(t("dash.vendors.back", "All vendors")) +
        "</button>" +
        '<div class="flex flex-wrap items-start gap-x-3.5 gap-y-3">' +
        '<span class="w-11 h-11 rounded-xl bg-[#ffd86a] text-[#17171a] flex-shrink-0 flex items-center justify-center text-[17px] font-extrabold">' +
        vEsc(initial) +
        '</span><div class="min-w-0 flex-1 basis-52"><div class="flex flex-wrap items-center gap-2">' +
        '<span data-vendor-title class="text-[20px] font-bold tracking-tight break-words">' +
        vEsc(d.vendor_name) +
        "</span>" +
        vendorStatusPill(d.status) +
        '</div><div class="text-[12.5px] text-muted mt-1 break-words">' +
        vEsc(d.vendor_email) +
        " · " +
        vEsc(t("dash.vendors.invited_on", "Invited {date}").replace("{date}", d.invited_on)) +
        (d.sent_on ? " · " + vEsc(t("dash.vendors.sent_on", "Sent {date}").replace("{date}", d.sent_on)) : "") +
        "</div></div>" +
        vendorToolbar(d) +
        "</div>";
    html += buildReportBox();
    // Like a scan's Risk tab: the score and the decision on the left,
    // the vendor on the right (one column on phones).
    html +=
        '<div class="grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_minmax(0,360px)] gap-6 items-start mt-5"><div class="min-w-0">' +
            vendorScoreCard(d) +
            vendorDecisionPanel(d) +
            vendorLinkPanel(d) +
            '<div id="vendor-detail-link" class="mt-3"></div></div><div class="min-w-0">' +
            vendorFacts(d) +
            "</div></div>";
    // Free: the top 3 reasons and a card to see the plans; nothing else.
    if (vendorIsFree(d)) {
        if (d.result && d.result.cards.length) {
            html +=
                '<div class="d-heading">' +
                    vEsc(t("dash.detail.what_we_found", "What we found")) +
                    '</div><div class="d-rows">' +
                    d.result.cards.map((c) => vendorCardRow(c.type, vendorCardTitle(c), c.value, c.detail)).join("") +
                    "</div>";
        }
        return (html +
            '<div class="mt-5">' +
            detailUnlockCard(t("dash.vendors.full_title", "See the full vendor check"), t("dash.vendors.full_text", "All the checks with their reasons, outside records, online presence, the vendor's answers and files, decisions, new links and the PDF are on the Team and Enterprise plans.")) +
            "</div>");
    }
    if (d.result) {
        const order = (k) => (k === "bad" ? 0 : k === "caution" ? 1 : k === "good" ? 2 : 3);
        const cards = d.result.cards.slice().sort((x, y) => order(x.type) - order(y.type));
        const flagged = cards.filter((c) => V_FLAGGED(c.type));
        const fine = cards.filter((c) => !V_FLAGGED(c.type));
        // Outside records that are warnings also count in "need a look".
        const outsideFlags = d.result.records.filter((r) => V_FLAGGED(r.type)).length;
        html +=
            '<div class="d-counts mt-6"><div class="d-count' +
                (flagged.length + outsideFlags > 0 ? " stat-hot" : "") +
                '"><b>' +
                (flagged.length + outsideFlags) +
                "</b><small>" +
                vEsc(flagged.length + outsideFlags === 1
                    ? t("dash.detail.needs_a_look_count", "needs a look")
                    : t("dash.detail.need_a_look_count", "need a look")) +
                '</small></div><div class="d-count"><b>' +
                (fine.length + d.result.records.length - outsideFlags) +
                "</b><small>" +
                vEsc(t("dash.detail.look_fine_count", "look fine")) +
                "</small></div></div>";
        if (flagged.length) {
            html +=
                '<div class="d-heading">' +
                    vEsc(t("dash.detail.needs_a_look", "Needs a look")) +
                    '</div><div class="d-rows">' +
                    flagged.map((c) => vendorCardRow(c.type, vendorCardTitle(c), c.value, c.detail)).join("") +
                    "</div>";
        }
        if (fine.length) {
            html +=
                '<div class="d-heading">' +
                    vEsc(t("dash.detail.looks_fine", "Looks fine")) +
                    '</div><div class="d-rows">' +
                    fine.map((c, i) => vendorCardRow(c.type, vendorCardTitle(c), c.value, c.detail, i >= FIRST_FINE)).join("") +
                    moreButton(fine.length - FIRST_FINE) +
                    "</div>";
        }
        // Outside records, Online presence and Scam warnings online: drawn
        // by the same code as a scan's Details tab.
        html += buildOutsideRecords(d.result.records);
        const social = vendorSocialRows(d);
        html += buildPresence(social.filter((r) => WEB_SCAM_ROWS.indexOf(r.platform) === -1));
        html += buildScamWarnings(social.filter((r) => WEB_SCAM_ROWS.indexOf(r.platform) !== -1));
        html +=
            '<div class="d-heading">' +
                vEsc(t("dash.detail.before_you_pay", "Before you pay")) +
                '</div><div class="d-rows">' +
                VENDOR_PAY_CHECKS.map((id, i) => {
                    const [titleKey, title, textKey, text] = PAY_CHECKS[id];
                    return detailRow({
                        mark: '<span class="d-mark d-mark-none"></span>',
                        title: t(titleKey, title),
                        moreHTML: escapeHtml(t(textKey, text)),
                        extra: i >= FIRST_STEPS,
                    });
                }).join("") +
                moreButton(VENDOR_PAY_CHECKS.length - FIRST_STEPS) +
                "</div>";
    }
    html += vendorDocuments(d);
    const answers = d.answers
        .map((a) => '<div class="px-4 py-3 bg-surface min-w-0"><div class="text-[11.5px] text-muted">' +
        vEsc(vendorFieldName(a.field)) +
        '</div><div class="text-[13.5px] font-semibold mt-0.5 break-words">' +
        vEsc(a.value || "—") +
        (a.field === "email" && a.value && d.email_confirmed === a.value
            ? ' <span class="text-[11px] font-bold text-muted">✓ ' + vEsc(t("dash.vendors.confirmed", "confirmed")) + "</span>"
            : "") +
        "</div></div>")
        .join("");
    html +=
        '<div class="d-heading">' +
            vEsc(t("dash.vendors.answers", "The vendor's answers")) +
            '</div><div class="bg-line border border-line rounded-2xl overflow-hidden grid sm:grid-cols-2 gap-px">' +
            answers +
            (d.answers.length % 2 ? '<div class="hidden sm:block bg-surface"></div>' : "") +
            "</div>";
    if (d.previous && d.previous.length) {
        const rows = d.previous
            .slice()
            .reverse()
            .map((p) => {
            const changed = (p.asked || [])
                .map((f) => vendorFieldName(f) + ": " + (p.answers[f] || "—"))
                .join(" · ");
            return ('<div class="px-4 py-2.5 border-b border-line last:border-0 text-[12.5px]"><span class="text-muted">' +
                vEsc(p.sent_on || "") +
                "</span> " +
                vEsc(t("dash.vendors.before", "Before:")) +
                " " +
                vEsc(changed) +
                "</div>");
        })
            .join("");
        html += vSection(t("dash.vendors.history", "Earlier answers"), rows);
    }
    return html;
}
function vSection(title, inner) {
    return ('<div class="mt-6"><div class="text-[12px] font-bold uppercase tracking-wide text-muted mb-2">' +
        vEsc(title) +
        '</div><div class="bg-surface border border-line rounded-2xl overflow-hidden">' +
        inner +
        "</div></div>");
}
// Approve / Reject: a box under the buttons with a note for the team
// (saved with the decision; the whole team sees it, the vendor never).
function buildDecideBox() {
    return ('<div id="vendor-decide" class="hidden mt-3.5 bg-surface2 rounded-xl p-4">' +
        '<div id="vendor-decide-title" class="text-[13.5px] font-bold mb-1"></div>' +
        '<div class="text-[12px] text-muted mb-3">' +
        vEsc(t("dash.vendors.decide_text", "Your team sees this note with your name and the date. The vendor never sees it.")) +
        '</div><textarea id="vendor-decide-note" rows="2" class="w-full p-3 rounded-lg border border-line bg-surface text-[13px] outline-none focus:border-ink" placeholder="' +
        vEsc(t("dash.vendors.decision_note", "A short note for your team (optional)")) +
        '"></textarea><div class="flex gap-2 mt-3"><button type="button" data-vendor-decide-send class="' +
        V_BTN_DARK +
        '"></button><button type="button" data-vendor-decide-close class="' +
        V_BTN +
        '">' +
        vEsc(t("dash.common.cancel", "Cancel")) +
        "</button></div></div>");
}
// "Cancel the link": asked in the page, not in a browser pop-up.
function buildCancelBox() {
    return ('<div id="vendor-cancel-box" class="hidden mt-3.5 bg-surface2 rounded-xl p-4">' +
        '<div class="text-[13.5px] font-bold mb-1">' +
        vEsc(t("dash.vendors.cancel_title", "Cancel this link?")) +
        '</div><div class="text-[12px] text-muted mb-3">' +
        vEsc(t("dash.vendors.cancel_confirm", "Cancel this link? The vendor will not be able to open the form.")) +
        '</div><div class="flex gap-2"><button type="button" data-vendor-cancel-send class="px-3.5 py-2 rounded-lg bg-coral text-white text-[12.5px] font-bold hover:opacity-90 transition">' +
        vEsc(t("dash.vendors.cancel", "Cancel the link")) +
        '</button><button type="button" data-vendor-cancel-close class="' +
        V_BTN +
        '">' +
        vEsc(t("dash.vendors.keep_link", "Keep it")) +
        "</button></div></div>");
}
// Only one box open at a time.
function openVendorBox(id) {
    ["vendor-decide", "vendor-ask", "vendor-report", "vendor-cancel-box"].forEach((box) => {
        const el = document.getElementById(box);
        if (el && box !== id)
            el.classList.add("hidden");
    });
    if (id)
        document.getElementById(id)?.classList.toggle("hidden");
}
function buildAskBox() {
    return ('<div id="vendor-ask" class="hidden mt-3.5 bg-surface2 rounded-xl p-4">' +
        '<div class="text-[13.5px] font-bold mb-2">' +
        vEsc(t("dash.vendors.ask_title", "Which answers should the vendor fix?")) +
        '</div><div class="flex flex-wrap gap-x-4 gap-y-2">' +
        VENDOR_FIELDS.map(([field, key, fallback]) => '<label class="flex items-center gap-1.5 text-[12.5px]"><input type="checkbox" data-ask-field="' +
            field +
            '" /> ' +
            vEsc(t(key, fallback)) +
            "</label>").join("") +
        "</div>" +
        '<input id="vendor-ask-note" class="' +
        V_INPUT +
        ' mt-3" placeholder="' +
        vEsc(t("dash.vendors.ask_note", "A short note for the vendor (optional)")) +
        '" /><div class="flex gap-2 mt-3"><button type="button" data-vendor-ask-send class="' +
        V_BTN_DARK +
        '">' +
        vEsc(t("dash.vendors.ask_send", "Send the link")) +
        '</button><button type="button" data-vendor-ask-close class="' +
        V_BTN +
        '">' +
        vEsc(t("dash.common.cancel", "Cancel")) +
        "</button></div></div>");
}
// A list of vendors from CSV text: name, email, country, phone, website.
function parseVendorCsv(text) {
    const rows = [];
    let row = [];
    let field = "";
    let quoted = false;
    for (let i = 0; i < text.length; i++) {
        const c = text[i];
        if (quoted) {
            if (c === '"' && text[i + 1] === '"') {
                field += '"';
                i++;
            }
            else if (c === '"')
                quoted = false;
            else
                field += c;
        }
        else if (c === '"')
            quoted = true;
        else if (c === "," || c === ";") {
            row.push(field);
            field = "";
        }
        else if (c === "\n" || c === "\r") {
            if (c === "\r" && text[i + 1] === "\n")
                i++;
            row.push(field);
            rows.push(row);
            row = [];
            field = "";
        }
        else
            field += c;
    }
    row.push(field);
    rows.push(row);
    const clean = rows.map((r) => r.map((f) => f.trim())).filter((r) => r.some((f) => f));
    if (!clean.length)
        return [];
    const header = clean[0].map((h) => h.toLowerCase());
    const hasHeader = header.includes("email") || header.includes("e-mail");
    const col = (names, fallback) => {
        const found = header.findIndex((h) => names.includes(h));
        return hasHeader ? found : fallback;
    };
    const idx = {
        name: col(["name", "company", "company name", "vendor", "nome", "empresa"], 0),
        email: col(["email", "e-mail"], 1),
        country: col(["country", "país", "pais"], 2),
        phone: col(["phone", "telefone", "whatsapp"], 3),
        website: col(["website", "site", "web"], 4),
    };
    const pick = (r, i) => (i >= 0 && r[i] ? r[i] : undefined);
    return clean
        .slice(hasHeader ? 1 : 0)
        .map((r) => ({
        name: pick(r, idx.name) || "",
        email: pick(r, idx.email) || "",
        country: pick(r, idx.country),
        phone: pick(r, idx.phone),
        website: pick(r, idx.website),
    }))
        .filter((v) => v.name || v.email);
}
function vendorError(code) {
    switch (code) {
        case "needs_paid_plan":
            return t("dash.vendors.err_plan", "Vendor checks are on the Team and Enterprise plans.");
        case "buyer_company_missing":
            return t("dash.vendors.err_buyer", "Please write your company name.");
        case "bad_email":
            return t("dash.vendors.err_email", "One of the email addresses does not look right.");
        case "name_missing":
            return t("dash.vendors.err_name", "Please write the vendor's company name.");
        case "too_many_vendors":
            return t("dash.vendors.err_too_many", "Up to 200 vendors at a time.");
        case "no_vendors":
            return t("dash.vendors.err_none", "Add at least one vendor.");
        case "no_fields":
            return t("dash.vendors.err_fields", "Choose at least one answer to fix.");
        case "email_limit":
            return t("dash.vendors.err_email_limit", "This address already got the most Safely emails for today. Send the link yourself, or try tomorrow.");
        case "already_reported":
            return t("dash.vendors.err_already_reported", "You already reported this vendor.");
        case "watch_limit_reached":
            return t("dash.vendors.err_watch_limit", "Your watch list is full.");
        default:
            return t("dash.watch.failed", "Could not change this right now. Please try again.");
    }
}
async function vendorApi(path, body) {
    const auth = window.safelyAuth;
    const res = await fetch(API_BASE + "/vendors" + path, {
        method: body === undefined ? "GET" : "POST",
        headers: body === undefined ? auth.authHeader() : { ...auth.authHeader(), "Content-Type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
    });
    let json = {};
    try {
        json = await res.json();
    }
    catch (e) {
        json = {};
    }
    if (!res.ok) {
        const err = new Error(json.error || "failed");
        err.code = json.error;
        throw err;
    }
    return json;
}
// Back to the list, at /vendors.
function showVendorList() {
    if (window.location.pathname !== "/vendors")
        history.pushState(null, "", "/vendors");
    loadVendors();
}
async function loadVendors() {
    const body = document.getElementById("vendors-body");
    if (!body)
        return;
    const auth = window.safelyAuth;
    if (!auth || !auth.getToken())
        return;
    vendorOpenId = null;
    vendorsAddressShown = "/vendors";
    try {
        vendorList = (await vendorApi(vendorWho === "me" ? "" : "?who=" + encodeURIComponent(vendorWho)));
        // A choice that is no longer allowed (left the team...): back to mine.
        if (vendorList.who)
            vendorWho = vendorList.who;
        body.innerHTML = buildVendorsPage(vendorList);
        filterVendorRows();
    }
    catch (e) {
        console.error("Safely: failed to load the vendors", e);
        body.innerHTML =
            '<div class="text-center text-muted text-sm py-16">' +
                vEsc(t("dash.watch.load_failed", "Could not load the list. Please refresh the page.")) +
                "</div>";
    }
}
// The address the Vendors page shows now ("/vendors" or
// "/vendors/<id>"), so the same address is not loaded twice.
let vendorsAddressShown = "";
// Shows the list or one vendor, from the address (a refresh, a shared
// link, Back / Forward). Old links /vendors?open=<id> still work.
function routeVendorsFromAddress() {
    const old = new URLSearchParams(window.location.search).get("open");
    if (old && /^[0-9a-fA-F-]{36}$/.test(old)) {
        history.replaceState(null, "", "/vendors/" + old);
    }
    const path = window.location.pathname.replace(/\/+$/, "") || "/vendors";
    if (path === vendorsAddressShown)
        return;
    const id = vendorIdFromAddress(path);
    if (id)
        openVendor(id, false);
    else
        loadVendors();
}
// One vendor's page, at its own address (/vendors/<id>). `push`: add
// the address to the browser's history (not when following Back).
async function openVendor(id, push = true) {
    const body = document.getElementById("vendors-body");
    if (!body)
        return;
    vendorOpenId = id;
    const address = "/vendors/" + id;
    vendorsAddressShown = address;
    if (push && window.location.pathname !== address)
        history.pushState(null, "", address);
    try {
        const d = (await vendorApi("/" + encodeURIComponent(id)));
        body.innerHTML = buildVendorDetail(d);
        window.scrollTo(0, 0);
    }
    catch (e) {
        console.error("Safely: failed to open the vendor", e);
        showToast(t("dash.watch.failed", "Could not change this right now. Please try again."));
        showVendorList();
    }
}
// Runs one action on the open vendor, then shows it again.
async function vendorAction(path, body, done) {
    if (!vendorOpenId)
        return null;
    const id = vendorOpenId;
    try {
        const res = await vendorApi("/" + encodeURIComponent(id) + path, body);
        if (done)
            showToast(done);
        await openVendor(id);
        return res;
    }
    catch (e) {
        showToast(vendorError(e && e.code));
        return null;
    }
}
function showVendorLink(link) {
    const box = document.getElementById("vendor-detail-link");
    if (box) {
        box.innerHTML =
            '<div class="bg-surface border border-line rounded-2xl px-4 py-1">' +
                vendorLinkBox(t("dash.vendors.new_link_ready", "New link"), link) +
                "</div>";
    }
}
async function sendInvites() {
    const val = (id) => (document.getElementById(id)?.value || "").trim();
    const listMode = !document.querySelector('[data-vendor-panel="list"]')?.classList.contains("hidden");
    const vendors = listMode
        ? parseVendorCsv(val("vi-csv"))
        : [
            {
                name: val("vi-name"),
                email: val("vi-email"),
                country: val("vi-country") || undefined,
                phone: val("vi-phone") || undefined,
                website: val("vi-website") || undefined,
            },
        ];
    const result = document.getElementById("vi-result");
    const button = document.querySelector("[data-vendor-send]");
    if (button)
        button.disabled = true;
    try {
        const res = await vendorApi("/invite", {
            buyer_company: val("vi-buyer"),
            language: val("vi-language") || "en",
            send_email: document.getElementById("vi-send")?.checked !== false,
            vendors,
        });
        if (result)
            result.innerHTML = buildInviteResult(res.invited || [], vendors);
        const listBody = await vendorApi("");
        vendorList = listBody;
        // Keep the invite box (with the links) open; refresh the rest.
        const page = document.getElementById("vendors-body");
        const invite = document.getElementById("vendor-invite");
        if (page && invite) {
            const keep = invite.outerHTML;
            page.innerHTML = buildVendorsPage(vendorList);
            const fresh = document.getElementById("vendor-invite");
            if (fresh)
                fresh.outerHTML = keep;
            filterVendorRows();
        }
    }
    catch (e) {
        if (result)
            result.innerHTML = '<div class="text-[12.5px] font-semibold text-coral">' + vEsc(vendorError(e && e.code)) + "</div>";
    }
    finally {
        const again = document.querySelector("[data-vendor-send]");
        if (again)
            again.disabled = false;
    }
}
function setInviteMode(mode) {
    document.querySelectorAll("[data-vendor-panel]").forEach((el) => {
        el.classList.toggle("hidden", el.dataset.vendorPanel !== mode);
    });
    document.querySelectorAll("[data-vendor-mode]").forEach((el) => {
        const on = el.dataset.vendorMode === mode;
        el.classList.toggle("bg-surface", on);
        el.classList.toggle("text-muted", !on);
    });
}
// My Reports reloads its rows, so a new vendor report shows there.
function refreshReportRows() {
    const htmx = window.htmx;
    if (htmx && typeof htmx.ajax === "function" && document.getElementById("report-rows")) {
        htmx.ajax("GET", API_BASE + "/reports/html", { target: "#report-rows", swap: "innerHTML" });
    }
}
// The "Vendor" / "Marketplace" / "Website" labels in My Reports, in the
// reader's language.
function translateReportKinds() {
    const names = {
        vendor: ["dash.reports.kind_vendor", "Vendor"],
        marketplace: ["dash.reports.kind_marketplace", "Marketplace"],
        website: ["dash.reports.kind_website", "Website"],
    };
    document.querySelectorAll("#report-rows [data-report-kind]").forEach((el) => {
        const name = names[el.dataset.reportKind || ""];
        if (name)
            el.textContent = t(name[0], name[1]);
    });
    // The reason ("Took payment and never shipped") and the "Website" link.
    document.querySelectorAll("#report-rows [data-report-reason]").forEach((el) => {
        const reason = el.dataset.reportReason || "";
        if (REPORT_REASON_TRANSLATIONS[reason])
            el.textContent = translateReportReason(reason);
    });
    document.querySelectorAll('#report-rows [data-report-link="website"]').forEach((el) => {
        el.textContent = t("dash.reports.kind_website", "Website") + " \u2197";
    });
}
// Downloads the vendor's PDF report (backend vendor_pdf.rs).
async function downloadVendorPdf(d, button) {
    const auth = window.safelyAuth;
    if (button)
        button.disabled = true;
    try {
        const res = await fetch(API_BASE + "/vendors/" + encodeURIComponent(d.id) + "/pdf", { headers: auth.authHeader() });
        if (!res.ok)
            throw new Error("failed");
        const url = URL.createObjectURL(await res.blob());
        const a = document.createElement("a");
        a.href = url;
        const safe = d.name.replace(/[^A-Za-z0-9]+/g, "-").replace(/^-|-$/g, "").toLowerCase() || "vendor";
        a.download = "safely-vendor-" + safe + ".pdf";
        document.body.appendChild(a);
        a.click();
        a.remove();
        setTimeout(() => URL.revokeObjectURL(url), 2000);
    }
    catch (e) {
        showToast(t("dash.vendors.pdf_failed", "Could not make the PDF right now. Please try again."));
    }
    finally {
        if (button)
            button.disabled = false;
    }
}
async function downloadVendorDocument(id, docId, name) {
    const auth = window.safelyAuth;
    try {
        const res = await fetch(API_BASE + "/vendors/" + encodeURIComponent(id) + "/documents/" + encodeURIComponent(docId), {
            headers: auth.authHeader(),
        });
        if (!res.ok)
            throw new Error("failed");
        const url = URL.createObjectURL(await res.blob());
        const a = document.createElement("a");
        a.href = url;
        a.download = name;
        document.body.appendChild(a);
        a.click();
        a.remove();
        setTimeout(() => URL.revokeObjectURL(url), 2000);
    }
    catch (e) {
        showToast(t("dash.watch.failed", "Could not change this right now. Please try again."));
    }
}
async function copyVendorLink(link) {
    try {
        await navigator.clipboard.writeText(link);
        showToast(t("dash.vendors.copied", "Link copied."));
    }
    catch (e) {
        window.prompt(t("dash.vendors.copy", "Copy link"), link);
    }
}
document.addEventListener("DOMContentLoaded", () => {
    const radio = document.getElementById("view-vendors");
    if (radio) {
        radio.addEventListener("change", () => {
            if (!radio.checked)
                return;
            closeDetailPanel();
            const mobileToggle = document.getElementById("mobile-nav-toggle");
            if (mobileToggle)
                mobileToggle.checked = false;
            vendorsAddressShown = "";
            routeVendorsFromAddress();
        });
        if (radio.checked)
            routeVendorsFromAddress();
        // Back / Forward between the list and a vendor (shared.ts).
        document.addEventListener("safely:address-changed", () => {
            if (radio.checked)
                routeVendorsFromAddress();
        });
        // "Vendors" in the menu while a vendor is open: back to the list.
        document.querySelector('label[for="view-vendors"]')?.addEventListener("click", () => {
            if (radio.checked && vendorOpenId)
                showVendorList();
        });
    }
    document.body.addEventListener("htmx:afterSwap", (event) => {
        if (event.target && event.target.id === "report-rows")
            translateReportKinds();
    });
    // A click outside the "whose vendors" menu closes it.
    document.addEventListener("click", (e) => {
        const box = document.getElementById("vendor-who");
        if (box && !box.contains(e.target))
            document.getElementById("vendor-who-menu")?.classList.add("hidden");
    });
    const body = document.getElementById("vendors-body");
    if (!body)
        return;
    body.addEventListener("change", async (e) => {
        const target = e.target;
        if (target.id === "vi-file" && target.files && target.files[0]) {
            const text = await target.files[0].text();
            const area = document.getElementById("vi-csv");
            if (area)
                area.value = text;
            target.dispatchEvent(new Event("input", { bubbles: true }));
        }
    });
    body.addEventListener("input", (e) => {
        if (e.target.id === "vendor-search") {
            vendorSearch = e.target.value;
            filterVendorRows();
            return;
        }
        const count = document.getElementById("vi-csv-count");
        const area = document.getElementById("vi-csv");
        if (count && area) {
            const n = parseVendorCsv(area.value).length;
            count.textContent = n ? t("dash.vendors.csv_count", "{n} vendors found").replace("{n}", String(n)) : "";
        }
    });
    body.addEventListener("click", async (e) => {
        const target = e.target;
        const on = (attr) => target.closest("[" + attr + "]");
        // Rows that open, and "Show more", as on a scan's result page.
        const opener = on("data-open");
        if (opener) {
            const row = opener.closest(".d-row");
            if (row)
                opener.setAttribute("aria-expanded", row.classList.toggle("is-open") ? "true" : "false");
            return;
        }
        const more = on("data-more");
        if (more) {
            const list = more.closest(".d-rows");
            if (!list)
                return;
            const all = list.classList.toggle("show-all");
            const label = more.querySelector("span");
            if (label)
                label.textContent = all ? t("dash.detail.show_less", "Show less") : more.dataset.more || "";
            return;
        }
        const whoPick = on("data-vendor-who");
        if (whoPick) {
            vendorWho = whoPick.dataset.vendorWho || "me";
            loadVendors();
            return;
        }
        if (on("data-vendor-who-btn")) {
            document.getElementById("vendor-who-menu")?.classList.toggle("hidden");
            return;
        }
        const riskPick = on("data-vendor-risk");
        if (riskPick && vendorList) {
            vendorRisk = riskPick.dataset.vendorRisk || "all";
            const keepInvite = document.getElementById("vendor-invite")?.outerHTML;
            body.innerHTML = buildVendorsPage(vendorList);
            const fresh = document.getElementById("vendor-invite");
            if (keepInvite && fresh)
                fresh.outerHTML = keepInvite;
            filterVendorRows();
            return;
        }
        if (on("data-vendor-report-open")) {
            openVendorBox("vendor-report");
            return;
        }
        if (on("data-vendor-report-close")) {
            document.getElementById("vendor-report")?.classList.add("hidden");
            return;
        }
        if (on("data-vendor-report-send")) {
            const picked = document.querySelector('input[name="vendor-report-reason"]:checked');
            const note = document.getElementById("vendor-report-note")?.value || "";
            const res = await vendorAction("/report", { report_type: picked ? picked.value : "scam", description: note }, t("dash.vendors.report_done", "Report sent. It's in My Reports."));
            if (res)
                refreshReportRows();
            return;
        }
        const pdf = on("data-vendor-pdf");
        if (pdf && vendorOpenId) {
            const name = document.querySelector("#vendors-body [data-vendor-title]")?.textContent || "vendor";
            downloadVendorPdf({ id: vendorOpenId, name }, pdf);
            return;
        }
        const doc = on("data-vendor-doc");
        if (doc && vendorOpenId) {
            downloadVendorDocument(vendorOpenId, doc.dataset.vendorDoc, doc.dataset.vendorDocName || "document");
            return;
        }
        if (on("data-vendor-plans") || on("data-detail-unlock")) {
            window.location.href = "/dashboard/?manage_billing=1";
        }
        else if (on("data-vendor-invite")) {
            document.getElementById("vendor-invite")?.classList.remove("hidden");
        }
        else if (on("data-vendor-close-invite")) {
            document.getElementById("vendor-invite")?.classList.add("hidden");
        }
        else if (on("data-vendor-mode")) {
            setInviteMode(on("data-vendor-mode").dataset.vendorMode || "one");
        }
        else if (on("data-vendor-send")) {
            sendInvites();
        }
        else if (on("data-vendor-copy")) {
            e.preventDefault();
            copyVendorLink(on("data-vendor-copy").dataset.vendorCopy || "");
        }
        else if (on("data-vendor-open")) {
            openVendor(on("data-vendor-open").dataset.vendorOpen);
        }
        else if (on("data-vendor-back")) {
            showVendorList();
        }
        else if (on("data-vendor-decide")) {
            const decision = on("data-vendor-decide").dataset.vendorDecide;
            const box = document.getElementById("vendor-decide");
            const send = box?.querySelector("[data-vendor-decide-send]");
            const title = document.getElementById("vendor-decide-title");
            if (!box || !send || !title)
                return;
            // The same button again closes the box.
            const open = !box.classList.contains("hidden") && box.dataset.decision === decision;
            openVendorBox(null);
            if (open)
                return;
            box.dataset.decision = decision;
            const approve = decision === "approve";
            title.textContent = approve
                ? t("dash.vendors.approve_title", "Approve this vendor")
                : t("dash.vendors.reject_title", "Reject this vendor");
            send.textContent = approve ? t("dash.vendors.approve", "Approve") : t("dash.vendors.reject", "Reject");
            send.className = approve
                ? V_BTN_DARK
                : "px-3.5 py-2 rounded-lg bg-coral text-white text-[12.5px] font-bold hover:opacity-90 transition";
            box.classList.remove("hidden");
            document.getElementById("vendor-decide-note")?.focus();
        }
        else if (on("data-vendor-decide-send")) {
            const box = document.getElementById("vendor-decide");
            const decision = box?.dataset.decision || "approve";
            const note = document.getElementById("vendor-decide-note")?.value || "";
            vendorAction("/decision", { decision, note }, decision === "approve" ? t("dash.vendors.approved", "Vendor approved.") : t("dash.vendors.rejected", "Vendor rejected."));
        }
        else if (on("data-vendor-decide-close")) {
            openVendorBox(null);
            const note = document.getElementById("vendor-decide-note");
            if (note)
                note.value = "";
        }
        else if (on("data-vendor-ask-open")) {
            openVendorBox("vendor-ask");
        }
        else if (on("data-vendor-ask-close")) {
            // Close the box and clear what was picked.
            const box = document.getElementById("vendor-ask");
            box?.classList.add("hidden");
            box?.querySelectorAll("[data-ask-field]").forEach((el) => (el.checked = false));
            const note = document.getElementById("vendor-ask-note");
            if (note)
                note.value = "";
        }
        else if (on("data-vendor-ask-send")) {
            const fields = Array.from(document.querySelectorAll("[data-ask-field]"))
                .filter((el) => el.checked)
                .map((el) => el.dataset.askField);
            const note = document.getElementById("vendor-ask-note")?.value || "";
            const res = await vendorAction("/ask", { fields, note, reason: "more_info" }, t("dash.vendors.asked", "Link sent to the vendor."));
            if (res && res.link)
                showVendorLink(res.link);
        }
        else if (on("data-vendor-bank")) {
            const res = await vendorAction("/ask", { reason: "bank_change" }, t("dash.vendors.asked", "Link sent to the vendor."));
            if (res && res.link)
                showVendorLink(res.link);
        }
        else if (on("data-vendor-watch")) {
            vendorAction("/watch", {}, t("dash.vendors.watching", "Safely now watches this supplier."));
        }
        else if (on("data-vendor-remind")) {
            vendorAction("/remind", {}, t("dash.vendors.reminded", "Reminder sent."));
        }
        else if (on("data-vendor-link")) {
            const res = await vendorAction("/link", {}, "");
            if (res && res.link)
                showVendorLink(res.link);
        }
        else if (on("data-vendor-cancel-send")) {
            vendorAction("/cancel", {}, t("dash.vendors.canceled", "Link canceled."));
        }
        else if (on("data-vendor-cancel-close")) {
            openVendorBox(null);
        }
        else if (on("data-vendor-cancel")) {
            openVendorBox("vendor-cancel-box");
        }
        else if (on("data-vendor-retry")) {
            vendorAction("/retry", {}, t("dash.vendors.retrying", "The check is running. Refresh in a minute."));
        }
    });
});
