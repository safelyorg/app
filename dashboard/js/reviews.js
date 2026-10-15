"use strict";
// "Your review" card in Settings: the signed-in user writes one review
// with a photo. It shows "Going live soon" until it is approved
// (scripts/reviews.sh), then "Live on safely.sh".
const REVIEW_MIN_CHARS = 20;
const REVIEW_MAX_CHARS = 600;
const REVIEW_PHOTO_SIZE = 400; // photos are cropped to a 400x400 square
const REVIEW_MAX_RAW_PHOTO_BYTES = 15 * 1024 * 1024; // before shrinking
document.addEventListener("DOMContentLoaded", () => {
    const card = document.getElementById("review-card");
    if (!card)
        return;
    const statusEl = document.getElementById("review-status");
    const photoInput = document.getElementById("review-photo-input");
    const photoPreview = document.getElementById("review-photo-preview");
    const photoPlaceholder = document.getElementById("review-photo-placeholder");
    const starButtons = Array.from(document.querySelectorAll(".review-star"));
    const nameInput = document.getElementById("review-name");
    const roleInput = document.getElementById("review-role");
    const bodyInput = document.getElementById("review-body");
    const counterEl = document.getElementById("review-counter");
    const consentInput = document.getElementById("review-consent");
    const errorEl = document.getElementById("review-error");
    const successEl = document.getElementById("review-success");
    const submitBtn = document.getElementById("review-submit");
    let rating = 0;
    let hasSavedPhoto = false;
    let newPhoto = null;
    let previewUrl = null;
    let loaded = false;
    function authHeader() {
        return window.safelyAuth.authHeader();
    }
    function showError(message) {
        errorEl.textContent = message;
        errorEl.classList.remove("hidden");
        successEl.classList.add("hidden");
    }
    function clearMessages() {
        errorEl.classList.add("hidden");
        successEl.classList.add("hidden");
    }
    function setPreview(url) {
        if (previewUrl)
            URL.revokeObjectURL(previewUrl);
        previewUrl = url;
        if (url) {
            photoPreview.src = url;
            photoPreview.classList.remove("hidden");
            photoPlaceholder.classList.add("hidden");
        }
        else {
            photoPreview.removeAttribute("src");
            photoPreview.classList.add("hidden");
            photoPlaceholder.classList.remove("hidden");
        }
    }
    function renderStars() {
        starButtons.forEach((btn) => {
            const on = Number(btn.dataset.value) <= rating;
            btn.classList.toggle("text-amber", on);
            btn.classList.toggle("text-surface3", !on);
            btn.setAttribute("aria-pressed", on ? "true" : "false");
        });
    }
    function renderCounter() {
        const n = bodyInput.value.trim().length;
        counterEl.textContent = n + "/" + REVIEW_MAX_CHARS;
        counterEl.classList.toggle("text-coral", n > REVIEW_MAX_CHARS);
    }
    function renderStatus(status) {
        statusEl.className =
            "flex-shrink-0 px-2 py-0.5 rounded-full text-[10px] font-bold uppercase tracking-wide";
        if (status === "approved") {
            statusEl.classList.add("bg-mint/10", "text-mint");
            statusEl.textContent = t("dash.review.status_live", "Live on safely.sh");
        }
        else if (status === "pending") {
            statusEl.classList.add("bg-brand/10", "text-brand");
            statusEl.textContent = t("dash.review.status_soon", "Going live soon");
        }
        else if (status === "rejected") {
            statusEl.classList.add("bg-coral/10", "text-coral");
            statusEl.textContent = t("dash.review.status_rejected", "Not approved");
        }
        else {
            statusEl.classList.add("hidden");
            statusEl.textContent = "";
        }
        submitBtn.textContent = status
            ? t("dash.review.update", "Update review")
            : t("dash.review.submit", "Submit review");
    }
    // Crops the photo to a centered square and saves it as a 400x400 JPEG.
    // Redrawing the image also drops hidden photo data (location, camera).
    function shrinkPhoto(file) {
        return new Promise((resolve, reject) => {
            const url = URL.createObjectURL(file);
            const img = new Image();
            img.onload = () => {
                URL.revokeObjectURL(url);
                const side = Math.min(img.naturalWidth, img.naturalHeight);
                if (side < 1) {
                    reject(new Error("empty image"));
                    return;
                }
                const canvas = document.createElement("canvas");
                canvas.width = REVIEW_PHOTO_SIZE;
                canvas.height = REVIEW_PHOTO_SIZE;
                const ctx = canvas.getContext("2d");
                if (!ctx) {
                    reject(new Error("no canvas"));
                    return;
                }
                ctx.fillStyle = "#ffffff"; // transparent PNGs get a white background
                ctx.fillRect(0, 0, REVIEW_PHOTO_SIZE, REVIEW_PHOTO_SIZE);
                ctx.drawImage(img, (img.naturalWidth - side) / 2, (img.naturalHeight - side) / 2, side, side, 0, 0, REVIEW_PHOTO_SIZE, REVIEW_PHOTO_SIZE);
                canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error("could not save image"))), "image/jpeg", 0.85);
            };
            img.onerror = () => {
                URL.revokeObjectURL(url);
                reject(new Error("not an image"));
            };
            img.src = url;
        });
    }
    async function loadMyPhoto() {
        try {
            const res = await fetch(API_BASE + "/reviews/mine/photo", { headers: authHeader() });
            if (!res.ok)
                return;
            setPreview(URL.createObjectURL(await res.blob()));
        }
        catch (e) {
            console.error("Safely: failed to load review photo", e);
        }
    }
    function fill(review) {
        if (review) {
            nameInput.value = review.display_name;
            roleInput.value = review.role || "";
            bodyInput.value = review.body;
            rating = review.rating;
            hasSavedPhoto = review.has_photo;
            consentInput.checked = true;
            if (review.has_photo && !newPhoto)
                loadMyPhoto();
        }
        else if (!nameInput.value) {
            const accountName = document.getElementById("settings-name");
            nameInput.value = accountName ? (accountName.textContent || "").trim() : "";
        }
        renderStars();
        renderCounter();
        renderStatus(review ? review.status : null);
    }
    async function loadMyReview() {
        if (loaded)
            return;
        const auth = window.safelyAuth;
        if (!auth || !auth.getToken())
            return;
        loaded = true;
        try {
            const res = await fetch(API_BASE + "/reviews/mine", { headers: authHeader() });
            if (res.status === 401) {
                auth.logout();
                return;
            }
            if (!res.ok)
                throw new Error("load failed");
            const data = await res.json();
            fill(data.review || null);
        }
        catch (e) {
            loaded = false;
            console.error("Safely: failed to load your review", e);
        }
    }
    // Loaded when Settings opens (that's where the card is).
    const settingsRadio = document.getElementById("view-settings");
    if (settingsRadio) {
        settingsRadio.addEventListener("change", loadMyReview);
        if (settingsRadio.checked)
            loadMyReview();
    }
    window.addEventListener("pageshow", () => {
        if (settingsRadio && settingsRadio.checked)
            loadMyReview();
    });
    starButtons.forEach((btn) => {
        btn.addEventListener("click", () => {
            rating = Number(btn.dataset.value);
            renderStars();
            clearMessages();
        });
    });
    bodyInput.addEventListener("input", renderCounter);
    photoInput.addEventListener("change", async () => {
        clearMessages();
        const file = photoInput.files && photoInput.files[0];
        photoInput.value = ""; // lets the same file be picked again
        if (!file)
            return;
        if (!["image/jpeg", "image/png", "image/webp"].includes(file.type)) {
            showError(t("dash.review.err_photo_type", "Please use a JPG, PNG or WebP photo."));
            return;
        }
        if (file.size > REVIEW_MAX_RAW_PHOTO_BYTES) {
            showError(t("dash.review.err_photo_big", "This photo is too large. Please choose one under 15 MB."));
            return;
        }
        try {
            newPhoto = await shrinkPhoto(file);
            setPreview(URL.createObjectURL(newPhoto));
        }
        catch (e) {
            showError(t("dash.review.err_photo_read", "Could not read this photo. Please try another one."));
        }
    });
    submitBtn.addEventListener("click", async () => {
        clearMessages();
        const body = bodyInput.value.trim();
        if (!newPhoto && !hasSavedPhoto) {
            showError(t("dash.review.err_photo_needed", "Please add a photo of yourself."));
            return;
        }
        if (rating < 1) {
            showError(t("dash.review.err_rating", "Please choose 1 to 5 stars."));
            return;
        }
        if (!nameInput.value.trim()) {
            showError(t("dash.review.err_name", "Please enter your name."));
            return;
        }
        if (body.length < REVIEW_MIN_CHARS) {
            showError(t("dash.review.err_short", "Please write at least 20 characters."));
            return;
        }
        if (body.length > REVIEW_MAX_CHARS) {
            showError(t("dash.review.err_long", "Your review can be at most 600 characters."));
            return;
        }
        if (!consentInput.checked) {
            showError(t("dash.review.err_consent", "Please allow Safely to show your review, name and photo."));
            return;
        }
        const originalText = submitBtn.textContent;
        submitBtn.disabled = true;
        submitBtn.textContent = t("dash.review.saving", "Saving...");
        try {
            const res = await fetch(API_BASE + "/reviews", {
                method: "POST",
                headers: Object.assign({ "Content-Type": "application/json" }, authHeader()),
                body: JSON.stringify({
                    display_name: nameInput.value.trim(),
                    role: roleInput.value.trim() || null,
                    rating,
                    body,
                    consent: true,
                }),
            });
            if (res.status === 401) {
                window.safelyAuth.logout();
                return;
            }
            const data = await res.json().catch(() => ({}));
            if (!res.ok) {
                showError(data.error || t("dash.review.err_save", "Could not save your review. Please try again."));
                return;
            }
            if (newPhoto) {
                const photoRes = await fetch(API_BASE + "/reviews/mine/photo", {
                    method: "POST",
                    headers: Object.assign({ "Content-Type": "image/jpeg" }, authHeader()),
                    body: newPhoto,
                });
                if (!photoRes.ok) {
                    const photoData = await photoRes.json().catch(() => ({}));
                    showError(photoData.error ||
                        t("dash.review.err_photo_save", "Your review was saved, but the photo wasn't. Please try the photo again."));
                    fill(data);
                    return;
                }
                newPhoto = null;
                hasSavedPhoto = true;
            }
            fill(Object.assign({}, data, { has_photo: hasSavedPhoto }));
            successEl.textContent = t("dash.review.saved", "Thank you! Your review will be live on safely.sh soon.");
            successEl.classList.remove("hidden");
        }
        catch (e) {
            console.error("Safely: failed to save review", e);
            showError(t("dash.review.err_save", "Could not save your review. Please try again."));
        }
        finally {
            submitBtn.disabled = false;
            if (submitBtn.textContent === t("dash.review.saving", "Saving...")) {
                submitBtn.textContent = originalText;
            }
        }
    });
});
