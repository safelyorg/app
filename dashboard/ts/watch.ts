// The "Watched suppliers" page (/watch). Safely checks these suppliers
// again by itself and emails when something changes (backend
// services/features/watch.rs). A team shares one list.
//
// GET /api/v1/watch gives: paid, limit (null = no limit), used,
// every_days, next_check and the items. Free: a locked card.

interface WatchChange {
  kind: string;
  title: string;
  detail: string;
}

interface WatchItem {
  seller_id: string;
  analysis_id: string | null;
  name: string | null;
  platform: string;
  platform_name?: string;
  status: string;
  changes: WatchChange[];
  listing_checked: boolean;
  website: string | null;
  watched_since: string;
  added_by?: string | null;
  added_by_name?: string | null;
  added_by_you?: boolean;
  last_checked: string | null;
}

interface WatchList {
  paid: boolean;
  limit: number | null;
  used: number;
  every_days: number | null;
  next_check: string | null;
  items: WatchItem[];
}

function watchEscape(text: unknown): string {
  return String(text ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// The status pill and the line under it, for one supplier.
function watchStatus(item: WatchItem): { pill: string; note: string } {
  const pill = (cls: string, text: string) =>
    '<span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11.5px] font-bold ' +
    cls +
    '">' +
    watchEscape(text) +
    "</span>";
  if (item.status === "changed" && item.changes && item.changes.length) {
    const first = item.changes[0];
    const more = item.changes.length > 1 ? " +" + (item.changes.length - 1) : "";
    return {
      pill: pill("bg-coral/10 text-coral", "● " + first.title + more),
      note: first.detail,
    };
  }
  if (item.status === "pending") {
    return {
      pill: pill("bg-surface2 text-muted", t("dash.watch.status_pending", "First check soon")),
      note: "",
    };
  }
  if (item.status === "paused") {
    return {
      pill: pill("bg-surface2 text-muted", t("dash.watch.status_paused", "Paused")),
      note: t("dash.watch.paused_note", "Checks start again when the plan is paid."),
    };
  }
  if (item.status === "listing_not_checked") {
    return {
      pill: pill("bg-surface2 text-muted", t("dash.watch.status_listing_not_checked", "Listing not checked")),
      note: t("dash.watch.listing_note", "The marketplace blocked the check. Other checks: no change."),
    };
  }
  return {
    pill: pill("bg-mint/10 text-mint", "✓ " + t("dash.watch.status_no_change", "No change")),
    note: "",
  };
}

// The "Added by" cell: "You", else the person's name, else their email.
function watchAddedBy(item: WatchItem): string {
  if (item.added_by_you) {
    return '<span class="font-semibold">' + watchEscape(t("dash.watch.added_by_you", "You")) + "</span>";
  }
  const who = item.added_by_name || item.added_by;
  if (!who) return '<span class="text-muted">—</span>';
  return '<span title="' + watchEscape(item.added_by || "") + '">' + watchEscape(who) + "</span>";
}

function watchPlatformName(item: WatchItem): string {
  // Added from a vendor's form (Vendors page), not from a marketplace.
  if (item.platform === "vendor") return t("dash.watch.platform_vendor", "Vendor form");
  return item.platform_name || item.platform;
}

function watchCard(big: string, label: string, hot = false): string {
  return (
    '<div class="' +
    (hot ? "stat-hot " : "") +
    'bg-surface border border-line rounded-2xl px-4 py-3.5">' +
    '<span class="block font-display text-[24px] leading-tight">' +
    watchEscape(big) +
    "</span>" +
    '<span class="text-[12px] text-muted">' +
    watchEscape(label) +
    "</span></div>"
  );
}

// The whole page body for a list.
function buildWatchPage(list: WatchList): string {
  if (!list.paid) {
    return (
      '<div class="rounded-2xl border border-dashed border-line p-8 text-center bg-surface">' +
      '<div class="text-[15px] font-bold">' +
      watchEscape(t("dash.watch.locked_title", "Watching suppliers is on the Team and Enterprise plans")) +
      "</div>" +
      '<div class="text-[13px] text-muted mt-1.5 max-w-md mx-auto leading-relaxed">' +
      watchEscape(
        t(
          "dash.watch.locked_text",
          "Safely checks your suppliers again by itself (scam reports, sanctions lists, company registers, website) and emails you when something changes.",
        ),
      ) +
      "</div>" +
      '<button type="button" data-watch-plans class="mt-4 px-4 py-2 rounded-lg bg-ink text-bg text-[12.5px] font-bold hover:opacity-90 transition">' +
      watchEscape(t("dash.detail.see_plans", "See plans")) +
      "</button></div>"
    );
  }
  const changed = list.items.filter((i) => i.status === "changed").length;
  const used = list.limit === null ? String(list.used) : list.used + " / " + list.limit;
  let html =
    '<div class="grid grid-cols-3 gap-3 mb-6">' +
    watchCard(used, t("dash.watch.stat_watched", "suppliers watched")) +
    watchCard(String(changed), t("dash.watch.stat_changed", "changed at the last check"), true) +
    watchCard(list.next_check || "—", t("dash.watch.stat_next", "next check")) +
    "</div>";

  if (!list.items.length) {
    return (
      html +
      '<div class="rounded-2xl border border-line bg-surface p-8 text-center">' +
      '<div class="text-[14px] font-bold">' +
      watchEscape(t("dash.watch.empty_title", "No suppliers watched yet")) +
      "</div>" +
      '<div class="text-[13px] text-muted mt-1.5">' +
      watchEscape(
        t("dash.watch.empty_text", 'Open a scan in History and click "Watch this supplier".'),
      ) +
      "</div></div>"
    );
  }

  html +=
    '<div class="bg-surface border border-line rounded-2xl overflow-hidden"><table class="w-full table-fixed">' +
    '<colgroup><col /><col class="hidden sm:table-column" style="width:150px" /><col class="hidden sm:table-column" style="width:130px" /><col class="hidden sm:table-column" style="width:120px" /><col style="width:32%" /><col style="width:150px" /></colgroup>' +
    '<thead><tr class="text-left text-[12px] font-semibold text-muted bg-surface2 border-b border-line">' +
    '<th class="px-4 py-3">' +
    watchEscape(t("dash.watch.col_supplier", "Supplier")) +
    "</th>" +
    '<th class="hidden sm:table-cell px-4 py-3">' +
    watchEscape(t("dash.watch.col_added_by", "Added by")) +
    "</th>" +
    '<th class="hidden sm:table-cell px-4 py-3">' +
    watchEscape(t("dash.table.platform", "Platform")) +
    "</th>" +
    '<th class="hidden sm:table-cell px-4 py-3">' +
    watchEscape(t("dash.watch.col_checked", "Last checked")) +
    "</th>" +
    '<th class="px-4 py-3">' +
    watchEscape(t("dash.watch.col_status", "Status")) +
    "</th><th></th></tr></thead><tbody>";

  list.items.forEach((item) => {
    const status = watchStatus(item);
    const since = t("dash.watch.since", "Watched since {date}").replace("{date}", item.watched_since);
    html +=
      '<tr class="border-b border-line last:border-0">' +
      '<td class="px-4 py-3.5"><div class="text-[13.5px] font-semibold truncate">' +
      watchEscape(item.name || t("dash.detail.untitled_listing", "Untitled listing")) +
      '</div><div class="text-[11.5px] text-muted mt-0.5 truncate">' +
      watchEscape(item.website ? since + " · " + item.website : since) +

      "</div></td>" +
      '<td class="hidden sm:table-cell px-4 py-3.5 text-[12.5px] truncate">' +
      watchAddedBy(item) +
      "</td>" +
      '<td class="hidden sm:table-cell px-4 py-3.5 text-[12.5px] text-muted truncate">' +
      watchEscape(watchPlatformName(item)) +
      "</td>" +
      '<td class="hidden sm:table-cell px-4 py-3.5 text-[12.5px] text-muted num">' +
      watchEscape(item.last_checked || "—") +
      "</td>" +
      '<td class="px-4 py-3.5">' +
      status.pill +
      (status.note
        ? '<div class="text-[11.5px] text-muted mt-1 leading-snug">' + watchEscape(status.note) + "</div>"
        : "") +
      "</td>" +
      '<td class="px-4 py-3.5 text-right whitespace-nowrap">' +
      (item.analysis_id
        ? '<button type="button" data-watch-open="' +
          watchEscape(item.analysis_id) +
          '" class="px-2.5 py-1 rounded-lg bg-surface2 border border-line text-[11.5px] font-bold hover:bg-surface3 transition">' +
          watchEscape(t("dash.watch.open", "Open")) +
          "</button> "
        : "") +
      '<button type="button" data-watch-stop="' +
      watchEscape(item.seller_id) +
      '" class="px-2.5 py-1 rounded-lg text-coral text-[11.5px] font-bold hover:bg-coral/10 transition">' +
      watchEscape(t("dash.watch.stop", "Stop")) +
      "</button></td></tr>";
  });
  return html + "</tbody></table></div>";
}

function watchDescription(list: WatchList | null): string {
  if (list && list.paid && list.every_days === 1) {
    return t("dash.watch.desc_daily", "Safely checks these suppliers every day and emails you when something changes.");
  }
  return t("dash.watch.desc_weekly", "Safely checks these suppliers every week and emails you when something changes.");
}

async function loadWatchList(): Promise<void> {
  const body = document.getElementById("watch-body");
  const desc = document.getElementById("watch-desc");
  if (!body) return;
  const auth = (window as any).safelyAuth;
  if (!auth || !auth.getToken()) return;
  try {
    const res = await fetch(API_BASE + "/watch", { headers: auth.authHeader() });
    if (!res.ok) throw new Error("failed");
    const list = (await res.json()) as WatchList;
    body.innerHTML = buildWatchPage(list);
    if (desc) desc.textContent = watchDescription(list);
  } catch (e) {
    console.error("Safely: failed to load the watched suppliers", e);
    body.innerHTML =
      '<div class="text-center text-muted text-sm py-16">' +
      watchEscape(t("dash.watch.load_failed", "Could not load the list. Please refresh the page.")) +
      "</div>";
  }
}

async function stopWatching(sellerId: string): Promise<void> {
  const auth = (window as any).safelyAuth;
  try {
    const res = await fetch(API_BASE + "/watch/stop", {
      method: "POST",
      headers: { ...auth.authHeader(), "Content-Type": "application/json" },
      body: JSON.stringify({ seller_id: sellerId }),
    });
    if (!res.ok && res.status !== 404) throw new Error("failed");
    showToast(t("dash.watch.stopped", "Stopped watching this supplier."));
  } catch (e) {
    console.error("Safely: could not stop watching", e);
    showToast(t("dash.watch.failed", "Could not change this right now. Please try again."));
  }
  loadWatchList();
}

document.addEventListener("DOMContentLoaded", () => {
  const radio = document.getElementById("view-watch") as HTMLInputElement | null;
  if (radio) {
    radio.addEventListener("change", () => {
      if (!radio.checked) return;
      closeDetailPanel();
      const mobileToggle = document.getElementById("mobile-nav-toggle") as HTMLInputElement | null;
      if (mobileToggle) mobileToggle.checked = false;
      loadWatchList();
    });
    if (radio.checked) loadWatchList();
  }
  const desc = document.getElementById("watch-desc");
  if (desc) desc.textContent = watchDescription(null);

  const body = document.getElementById("watch-body");
  if (body) {
    body.addEventListener("click", (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      const open = target.closest("[data-watch-open]") as HTMLElement | null;
      if (open) {
        openDetail(open.dataset.watchOpen as string);
        return;
      }
      const stop = target.closest("[data-watch-stop]") as HTMLElement | null;
      if (stop) {
        stopWatching(stop.dataset.watchStop as string);
        return;
      }
      if (target.closest("[data-watch-plans]")) {
        window.location.href = "/dashboard/?manage_billing=1";
      }
    });
  }
});
