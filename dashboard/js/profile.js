function updateSidebarUserName(name) {
  var el = document.getElementById("sidebar-user-name");
  if (el) el.textContent = name || "User";
}

var currentAvatarObjectUrl = null;

async function updateAvatar(hasAvatar) {
  var targets = [
    { img: "sidebar-avatar-img", placeholder: "sidebar-avatar-placeholder" },
    {
      img: "settings-avatar-img",
      placeholder: "settings-avatar-placeholder",
    },
  ];

  if (!hasAvatar) {
    targets.forEach(function (t) {
      var img = document.getElementById(t.img);
      var placeholder = document.getElementById(t.placeholder);
      if (img) img.classList.add("hidden");
      if (placeholder) placeholder.classList.remove("hidden");
    });
    return;
  }

  try {
    var res = await fetch(API_BASE + "/me/avatar", {
      headers: window.safelyAuth.authHeader(),
    });
    if (res.status === 401) {
      window.safelyAuth.logout();
      return;
    }
    if (!res.ok) throw new Error("Failed to load avatar");

    var blob = await res.blob();

    if (currentAvatarObjectUrl) URL.revokeObjectURL(currentAvatarObjectUrl);
    currentAvatarObjectUrl = URL.createObjectURL(blob);

    targets.forEach(function (t) {
      var img = document.getElementById(t.img);
      var placeholder = document.getElementById(t.placeholder);
      if (img) {
        img.src = currentAvatarObjectUrl;
        img.classList.remove("hidden");
      }
      if (placeholder) placeholder.classList.add("hidden");
    });
  } catch (e) {
    console.error("Safely: failed to load avatar image", e);
  }
}

async function uploadAvatar(file) {
  var errorEl = document.getElementById("settings-avatar-error");
  var label = document.getElementById("settings-avatar-label");
  if (errorEl) errorEl.classList.add("hidden");

  var validTypes = ["image/png", "image/jpeg", "image/webp"];
  if (!validTypes.includes(file.type)) {
    if (errorEl) {
      errorEl.textContent = "Please choose a PNG, JPEG, or WEBP image.";
      errorEl.classList.remove("hidden");
    }
    return;
  }
  if (file.size > 2 * 1024 * 1024) {
    if (errorEl) {
      errorEl.textContent = "Image must be 2MB or smaller.";
      errorEl.classList.remove("hidden");
    }
    return;
  }

  var originalText = label ? label.textContent : "";
  if (label) label.textContent = "Uploading...";

  try {
    var formData = new FormData();
    formData.append("avatar", file);

    var res = await fetch(API_BASE + "/me/avatar", {
      method: "POST",
      headers: window.safelyAuth.authHeader(),
      body: formData,
    });

    if (res.status === 401) {
      window.safelyAuth.logout();
      return;
    }
    if (!res.ok) {
      var errBody = await res.text();
      throw new Error(errBody || "Upload failed");
    }

    await res.json();
    updateAvatar(true);
  } catch (e) {
    console.error("Safely: avatar upload failed", e);
    if (errorEl) {
      errorEl.textContent = "Could not upload photo. Please try again.";
      errorEl.classList.remove("hidden");
    }
  } finally {
    if (label) label.textContent = originalText;
  }
}
