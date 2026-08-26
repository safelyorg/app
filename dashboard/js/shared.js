"use strict";
// Shared constants and helpers used across the dashboard's other
// script files.
const API_BASE = "/api/v1";
const RISK_HEX = {
    low: "#35d0a6",
    caution: "#f2b84c",
    high: "#ff5d5d",
};
function riskHex(level) {
    return RISK_HEX[level] || RISK_HEX.high;
}
function showToast(message, durationMs = 3000) {
    const toast = document.createElement("div");
    toast.textContent = message;
    toast.className = "safely-toast";
    document.body.appendChild(toast);
    setTimeout(() => {
        toast.classList.add("fade-out");
        setTimeout(() => toast.remove(), 300);
    }, durationMs);
}
function verdictTextClass(level) {
    return level === "low"
        ? "text-mint"
        : level === "caution"
            ? "text-amber"
            : "text-coral";
}
function signalTextClass(type) {
    return type === "good"
        ? "text-mint"
        : type === "info"
            ? "text-muted"
            : type === "caution"
                ? "text-amber"
                : "text-coral";
}
function formatDate(isoString) {
    return isoString ? isoString.slice(0, 10) : "";
}
function escapeAttr(str) {
    return String(str).replace(/"/g, "&quot;");
}
