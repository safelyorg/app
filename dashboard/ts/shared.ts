// Shared constants and helpers used across the dashboard's other
// script files.

const API_BASE = "/api/v1";

function showToast(message: string, durationMs: number = 3000): void {
  const toast = document.createElement("div");
  toast.textContent = message;
  toast.className = "safely-toast";
  document.body.appendChild(toast);

  setTimeout(() => {
    toast.classList.add("fade-out");
    setTimeout(() => toast.remove(), 300);
  }, durationMs);
}

function formatDate(isoString: string | null | undefined): string {
  return isoString ? isoString.slice(0, 10) : "";
}

function escapeAttr(str: unknown): string {
  return String(str).replace(/"/g, "&quot;");
}

function t(key: string, fallback: string): string {
  const fn = (window as any).safelyT;
  return typeof fn === "function" ? fn(key, fallback) : fallback;
}

// ── Each dashboard page has its own address ──
// /dashboard is History, /reports is My Reports and /settings is
// Settings; one scan's result is /dashboard/<platform>/<scan id>
// (/dashboard/alibaba/0193..., /dashboard/website/0193...). A refresh
// or a shared link opens the same page, and the browser's Back button
// goes back to the page before. The pages are switched by the hidden
// "view" radio buttons in index.html; the server sends index.html for
// all these addresses (backend/src/main.rs). Runs here, before any
// page's own start-up code, because those check which radio is on.
const DASHBOARD_PAGES: [string, string][] = [
  ["view-history", "/dashboard/"],
  ["view-reports", "/reports"],
  ["view-settings", "/settings"],
];

// True while the dashboard follows Back / Forward: the address is
// already right, so nothing may add a new one (detail_view.ts
// closeDetailPanel reads it).
let dashboardFollowingAddress = false;

// "/settings" for "view-settings".
function dashboardAddressFor(viewId: string): string {
  const page = DASHBOARD_PAGES.find(([id]) => id === viewId);
  return page ? page[1] : "/dashboard/";
}

// The address of one scan's result.
function resultAddressFor(platform: string, analysisId: string): string {
  return (
    "/dashboard/" +
    encodeURIComponent((platform || "scan").toLowerCase()) +
    "/" +
    encodeURIComponent(analysisId)
  );
}

// The scan id in a result address, or null.
function resultIdFromAddress(path: string): string | null {
  const match = path.match(/^\/dashboard\/[^/]+\/([0-9a-fA-F-]{36})\/?$/);
  return match ? match[1] : null;
}

// The radio button of the page in the address ("view-settings"). A
// result belongs to History, and so does any other address.
function dashboardViewFromAddress(path: string): string {
  const clean = path.replace(/\/+$/, "").toLowerCase();
  if (clean === "/reports") return "view-reports";
  if (clean === "/settings") return "view-settings";
  return "view-history";
}

(function () {
  // Turns on the page in the address. `announce`: also tell the pages'
  // own code (loads Settings, closes an open result), as a click would.
  function showPageFromAddress(announce: boolean): void {
    const radio = document.getElementById(
      dashboardViewFromAddress(window.location.pathname),
    ) as HTMLInputElement | null;
    if (!radio || radio.checked) return;
    radio.checked = true;
    if (announce) radio.dispatchEvent(new Event("change", { bubbles: true }));
  }

  showPageFromAddress(false);

  DASHBOARD_PAGES.forEach(([id]) => {
    const radio = document.getElementById(id) as HTMLInputElement | null;
    if (!radio) return;
    radio.addEventListener("change", () => {
      if (!radio.checked) return;
      const address = dashboardAddressFor(id);
      // Coming back here with Back already put the right address in.
      if (dashboardViewFromAddress(window.location.pathname) === id) return;
      history.pushState(null, "", address);
    });
  });

  // Back / Forward: show the page and the result of the new address.
  window.addEventListener("popstate", () => {
    dashboardFollowingAddress = true;
    try {
      showPageFromAddress(true);
      const resultId = resultIdFromAddress(window.location.pathname);
      if (resultId) {
        if (resultId !== currentAnalysisId) openDetail(resultId);
      } else {
        closeDetailPanel();
      }
    } finally {
      dashboardFollowingAddress = false;
    }
  });
})();
