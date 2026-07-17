var settingsLoaded = false;

async function loadSettingsData() {
  var loading = document.getElementById("settings-loading");
  var body = document.getElementById("settings-body");
  if (!loading || !body) return;

  try {
    var res = await fetch(API_BASE + "/me", {
      headers: window.safelyAuth.authHeader(),
    });

    if (res.status === 401) {
      window.safelyAuth.logout();
      return;
    }
    if (!res.ok) {
      loading.textContent = "Could not load account settings.";
      return;
    }

    var data = await res.json();

    document.getElementById("settings-email").textContent = data.email || "Unknown";
    document.getElementById("settings-name").textContent = data.name || "User";
    updateSidebarUserName(data.name);
    updateAvatar(data.has_avatar);
    document.getElementById("settings-signin-method").textContent =
      data.signed_in_with === "google" ? "Google" : "Email magic link";
    setGoogleButtonState(data.google_linked);
    document.getElementById("settings-created").textContent = formatDate(data.created_at);
    document.getElementById("settings-last-login").textContent = data.last_login_at
      ? formatDate(data.last_login_at)
      : "Unknown";

    loading.classList.add("hidden");
    body.classList.remove("hidden");
    settingsLoaded = true;
  } catch (e) {
    console.error("Safely: failed to load settings", e);
    loading.textContent = "Could not load account settings.";
  }
}

function toggleProfileEdit(showEdit) {
  var editBtn = document.getElementById("profile-edit-btn");
  var actions = document.getElementById("profile-edit-actions");
  var nameDisplay = document.getElementById("settings-name");
  var nameInput = document.getElementById("settings-name-input");
  var avatarOverlay = document.getElementById("avatar-edit-overlay");
  var errorEl = document.getElementById("settings-name-error");
  if (!editBtn || !actions || !nameDisplay || !nameInput) return;

  if (showEdit) {
    nameInput.value = nameDisplay.textContent === "User" ? "" : nameDisplay.textContent;
    editBtn.classList.add("hidden");
    actions.classList.remove("hidden");
    actions.classList.add("flex");
    nameDisplay.classList.add("hidden");
    nameInput.classList.remove("hidden");
    if (avatarOverlay) {
      avatarOverlay.classList.remove("hidden");
      avatarOverlay.classList.add("flex");
    }
    if (errorEl) errorEl.classList.add("hidden");
    nameInput.focus();
  } else {
    editBtn.classList.remove("hidden");
    actions.classList.add("hidden");
    actions.classList.remove("flex");
    nameDisplay.classList.remove("hidden");
    nameInput.classList.add("hidden");
    if (avatarOverlay) {
      avatarOverlay.classList.add("hidden");
      avatarOverlay.classList.remove("flex");
    }
  }
}

async function saveProfileEdit() {
  var input = document.getElementById("settings-name-input");
  var errorEl = document.getElementById("settings-name-error");
  var saveBtn = document.getElementById("profile-save-btn");
  var newName = input.value.trim();
  var currentName = document.getElementById("settings-name").textContent;

  errorEl.classList.add("hidden");

  if (!newName) {
    errorEl.textContent = "Name cannot be empty.";
    errorEl.classList.remove("hidden");
    return;
  }

  if (newName === currentName) {
    toggleProfileEdit(false);
    return;
  }

  var originalText = saveBtn.textContent;
  saveBtn.disabled = true;
  saveBtn.textContent = "Saving...";

  try {
    var res = await fetch(API_BASE + "/me", {
      method: "PATCH",
      headers: Object.assign(
        { "Content-Type": "application/json" },
        window.safelyAuth.authHeader(),
      ),
      body: JSON.stringify({ name: newName }),
    });

    if (res.status === 401) {
      window.safelyAuth.logout();
      return;
    }
    if (!res.ok) {
      var errBody = await res.text();
      throw new Error(errBody || "Request failed");
    }

    document.getElementById("settings-name").textContent = newName;
    updateSidebarUserName(newName);
    toggleProfileEdit(false);
  } catch (e) {
    errorEl.textContent = "Could not save. Please try again.";
    errorEl.classList.remove("hidden");
  } finally {
    saveBtn.disabled = false;
    saveBtn.textContent = originalText;
  }
}

function setGoogleButtonState(connected) {
  var btn = document.getElementById("google-connect-btn");
  if (!btn) return;
  btn.dataset.connected = connected ? "true" : "false";
  btn.disabled = false;
  if (connected) {
    btn.textContent = "Connected";
    btn.classList.remove("hover:bg-surface3");
  } else {
    btn.textContent = "Connect";
    btn.classList.add("hover:bg-surface3");
    btn.classList.remove("border-coral", "text-coral");
  }
}

function wireGoogleButtonHover() {
  var btn = document.getElementById("google-connect-btn");
  if (!btn) return;
  btn.addEventListener("mouseenter", function () {
    if (btn.dataset.connected === "true") {
      btn.textContent = "Disconnect";
      btn.classList.add("border-coral", "text-coral");
    }
  });
  btn.addEventListener("mouseleave", function () {
    if (btn.dataset.connected === "true") {
      btn.textContent = "Connected";
      btn.classList.remove("border-coral", "text-coral");
    }
  });
}

async function handleGoogleButtonClick() {
  var btn = document.getElementById("google-connect-btn");
  if (!btn) return;

  if (btn.dataset.connected === "true") {
    btn.disabled = true;
    try {
      var res = await fetch(API_BASE + "/me/google/disconnect", {
        method: "POST",
        headers: window.safelyAuth.authHeader(),
      });
      if (res.status === 401) {
        window.safelyAuth.logout();
        return;
      }
      if (!res.ok) throw new Error("Failed to disconnect");
      setGoogleButtonState(false);

      var signinMethodEl = document.getElementById("settings-signin-method");
      if (signinMethodEl) signinMethodEl.textContent = "Email magic link";

      var statusEl = document.getElementById("google-status-message");
      if (statusEl) {
        statusEl.textContent =
          "Google disconnected. You can sign in using your email magic link.";
        statusEl.classList.remove("hidden");
      }
    } catch (e) {
      alert("Could not disconnect Google. Please try again.");
      btn.disabled = false;
    }
  } else {
    var token = window.safelyAuth.getToken();
    window.location.href =
      API_BASE + "/auth/google/connect?session=" + encodeURIComponent(token);
  }
}

function checkGoogleConnectResult() {
  var params = new URLSearchParams(window.location.search);
  var error = params.get("error");
  var connected = params.get("google_connected");

  if (error === "google_already_linked") {
    alert(
      "That Google account is already connected to a different Safely account.",
    );
  } else if (error === "google_email_mismatch") {
    alert(
      "That Google account uses a different email address than your Safely account. " +
        "Please connect a Google account that uses the same email address.",
    );
  } else if (error === "session_expired") {
    alert("Your session expired - please log in again and retry connecting Google.");
  } else if (connected === "1") {
    // Nothing to alert here - the Settings fetch that already ran (or
    // will run) picks up the new state naturally via google_linked.
  }

  if (error || connected) {
    var url = new URL(window.location.href);
    url.searchParams.delete("error");
    url.searchParams.delete("google_connected");
    history.replaceState(null, "", url.pathname + url.hash);
  }
}
