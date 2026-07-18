chrome.runtime.onInstalled.addListener((details) => {
  if (details.reason === "install") {
    console.log("[Safely] Installed — protecting payments everywhere.");
  }
});

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === "PING") {
    sendResponse({ status: "ok" });
  }

  // Sent by auth-bridge.js, which runs only on Safely's own site. This is
  // the one place chrome.storage.local gets written for the session token -
  // every other content script (on OLX, Facebook, etc.) only ever READS
  // this value, never writes it, since they have no access to the website's
  // localStorage where the real login state lives.
  if (message.type === "SAFELY_SESSION_UPDATE") {
    if (message.token) {
      chrome.storage.local.set({ safely_session_token: message.token });
    } else {
      // token is null - the person logged out on the website.
      chrome.storage.local.remove("safely_session_token");
    }
    sendResponse({ status: "ok" });
  }

  return true;
});
