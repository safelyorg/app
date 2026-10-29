"use strict";
chrome.runtime.onInstalled.addListener((details) => {
    if (details.reason === "install") {
        console.log("[Safely] Installed — protecting payments everywhere.");
    }
});
// Sent by auth-bridge.js, which runs only on Safely's own site.
// This is the one place chrome.storage.local gets written for the
// session token - every other content script (on Alibaba, Global Sources,
// etc.) only ever READS this value, never writes it, since they
// have no access to the website's localStorage where the real
// login state lives.
function handleSessionUpdateMessage(message, _sender, sendResponse) {
    if (message.type === "SAFELY_SESSION_UPDATE") {
        if (message.token) {
            chrome.storage.local.set({ safely_session_token: message.token });
        }
        else {
            chrome.storage.local.remove("safely_session_token");
        }
        sendResponse({ status: "ok" });
        return true;
    }
    // Other messages are answered by the other listener below.
    return false;
}
chrome.runtime.onMessage.addListener(handleSessionUpdateMessage);
// The only pages the background script loads for a scan:
// - a Global Sources supplier's page, e.g.
//   https://boxinltd.manufacturer.globalsources.com/homepage_6008846289828.htm
//   (must match GLOBALSOURCES_SUPPLIER_PAGE in scrapers/index.ts);
// - an Alibaba company's own page, e.g.
//   https://zl9809.en.alibaba.com/company_profile.html
//   (must match ALIBABA_COMPANY_PAGE in scrapers/index.ts).
// A product page cannot load them itself (they are on another address).
// Any other address is refused, so a web page cannot use this to load
// other sites.
const SUPPLIER_PAGES = [
    /^https:\/\/[a-z0-9-]+\.[a-z]+\.globalsources\.com\/homepage_\d+\.htm$/,
    /^https:\/\/[a-z0-9-]+\.en\.alibaba\.com\/company_profile\.html$/,
];
// The largest page sent back (Global Sources supplier pages are about
// 0.1 MB, Alibaba company pages about 0.3 MB).
const MAX_SUPPLIER_PAGE_CHARS = 3000000;
const SUPPLIER_PAGE_TIMEOUT_MS = 15000;
function isAllowedSupplierPage(url) {
    return typeof url === "string" && SUPPLIER_PAGES.some((pattern) => pattern.test(url));
}
async function loadSupplierPage(url) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), SUPPLIER_PAGE_TIMEOUT_MS);
    try {
        // No cookies: the supplier page is public.
        const response = await fetch(url, { credentials: "omit", signal: controller.signal });
        const type = response.headers.get("content-type") || "";
        if (!response.ok || !type.includes("text/html"))
            return null;
        const html = await response.text();
        return html.length > 0 && html.length <= MAX_SUPPLIER_PAGE_CHARS ? html : null;
    }
    catch (e) {
        console.error("[Safely] could not load the supplier page", e);
        return null;
    }
    finally {
        clearTimeout(timer);
    }
}
// Sent by scrapers/index.ts (fetchGlobalsourcesSupplierPage) during a
// scan. Only messages from this extension's own scripts are answered.
function handleFetchSupplierPageMessage(message, sender, sendResponse) {
    if (!message || message.type !== "SAFELY_FETCH_SUPPLIER_PAGE")
        return false;
    if (sender.id !== chrome.runtime.id || !isAllowedSupplierPage(message.url)) {
        sendResponse({ html: null });
        return false;
    }
    loadSupplierPage(message.url).then((html) => sendResponse({ html }));
    return true;
}
chrome.runtime.onMessage.addListener(handleFetchSupplierPageMessage);
// Calls to Safely's own server, made here for the content scripts
// (see safelyFetch in scrapers/index.ts): a website can block its own
// page's requests to other servers, but not the background script's.
// Only Safely's API is allowed, so this cannot be used to reach other
// sites.
const SAFELY_API_BASES = ["https://safely.sh/api/v1/", "http://localhost:3000/api/v1/"];
function isSafelyApiUrl(url) {
    return typeof url === "string" && SAFELY_API_BASES.some((base) => url.startsWith(base));
}
// A scan can take more than a minute. Chrome stops an idle background
// script after 30 seconds, so a small Chrome call every 20 seconds keeps
// it running while a call is waiting for the server.
let callsWaiting = 0;
let keepAlive;
function startKeepAlive() {
    callsWaiting += 1;
    if (!keepAlive) {
        keepAlive = setInterval(() => {
            chrome.runtime.getPlatformInfo(() => void chrome.runtime.lastError);
        }, 20000);
    }
}
function stopKeepAlive() {
    callsWaiting = Math.max(0, callsWaiting - 1);
    if (callsWaiting === 0 && keepAlive) {
        clearInterval(keepAlive);
        keepAlive = undefined;
    }
}
async function callSafelyApi(message) {
    startKeepAlive();
    try {
        const response = await fetch(message.url, {
            method: message.method,
            headers: message.headers,
            body: message.body,
        });
        return { status: response.status, text: await response.text() };
    }
    catch (e) {
        return { networkError: String((e && e.message) || e) };
    }
    finally {
        stopKeepAlive();
    }
}
function handleApiRequestMessage(message, sender, sendResponse) {
    if (!message || message.type !== "SAFELY_API_REQUEST")
        return false;
    if (sender.id !== chrome.runtime.id || !isSafelyApiUrl(message.url)) {
        sendResponse({ networkError: "not allowed" });
        return false;
    }
    callSafelyApi(message).then(sendResponse);
    return true;
}
chrome.runtime.onMessage.addListener(handleApiRequestMessage);
// Exposed the same, safe way as every other testable function in this
// codebase - never via `export`, since this file loads as a plain
// script, not a module.
globalThis.__safelyHandleSessionUpdateMessage = handleSessionUpdateMessage;
globalThis.__safelyHandleApiRequestMessage = handleApiRequestMessage;
globalThis.__safelyIsSafelyApiUrl = isSafelyApiUrl;
globalThis.__safelyHandleFetchSupplierPageMessage = handleFetchSupplierPageMessage;
globalThis.__safelyIsAllowedSupplierPage = isAllowedSupplierPage;
