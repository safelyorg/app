var API_BASE = "/api/v1";
var currentHistory = [];
var currentReports = [];

var RISK_HEX = { low: "#35d0a6", caution: "#f2b84c", high: "#ff5d5d" };

function riskHex(level) {
  return RISK_HEX[level] || RISK_HEX.high;
}

function verdictTextClass(level) {
  return level === "low"
    ? "text-mint"
    : level === "caution"
      ? "text-amber"
      : "text-coral";
}

function verdictBorderClass(level) {
  return level === "low"
    ? "border-mint"
    : level === "caution"
      ? "border-amber"
      : "border-coral";
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

function detectCardBrand(digits) {
  if (/^4/.test(digits)) return "visa";
  if (/^5[1-5]/.test(digits)) return "mastercard";
  if (/^2(22[1-9]|2[3-9]\d|[3-6]\d{2}|7[01]\d|720)/.test(digits)) {
    return "mastercard";
  }
  return null;
}

function updateCardBrandIcon(digits) {
  var brand = detectCardBrand(digits);
  var visaIcon = document.getElementById("card-brand-visa");
  var mcIcon = document.getElementById("card-brand-mastercard");
  if (visaIcon) visaIcon.classList.toggle("hidden", brand !== "visa");
  if (mcIcon) mcIcon.classList.toggle("hidden", brand !== "mastercard");
}
