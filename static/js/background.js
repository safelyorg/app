chrome.runtime.onInstalled.addListener((details) => {
  if (details.reason === "install") {
    console.log("[Safely] Installed — protecting payments everywhere.");
  }
});

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message.type === "PING") {
    sendResponse({ status: "ok" });
  }
  return true;
});
