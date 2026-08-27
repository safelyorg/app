interface SessionUpdateMessage {
  type: "SAFELY_SESSION_UPDATE";
  token: string | null;
}

chrome.runtime.onInstalled.addListener((details) => {
  if (details.reason === "install") {
    console.log("[Safely] Installed — protecting payments everywhere.");
  }
});

// Sent by auth-bridge.js, which runs only on Safely's own site.
// This is the one place chrome.storage.local gets written for the
// session token - every other content script (on OLX, Facebook,
// etc.) only ever READS this value, never writes it, since they
// have no access to the website's localStorage where the real
// login state lives.
function handleSessionUpdateMessage(
  message: SessionUpdateMessage,
  _sender: chrome.runtime.MessageSender,
  sendResponse: (response: { status: string }) => void,
): boolean {
  if (message.type === "SAFELY_SESSION_UPDATE") {
    if (message.token) {
      chrome.storage.local.set({ safely_session_token: message.token });
    } else {
      chrome.storage.local.remove("safely_session_token");
    }
    sendResponse({ status: "ok" });
  }
  return true;
}

chrome.runtime.onMessage.addListener(handleSessionUpdateMessage);

// Exposed the same, safe way as every other testable function in this
// codebase - never via `export`, since this file loads as a plain
// script, not a module.
(globalThis as any).__safelyHandleSessionUpdateMessage = handleSessionUpdateMessage;
