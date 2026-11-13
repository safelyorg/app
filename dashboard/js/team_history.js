"use strict";
// Team scans in History: one dropdown next to "Analysis History".
//
// - Owner: "My scans", "All team scans", then each member.
// - Member: "My scans", then each teammate (the owner first), one at a
//   time. Members do not get "All team scans".
// - Not in a paid team: no dropdown, History stays as it is.
//
// Picking one loads that list from the server (GET /api/v1/history/html
// ?who=team or ?who=<user id>; backend services/features/team_history.rs)
// and changes the title text, the three numbers and the "Scanned by"
// column to match. The page always opens on "My scans".
// "me", "team" or a person's user ID (also on window.safelyHistoryWho).
let historyWho = "me";
let historyChoices = null;
// "ali" from "ali@company.com", for the short labels.
function shortName(email) {
    const at = email.indexOf("@");
    return at > 0 ? email.slice(0, at) : email;
}
function escapeText(text) {
    return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;");
}
function personFor(who) {
    return historyChoices ? historyChoices.people.find((p) => p.id === who) : undefined;
}
// The dropdown button's text for a choice.
function whoLabel(who) {
    if (who === "me")
        return t("dash.team_history.my_scans", "My scans");
    if (who === "team")
        return t("dash.team_history.all_team", "All team scans");
    const person = personFor(who);
    const name = person ? shortName(person.email) : "";
    return t("dash.team_history.person_scans", "{name}'s scans").replace("{name}", name);
}
function initialOf(email) {
    return email ? email.charAt(0).toUpperCase() : "?";
}
function menuRow(who, text, badge, dark) {
    const on = who === historyWho;
    return ('<button type="button" data-history-who="' +
        escapeText(who) +
        '" class="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-left text-[12.5px] hover:bg-surface2 transition' +
        (on ? " bg-surface2 font-semibold" : "") +
        '">' +
        (badge
            ? '<span class="w-5 h-5 rounded-full ' +
                (dark ? "bg-ink text-bg" : "bg-amber/20 text-ink") +
                ' text-[10px] font-bold flex items-center justify-center flex-shrink-0">' +
                escapeText(badge) +
                "</span>"
            : "") +
        '<span class="truncate">' +
        escapeText(text) +
        "</span>" +
        (on
            ? '<svg class="ml-auto flex-shrink-0 text-brand" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6L9 17l-5-5"/></svg>'
            : "") +
        "</button>");
}
// The dropdown: the button and its (closed) menu.
function buildHistoryDropdown(choices) {
    let menu = menuRow("me", t("dash.team_history.my_scans", "My scans"), "", false);
    if (choices.can_see_all) {
        menu += menuRow("team", t("dash.team_history.all_team", "All team scans"), "", false);
    }
    if (choices.people.length) {
        menu +=
            '<div class="px-3 pt-2.5 pb-1 mt-1 border-t border-line text-[10.5px] font-semibold uppercase tracking-wide text-muted">' +
                escapeText(choices.can_see_all
                    ? t("dash.team_history.members", "Team members")
                    : t("dash.team_history.team", "Team")) +
                "</div>";
        choices.people.forEach((p) => {
            const text = p.is_owner
                ? p.email + " " + t("dash.team_history.owner_tag", "(owner)")
                : p.email;
            menu += menuRow(p.id, text, initialOf(p.email), p.is_owner);
        });
    }
    return ('<button type="button" id="history-who-btn" aria-haspopup="true" aria-expanded="false" class="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-surface border border-line text-[12px] font-bold hover:border-brand/40 transition">' +
        '<span id="history-who-label">' +
        escapeText(whoLabel(historyWho)) +
        "</span>" +
        '<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" class="text-muted"><path d="M6 9l6 6 6-6"/></svg>' +
        "</button>" +
        '<div id="history-who-menu" class="hidden absolute left-0 top-full mt-1.5 w-64 max-w-[80vw] bg-surface border border-line rounded-xl p-1.5 shadow-xl z-30">' +
        menu +
        "</div>");
}
function setText(id, text) {
    const el = document.getElementById(id);
    if (!el)
        return;
    // This text now follows the dropdown, not the page-load translation.
    el.removeAttribute("data-i18n");
    el.textContent = text;
}
// Page title text and the labels under the three numbers.
function renderWhoTexts() {
    if (historyWho === "me") {
        setText("history-desc", t("dash.history.page_desc", "Every listing you checked with the Safely extension."));
        setText("stat-checked-label", t("dash.stat.checked", "listings checked"));
        setText("stat-reported-label", t("dash.stat.reported", "reported by you"));
        return;
    }
    if (historyWho === "team") {
        setText("history-desc", t("dash.team_history.desc_team", "Every listing your team checked with the Safely extension."));
        setText("stat-checked-label", t("dash.team_history.checked_team", "listings checked by the team"));
        setText("stat-reported-label", t("dash.team_history.reported_team", "reported by the team"));
        return;
    }
    const person = personFor(historyWho);
    const email = person ? person.email : "";
    const name = shortName(email);
    setText("history-desc", t("dash.team_history.desc_person", "Every listing {email} checked with the Safely extension.").replace("{email}", email));
    setText("stat-checked-label", t("dash.team_history.checked_person", "listings checked by {name}").replace("{name}", name));
    setText("stat-reported-label", t("dash.team_history.reported_person", "reported by {name}").replace("{name}", name));
}
// The "Scanned by" column: only on "All team scans".
function setScannedByColumn(on) {
    const th = document.getElementById("th-who");
    if (th)
        th.className = on ? "hidden sm:table-cell px-4 py-3" : "hidden";
    const cols = document.getElementById("history-cols");
    const existing = document.getElementById("col-who");
    if (on && cols && !existing) {
        const col = document.createElement("col");
        col.id = "col-who";
        col.className = "hidden sm:table-column";
        col.style.width = "190px";
        // After Date and Listing, before Platform.
        cols.insertBefore(col, cols.children[2] || null);
    }
    else if (!on && existing) {
        existing.remove();
    }
}
function closeHistoryMenu() {
    const menu = document.getElementById("history-who-menu");
    const btn = document.getElementById("history-who-btn");
    if (menu)
        menu.classList.add("hidden");
    if (btn)
        btn.setAttribute("aria-expanded", "false");
}
function loadHistoryRows() {
    const url = API_BASE + "/history/html" + (historyWho === "me" ? "" : "?who=" + encodeURIComponent(historyWho));
    const htmx = window.htmx;
    if (htmx && typeof htmx.ajax === "function") {
        htmx.ajax("GET", url, { target: "#history-rows", swap: "innerHTML" });
    }
}
function pickHistoryWho(who) {
    closeHistoryMenu();
    if (who === historyWho)
        return;
    historyWho = who;
    // renderStats (history.ts) reads this to count the right "reported".
    window.safelyHistoryWho = who;
    const box = document.getElementById("history-who");
    if (box && historyChoices)
        box.innerHTML = buildHistoryDropdown(historyChoices);
    renderWhoTexts();
    setScannedByColumn(who === "team");
    loadHistoryRows();
}
async function loadHistoryChoices() {
    const auth = window.safelyAuth;
    if (!auth || !auth.getToken())
        return;
    try {
        const res = await fetch(API_BASE + "/history/people", { headers: auth.authHeader() });
        if (!res.ok)
            return;
        const data = (await res.json());
        const box = document.getElementById("history-who");
        if (!box)
            return;
        if (!data.team) {
            historyChoices = null;
            box.classList.add("hidden");
            box.innerHTML = "";
            if (historyWho !== "me")
                pickHistoryWho("me");
            return;
        }
        historyChoices = data;
        // The list picked before is gone (left the team, person removed).
        const still = historyWho === "me" || (historyWho === "team" ? data.can_see_all : !!personFor(historyWho));
        if (!still)
            pickHistoryWho("me");
        box.innerHTML = buildHistoryDropdown(data);
        box.classList.remove("hidden");
    }
    catch (e) {
        console.error("Safely: failed to load the team list for History", e);
    }
}
document.addEventListener("DOMContentLoaded", () => {
    const box = document.getElementById("history-who");
    if (box) {
        box.addEventListener("click", (e) => {
            const target = e.target;
            const pick = target.closest("[data-history-who]");
            if (pick) {
                pickHistoryWho(pick.dataset.historyWho);
                return;
            }
            if (target.closest("#history-who-btn")) {
                const menu = document.getElementById("history-who-menu");
                const btn = document.getElementById("history-who-btn");
                if (!menu)
                    return;
                const open = menu.classList.contains("hidden");
                menu.classList.toggle("hidden", !open);
                if (btn)
                    btn.setAttribute("aria-expanded", open ? "true" : "false");
            }
        });
    }
    document.addEventListener("click", (e) => {
        const box2 = document.getElementById("history-who");
        if (box2 && !box2.contains(e.target))
            closeHistoryMenu();
    });
    // New rows: "You" in the reader's language, and the search box
    // filter applied again.
    document.body.addEventListener("htmx:afterSwap", (event) => {
        if (!event.target || event.target.id !== "history-rows")
            return;
        document.querySelectorAll("#history-rows [data-scanned-you]").forEach((el) => {
            el.textContent = t("dash.team_history.you", "You");
        });
        const search = document.getElementById("search-box");
        if (search && search.value)
            search.dispatchEvent(new Event("input"));
    });
    loadHistoryChoices();
});
// After a change in Settings > Team (team.ts), the list may change.
window.safelyReloadHistoryChoices = loadHistoryChoices;
