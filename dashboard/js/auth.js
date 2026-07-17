// ============================================================
// Session handling. Runs first, before anything else.
// ============================================================
(function () {
  "use strict";
  var STORAGE_KEY = "safely_session_token";

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
    logout: async function () {
      var token = getToken();
      if (token) {
        try {
          await fetch("/api/v1/auth/logout", {
            method: "POST",
            headers: { Authorization: "Bearer " + token },
          });
        } catch (e) {
          console.error("Safely: logout request failed", e);
        }
      }
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
