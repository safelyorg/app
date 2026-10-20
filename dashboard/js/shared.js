"use strict";
// Shared constants and helpers used across the dashboard's other
// script files.
const API_BASE = "/api/v1";
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
function formatDate(isoString) {
    return isoString ? isoString.slice(0, 10) : "";
}
function escapeAttr(str) {
    return String(str).replace(/"/g, "&quot;");
}
function t(key, fallback) {
    const fn = window.safelyT;
    return typeof fn === "function" ? fn(key, fallback) : fallback;
}
