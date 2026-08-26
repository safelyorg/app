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
  // Reads a ?error=... left in the URL by a failed auth attempt (e.g.
  // clicking an already-used magic link) and shows a small, honest
  // message about it - without this, someone who's still logged in
  // from an earlier successful click just sees the real dashboard with
  // no explanation for the "expired_link" text that briefly flashed in
  // the URL bar, which reads as confusing/contradictory even though
  // both things are correct and unrelated to each other.
  function checkForAuthErrorInUrl() {
    var params = new URLSearchParams(window.location.search);
    var error = params.get("error");
    if (!error) return;

    var messages = {
      expired_link:
        "That sign-in link had already been used or expired. You're still signed in from before, so nothing else to do here.",
      server_error:
        "Something went wrong on our end during that last sign-in attempt. If you're seeing this and aren't signed in, please try again.",
      google_denied: "Google sign-in was cancelled.",
      google_exchange_failed: "Couldn't complete Google sign-in. Please try again.",
      state_mismatch: "That sign-in attempt couldn't be verified. Please try again.",
      google_email_mismatch:
        "That Google account's email doesn't match your account's email, so it wasn't connected.",
      google_already_linked:
        "That Google account is already connected to a different Safely account.",
      session_expired: "Your session had expired. Please sign in again.",
    };

    showAuthToast(messages[error] || "Something didn't go as expected. Please try again.");

    // Clean the URL immediately after reading it, same as the session
    // hash already does - otherwise refreshing the page would keep
    // showing this same message on every reload.
    history.replaceState(null, "", window.location.pathname);
  }
  function showAuthToast(message) {
    var toast = document.createElement("div");
    toast.textContent = message;
    toast.style.cssText =
      "position:fixed;top:16px;left:50%;transform:translateX(-50%);" +
      "background:#1b1b20;color:#f2f1ed;border:1px solid rgba(255,255,255,0.12);" +
      "padding:12px 18px;border-radius:12px;font-size:13px;font-weight:500;" +
      "max-width:90vw;text-align:center;z-index:9999;" +
      "box-shadow:0 12px 32px -8px rgba(0,0,0,0.5);" +
      "font-family:Inter,-apple-system,sans-serif;";
    document.body.appendChild(toast);
    setTimeout(function () {
      toast.style.transition = "opacity 0.3s ease";
      toast.style.opacity = "0";
      setTimeout(function () {
        toast.remove();
      }, 300);
    }, 5000);
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
  // Attaches the real session token to every HTMX request automatically.
  document.body.addEventListener("htmx:configRequest", function (event) {
    var token = getToken();
    if (token) {
      event.detail.headers["Authorization"] = "Bearer " + token;
    }
  });

  captureSessionFromUrl();
  renderAuthState();
  checkForAuthErrorInUrl();
  var logoutBtn = document.getElementById("btnLogout");
  if (logoutBtn) {
    logoutBtn.addEventListener("click", function (e) {
      e.preventDefault();
      window.safelyAuth.logout();
    });
  }
})();
