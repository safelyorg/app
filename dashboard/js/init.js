"use strict";
document.addEventListener("DOMContentLoaded", async () => {
    checkGoogleConnectResult();
    if (window.safelyAuth && window.safelyAuth.getToken()) {
        loadDashboardData();
    }
    // Event delegation for history row clicks - works correctly however
    // the rows were inserted, since the listener lives on the parent.
    const historyRowsBody = document.getElementById("history-rows");
    if (historyRowsBody) {
        historyRowsBody.addEventListener("click", (e) => {
            const target = e.target;
            const row = target.closest(".history-row");
            if (row)
                openDetail(row.dataset.id);
        });
    }
    // Updates the "Checked" stat count right after HTMX finishes
    // inserting the real rows - the rows don't exist yet until then.
    document.body.addEventListener("htmx:afterSwap", (event) => {
        if (event.target.id === "history-rows" || event.target.id === "report-rows") {
            renderStats();
        }
    });
    const closeBtn = document.getElementById("detail-close");
    if (closeBtn) {
        closeBtn.addEventListener("click", closeDetailPanel);
    }
    const navHistory = document.getElementById("view-history");
    const navReports = document.getElementById("view-reports");
    const navSettings = document.getElementById("view-settings");
    if (navHistory) {
        navHistory.addEventListener("change", closeDetailPanel);
        navHistory.addEventListener("click", closeDetailPanel);
    }
    if (navReports) {
        navReports.addEventListener("change", closeDetailPanel);
        navReports.addEventListener("click", closeDetailPanel);
    }
    if (navSettings) {
        navSettings.addEventListener("change", closeDetailPanel);
        navSettings.addEventListener("click", closeDetailPanel);
        navSettings.addEventListener("change", () => {
            if (!settingsLoaded)
                loadSettingsData();
        });
        if (navSettings.checked && !settingsLoaded) {
            loadSettingsData();
        }
    }
    const mobileToggle = document.getElementById("mobile-nav-toggle");
    if (mobileToggle) {
        [navHistory, navReports, navSettings].forEach((input) => {
            if (input) {
                input.addEventListener("change", () => {
                    mobileToggle.checked = false;
                });
            }
        });
    }
    window.addEventListener("pageshow", () => {
        const settingsRadio = document.getElementById("view-settings");
        if (settingsRadio && settingsRadio.checked && !settingsLoaded) {
            loadSettingsData();
        }
    });
    const settingsLink = document.getElementById("account-settings-link");
    if (settingsLink) {
        settingsLink.addEventListener("click", () => {
            const menu = document.getElementById("account-menu");
            if (menu)
                menu.open = false;
        });
    }
    const profileEditBtn = document.getElementById("profile-edit-btn");
    if (profileEditBtn) {
        profileEditBtn.addEventListener("click", () => toggleProfileEdit(true));
    }
    const profileCancelBtn = document.getElementById("profile-cancel-btn");
    if (profileCancelBtn) {
        profileCancelBtn.addEventListener("click", () => toggleProfileEdit(false));
    }
    const profileSaveBtn = document.getElementById("profile-save-btn");
    if (profileSaveBtn) {
        profileSaveBtn.addEventListener("click", saveProfileEdit);
    }
    const nameInput = document.getElementById("settings-name-input");
    if (nameInput) {
        nameInput.addEventListener("keydown", (e) => {
            if (e.key === "Enter")
                saveProfileEdit();
            if (e.key === "Escape")
                toggleProfileEdit(false);
        });
    }
    // ============================================================
    // Delete account
    // ============================================================
    const deleteBtn = document.getElementById("delete-account-btn");
    const deleteConfirmBox = document.getElementById("delete-account-confirm");
    const deleteConfirmEmailEl = document.getElementById("delete-confirm-email");
    const deleteConfirmInput = document.getElementById("delete-confirm-input");
    const deleteConfirmBtn = document.getElementById("delete-confirm-btn");
    const deleteCancelBtn = document.getElementById("delete-cancel-btn");
    const deleteConfirmError = document.getElementById("delete-confirm-error");
    if (deleteBtn && deleteConfirmBox) {
        deleteBtn.addEventListener("click", () => {
            const emailEl = document.getElementById("settings-email");
            const accountEmail = emailEl ? emailEl.textContent : "";
            if (deleteConfirmEmailEl)
                deleteConfirmEmailEl.textContent = accountEmail;
            if (deleteConfirmInput)
                deleteConfirmInput.value = "";
            if (deleteConfirmBtn)
                deleteConfirmBtn.disabled = true;
            if (deleteConfirmError)
                deleteConfirmError.classList.add("hidden");
            deleteConfirmBox.classList.remove("hidden");
            if (deleteConfirmInput)
                deleteConfirmInput.focus();
        });
    }
    if (deleteCancelBtn && deleteConfirmBox) {
        deleteCancelBtn.addEventListener("click", () => {
            deleteConfirmBox.classList.add("hidden");
        });
    }
    if (deleteConfirmInput && deleteConfirmBtn) {
        deleteConfirmInput.addEventListener("input", () => {
            const emailEl = document.getElementById("settings-email");
            const accountEmail = emailEl ? (emailEl.textContent || "").trim().toLowerCase() : "";
            const typed = deleteConfirmInput.value.trim().toLowerCase();
            deleteConfirmBtn.disabled = !(typed && typed === accountEmail);
        });
    }
    if (deleteConfirmBtn) {
        deleteConfirmBtn.addEventListener("click", async () => {
            deleteConfirmBtn.disabled = true;
            const originalText = deleteConfirmBtn.textContent;
            deleteConfirmBtn.textContent = "Deleting...";
            try {
                const res = await fetch(API_BASE + "/me", {
                    method: "DELETE",
                    headers: window.safelyAuth.authHeader(),
                });
                if (res.status === 401) {
                    window.safelyAuth.logout();
                    return;
                }
                if (!res.ok)
                    throw new Error("Failed to delete account");
                window.safelyAuth.clearToken();
                window.location.href = "/?account_deleted=1";
            }
            catch (e) {
                if (deleteConfirmError) {
                    deleteConfirmError.textContent = "Could not delete your account. Please try again.";
                    deleteConfirmError.classList.remove("hidden");
                }
                deleteConfirmBtn.disabled = false;
                deleteConfirmBtn.textContent = originalText;
            }
        });
    }
    const googleConnectBtn = document.getElementById("google-connect-btn");
    if (googleConnectBtn) {
        wireGoogleButtonHover();
        googleConnectBtn.addEventListener("click", handleGoogleButtonClick);
    }
    async function loadProductIds() {
        try {
            const res = await fetch(API_BASE + "/billing/product-ids");
            if (!res.ok)
                return;
            const ids = await res.json();
            document.querySelectorAll(".plan-option").forEach((opt) => {
                const realId = ids[opt.dataset.plan];
                if (realId) {
                    opt.dataset.productId = realId;
                }
            });
        }
        catch (e) {
            console.error("Safely: failed to load product IDs", e);
        }
    }
    await loadProductIds();
    // ============================================================
    // Plan & Billing - inline expanding section
    // ============================================================
    const planBillingToggle = document.getElementById("plan-billing-toggle");
    const planBillingExpanded = document.getElementById("plan-billing-expanded");
    const planBillingChevron = document.getElementById("plan-billing-chevron");
    const continueBtn = document.getElementById("plan-continue-btn");
    const cancelSubBtn = document.getElementById("cancel-subscription-btn");
    const cancelSubConfirm = document.getElementById("cancel-sub-confirm");
    const currentPlanBadge = document.getElementById("current-plan-badge");
    let selectedProductId = null;
    let selectedPlanName = null;
    let realSubscriptionStatus = null;
    let realSubscriptionPlan = null;
    function togglePlanSection(show) {
        if (!planBillingExpanded)
            return;
        planBillingExpanded.style.maxHeight = show ? "600px" : "0";
        if (planBillingChevron) {
            planBillingChevron.style.transform = show ? "rotate(180deg)" : "";
        }
    }
    if (planBillingToggle) {
        planBillingToggle.addEventListener("click", () => {
            const isCurrentlyOpen = planBillingExpanded.style.maxHeight &&
                planBillingExpanded.style.maxHeight !== "0px" &&
                planBillingExpanded.style.maxHeight !== "0";
            togglePlanSection(!isCurrentlyOpen);
        });
    }
    async function loadRealSubscriptionStatus() {
        try {
            const res = await fetch(API_BASE + "/billing/subscription-status", {
                headers: window.safelyAuth.authHeader(),
            });
            if (!res.ok)
                return;
            const data = await res.json();
            realSubscriptionStatus = data.status;
            realSubscriptionPlan = data.plan_name;
            const isActive = data.status === "active" || data.status === "trialing";
            const nameEl = document.getElementById("current-plan-name");
            const priceEl = document.getElementById("current-plan-price");
            if (isActive) {
                if (nameEl)
                    nameEl.textContent = data.plan_name;
                if (priceEl && data.current_period_end) {
                    const renewDate = new Date(data.current_period_end);
                    const formatted = renewDate.toLocaleDateString("en-US", {
                        month: "short",
                        day: "numeric",
                        year: "numeric",
                    });
                    const label = data.status === "trialing" ? "Trial ends" : "Renews at";
                    priceEl.textContent = label + " " + formatted;
                }
                if (currentPlanBadge)
                    currentPlanBadge.classList.remove("hidden");
            }
            else {
                if (nameEl)
                    nameEl.textContent = "No active plan";
                if (priceEl)
                    priceEl.textContent = "Choose a plan below to get started";
                if (currentPlanBadge)
                    currentPlanBadge.classList.add("hidden");
            }
            document.querySelectorAll(".plan-option").forEach((opt) => {
                const check = opt.querySelector(".plan-check");
                if (!check)
                    return;
                check.classList.toggle("hidden", !(isActive && opt.dataset.plan === data.plan_name));
            });
        }
        catch (e) {
            console.error("Safely: failed to load subscription status", e);
        }
    }
    loadRealSubscriptionStatus();
    const billingParams = new URLSearchParams(window.location.search);
    if (billingParams.get("manage_billing") === "1") {
        document.getElementById("view-settings").checked = true;
        togglePlanSection(true);
        history.replaceState(null, "", window.location.pathname);
    }
    // Detect a successful checkout redirect, show a friendly
    // confirmation, then clean Creem's appended parameters out of the
    // URL bar - purely cosmetic, nothing here is trusted for security.
    const checkoutParams = new URLSearchParams(window.location.search);
    if (checkoutParams.get("checkout") === "success") {
        history.replaceState(null, "", window.location.pathname);
        loadRealSubscriptionStatus().then(() => {
            if (realSubscriptionStatus === "trialing") {
                showToast("Welcome! Your 7-day free trial has started.");
            }
            else {
                showToast("Welcome! Your subscription is now active.");
            }
        });
    }
    document.querySelectorAll(".plan-option").forEach((opt) => {
        opt.addEventListener("click", () => {
            document.querySelectorAll(".plan-option .plan-check").forEach((c) => {
                c.classList.add("hidden");
            });
            const check = opt.querySelector(".plan-check");
            if (check)
                check.classList.remove("hidden");
            selectedProductId = opt.dataset.productId || null;
            selectedPlanName = opt.dataset.plan;
            if (continueBtn)
                continueBtn.disabled = false;
        });
    });
    if (continueBtn) {
        continueBtn.addEventListener("click", async () => {
            if (!selectedPlanName)
                return;
            if (!selectedProductId) {
                showToast(selectedPlanName + " isn't available yet - check back soon.");
                return;
            }
            const isActive = realSubscriptionStatus === "active" || realSubscriptionStatus === "trialing";
            if (isActive && realSubscriptionPlan === selectedPlanName) {
                showToast("You're already subscribed to " + selectedPlanName + ".");
                return;
            }
            const originalText = continueBtn.textContent;
            continueBtn.disabled = true;
            if (isActive) {
                continueBtn.textContent = "Updating...";
                try {
                    const res = await fetch(API_BASE + "/billing/change-plan", {
                        method: "POST",
                        headers: Object.assign({ "Content-Type": "application/json" }, window.safelyAuth.authHeader()),
                        body: JSON.stringify({ product_id: selectedProductId, plan_name: selectedPlanName }),
                    });
                    if (res.status === 401) {
                        window.safelyAuth.logout();
                        return;
                    }
                    if (res.status === 409) {
                        showToast("You can switch plans once your trial ends.");
                        return;
                    }
                    if (!res.ok)
                        throw new Error("Plan change failed");
                    const data = await res.json();
                    if (data.applied === "immediately") {
                        showToast("You've been upgraded to " + selectedPlanName + ".");
                    }
                    else {
                        showToast("You'll switch to " + selectedPlanName + " when your current period ends.");
                    }
                    await loadRealSubscriptionStatus();
                    togglePlanSection(false);
                }
                catch (e) {
                    showToast("Couldn't update your plan. Please try again.");
                }
                finally {
                    continueBtn.disabled = false;
                    continueBtn.textContent = originalText;
                }
                return;
            }
            continueBtn.textContent = "Redirecting...";
            try {
                const res = await fetch(API_BASE + "/billing/checkout", {
                    method: "POST",
                    headers: Object.assign({ "Content-Type": "application/json" }, window.safelyAuth.authHeader()),
                    body: JSON.stringify({ product_id: selectedProductId }),
                });
                if (res.status === 401) {
                    window.safelyAuth.logout();
                    return;
                }
                if (!res.ok)
                    throw new Error("Checkout creation failed");
                const data = await res.json();
                window.location.href = data.checkout_url;
            }
            catch (e) {
                console.error("Safely: failed to start checkout", e);
                showToast("Couldn't start checkout. Please try again.");
                continueBtn.disabled = false;
                continueBtn.textContent = originalText;
            }
        });
    }
    function toggleCancelConfirm(show) {
        if (!cancelSubConfirm)
            return;
        cancelSubConfirm.style.maxHeight = show ? "200px" : "0";
    }
    if (cancelSubBtn) {
        cancelSubBtn.addEventListener("click", () => {
            const isActive = realSubscriptionStatus === "active" || realSubscriptionStatus === "trialing";
            if (!isActive) {
                showToast("You don't have an active subscription to cancel.");
                return;
            }
            toggleCancelConfirm(true);
        });
    }
    const cancelSubConfirmNo = document.getElementById("cancel-sub-confirm-no");
    if (cancelSubConfirmNo) {
        cancelSubConfirmNo.addEventListener("click", () => toggleCancelConfirm(false));
    }
    const cancelSubConfirmYes = document.getElementById("cancel-sub-confirm-yes");
    if (cancelSubConfirmYes) {
        cancelSubConfirmYes.addEventListener("click", async () => {
            const originalText = cancelSubConfirmYes.textContent;
            cancelSubConfirmYes.disabled = true;
            cancelSubConfirmYes.textContent = "Canceling...";
            try {
                const res = await fetch(API_BASE + "/billing/cancel-subscription", {
                    method: "POST",
                    headers: window.safelyAuth.authHeader(),
                });
                if (res.status === 401) {
                    window.safelyAuth.logout();
                    return;
                }
                if (!res.ok)
                    throw new Error("Cancel failed");
                // Update the screen directly, right now - the real database
                // row only updates later, once Creem's webhook actually
                // arrives, so a fresh fetch here would show stale data.
                realSubscriptionStatus = "canceled";
                realSubscriptionPlan = null;
                const nameEl = document.getElementById("current-plan-name");
                const priceEl = document.getElementById("current-plan-price");
                if (nameEl)
                    nameEl.textContent = "No active plan";
                if (priceEl)
                    priceEl.textContent = "Choose a plan below to get started";
                if (currentPlanBadge)
                    currentPlanBadge.classList.add("hidden");
                document.querySelectorAll(".plan-option .plan-check").forEach((c) => {
                    c.classList.add("hidden");
                });
                toggleCancelConfirm(false);
                togglePlanSection(false);
                showToast("Your subscription has been canceled.");
            }
            catch (e) {
                showToast("Couldn't cancel your subscription. Please try again.");
                cancelSubConfirmYes.disabled = false;
                cancelSubConfirmYes.textContent = originalText;
            }
        });
    }
    // ============================================================
    // Terms & Privacy modal
    // ============================================================
    const termsBtn = document.getElementById("terms-privacy-btn");
    const termsModal = document.getElementById("terms-privacy-modal");
    const termsClose = document.getElementById("terms-privacy-close");
    const termsBackdrop = document.getElementById("terms-privacy-backdrop");
    function toggleTermsModal(show) {
        if (!termsModal)
            return;
        termsModal.classList.toggle("hidden", !show);
        termsModal.classList.toggle("flex", show);
    }
    if (termsBtn)
        termsBtn.addEventListener("click", () => toggleTermsModal(true));
    if (termsClose)
        termsClose.addEventListener("click", () => toggleTermsModal(false));
    if (termsBackdrop)
        termsBackdrop.addEventListener("click", () => toggleTermsModal(false));
    document.addEventListener("keydown", (e) => {
        if (e.key === "Escape")
            toggleTermsModal(false);
    });
    const avatarInput = document.getElementById("settings-avatar-input");
    if (avatarInput) {
        avatarInput.addEventListener("change", (e) => {
            const target = e.target;
            const file = target.files && target.files[0];
            if (file)
                uploadAvatar(file);
        });
    }
    document.querySelectorAll(".detail-tab-btn").forEach((btn) => {
        btn.addEventListener("click", () => {
            switchDetailTab(btn.dataset.detailTab);
        });
    });
    const searchBox = document.getElementById("search-box");
    if (searchBox) {
        searchBox.addEventListener("input", (e) => {
            const target = e.target;
            const q = target.value.trim().toLowerCase();
            document.querySelectorAll("#history-rows tr").forEach((tr) => {
                const haystack = tr.getAttribute("data-search") || "";
                tr.setAttribute("data-search-hidden", q && haystack.indexOf(q) === -1 ? "true" : "false");
            });
        });
    }
    document.addEventListener("click", (e) => {
        const menu = document.getElementById("account-menu");
        if (menu && menu.open && !menu.contains(e.target)) {
            menu.open = false;
        }
    });
});
