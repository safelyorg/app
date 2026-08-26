function updateSidebarUserName(name: string | null | undefined): void {
  const el = document.getElementById("sidebar-user-name");
  if (el) el.textContent = name || "User";
}

let currentAvatarObjectUrl: string | null = null;

interface AvatarTarget {
  img: string;
  placeholder: string;
}

async function updateAvatar(hasAvatar: boolean): Promise<void> {
  const targets: AvatarTarget[] = [
    { img: "sidebar-avatar-img", placeholder: "sidebar-avatar-placeholder" },
    { img: "settings-avatar-img", placeholder: "settings-avatar-placeholder" },
  ];

  if (!hasAvatar) {
    targets.forEach((t) => {
      const img = document.getElementById(t.img);
      const placeholder = document.getElementById(t.placeholder);
      if (img) img.classList.add("hidden");
      if (placeholder) placeholder.classList.remove("hidden");
    });
    return;
  }

  try {
    const res = await fetch(API_BASE + "/me/avatar", {
      headers: (window as any).safelyAuth.authHeader(),
    });
    if (res.status === 401) {
      (window as any).safelyAuth.logout();
      return;
    }
    if (!res.ok) throw new Error("Failed to load avatar");

    const blob = await res.blob();
    if (currentAvatarObjectUrl) URL.revokeObjectURL(currentAvatarObjectUrl);
    currentAvatarObjectUrl = URL.createObjectURL(blob);

    targets.forEach((t) => {
      const img = document.getElementById(t.img) as HTMLImageElement | null;
      const placeholder = document.getElementById(t.placeholder);
      if (img) {
        img.src = currentAvatarObjectUrl as string;
        img.classList.remove("hidden");
      }
      if (placeholder) placeholder.classList.add("hidden");
    });
  } catch (e) {
    console.error("Safely: failed to load avatar image", e);
  }
}

async function uploadAvatar(file: File): Promise<void> {
  const errorEl = document.getElementById("settings-avatar-error");
  const label = document.getElementById("settings-avatar-label");
  if (errorEl) errorEl.classList.add("hidden");

  const validTypes = ["image/png", "image/jpeg", "image/webp"];
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

  const originalText = label ? label.textContent : "";
  if (label) label.textContent = "Uploading...";

  try {
    const formData = new FormData();
    formData.append("avatar", file);

    const res = await fetch(API_BASE + "/me/avatar", {
      method: "POST",
      headers: (window as any).safelyAuth.authHeader(),
      body: formData,
    });

    if (res.status === 401) {
      (window as any).safelyAuth.logout();
      return;
    }
    if (!res.ok) {
      const errBody = await res.text();
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
