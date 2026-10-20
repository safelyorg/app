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
