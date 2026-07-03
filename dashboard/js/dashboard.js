// ============================================================
// Session handling. Runs first, before anything else.
// ============================================================

(function () {
  "use strict";

  var STORAGE_KEY = "safely_session_token";

  // The backend redirects here as /dashboard#session=<token> right after
  // a successful magic-link or Google sign-in. Capture it once, then
  // scrub it from the visible URL so it doesn't linger in browser
  // history or get reprocessed on refresh.
  function captureSessionFromUrl() {
    var hash = window.location.hash;
    if (hash.indexOf("#session=") === 0) {
      var token = hash.slice("#session=".length);
      if (token) {
        localStorage.setItem(STORAGE_KEY, token);
      }
      history.replaceState(
        null,
        "",
        window.location.pathname + window.location.search,
      );
    }
  }

  function getToken() {
    return localStorage.getItem(STORAGE_KEY);
  }

  function clearToken() {
    localStorage.removeItem(STORAGE_KEY);
  }

  // Gate the whole page on whether a token exists. If either #app or
  // #login-gate isn't found (i.e. this HTML hasn't been updated to
  // include them yet), this silently does nothing rather than erroring -
  // that's the current situation, and it's exactly why nothing appeared
  // gated before now.
  function renderAuthState() {
    var token = getToken();
    var app = document.getElementById("app");
    var gate = document.getElementById("login-gate");
    if (token) {
      if (app) app.classList.remove("hidden");
      if (gate) gate.classList.add("hidden");
    } else {
      if (app) app.classList.add("hidden");
      if (gate) gate.classList.remove("hidden");
    }
  }

  window.safelyAuth = {
    getToken: getToken,
    clearToken: clearToken,
    authHeader: function () {
      var token = getToken();
      return token ? { Authorization: "Bearer " + token } : {};
    },
    logout: function () {
      clearToken();
      window.location.href = "/";
    },
  };

  captureSessionFromUrl();
  renderAuthState();

  var logoutBtn = document.getElementById("btnLogout");
  if (logoutBtn) {
    logoutBtn.addEventListener("click", function (e) {
      e.preventDefault();
      window.safelyAuth.logout();
    });
  }
})();

// ============================================================
// The one interaction genuinely impossible in pure CSS: substring
// matching against two fields as the user types. Everything else in
// this dashboard runs on radios, :has(), :target and <details> alone.
// ============================================================
document.addEventListener("DOMContentLoaded", function () {
  var searchBox = document.getElementById("search-box");
  if (searchBox) {
    searchBox.addEventListener("input", function (e) {
      var q = e.target.value.trim().toLowerCase();
      document.querySelectorAll("#history-rows tr").forEach(function (tr) {
        var haystack = tr.getAttribute("data-search") || "";
        tr.setAttribute(
          "data-search-hidden",
          q && haystack.indexOf(q) === -1 ? "true" : "false",
        );
      });
    });
  }
});
