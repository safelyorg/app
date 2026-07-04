// auth-bridge.js
//
// This content script is special: it does NOT run on OLX or Facebook. It
// only runs on Safely's own website (the dashboard/landing page). Its one
// job is to peek at this page's localStorage, grab the login token if one
// exists, and hand it to the extension's background script.
//
// Why this is needed: content scripts on OLX cannot see localStorage that
// belongs to a completely different website (localhost:3000). Storage is
// locked to whichever origin created it. This script is the only piece of
// the extension that is ever "inside" the website's own origin, so it is
// the only piece that can legally read this value and pass it along.
(function () {
  "use strict";

  var STORAGE_KEY = "safely_session_token";
  var lastSent = undefined; // undefined so the very first check always sends, even if the token is null

  function relayToken() {
    var token = null;
    try {
      token = window.localStorage.getItem(STORAGE_KEY);
    } catch (e) {
      // localStorage can throw in rare restricted contexts (e.g. some
      // privacy modes) - fail quietly rather than break the page.
    }

    if (token === lastSent) return; // nothing changed since last check
    lastSent = token;

    chrome.runtime.sendMessage({
      type: "SAFELY_SESSION_UPDATE",
      token: token, // null here means "logged out" - the background script clears its copy too
    });
  }

  relayToken();

  // A page writing to its own localStorage does NOT fire a 'storage' event
  // in that same tab (only other open tabs get notified of the change) -
  // so there is no free "tell me when this changes" signal to listen for
  // here. Polling every couple seconds is simple, cheap, and reliable
  // enough for something that only changes on login/logout.
  setInterval(relayToken, 2000);
})();
