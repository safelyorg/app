document.addEventListener("DOMContentLoaded", function () {
  checkGoogleConnectResult();

  if (window.safelyAuth && window.safelyAuth.getToken()) {
    loadDashboardData();
  }

  var closeBtn = document.getElementById("detail-close");
  if (closeBtn) {
    closeBtn.addEventListener("click", closeDetailPanel);
  }

  var navHistory = document.getElementById("view-history");
  var navReports = document.getElementById("view-reports");
  var navSettings = document.getElementById("view-settings");
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
      navSettings.addEventListener("change", function () {
          if (!settingsLoaded) loadSettingsData();
      });
      if (navSettings.checked && !settingsLoaded) {
          loadSettingsData();
      }
  }
  var mobileToggle = document.getElementById("mobile-nav-toggle");
  if (mobileToggle) {
      [navHistory, navReports, navSettings].forEach(function (input) {
          if (input) {
              input.addEventListener("change", function () {
                  mobileToggle.checked = false;
              });
          }
      });
  }

  window.addEventListener("pageshow", function () {
    var settingsRadio = document.getElementById("view-settings");
    if (settingsRadio && settingsRadio.checked && !settingsLoaded) {
      loadSettingsData();
    }
  });

  var settingsLink = document.getElementById("account-settings-link");
  if (settingsLink) {
    settingsLink.addEventListener("click", function () {
      var menu = document.getElementById("account-menu");
      if (menu) menu.open = false;
    });
  }

  var profileEditBtn = document.getElementById("profile-edit-btn");
  if (profileEditBtn) {
    profileEditBtn.addEventListener("click", function () {
      toggleProfileEdit(true);
    });
  }
  var profileCancelBtn = document.getElementById("profile-cancel-btn");
  if (profileCancelBtn) {
    profileCancelBtn.addEventListener("click", function () {
      toggleProfileEdit(false);
    });
  }
  var profileSaveBtn = document.getElementById("profile-save-btn");
  if (profileSaveBtn) {
    profileSaveBtn.addEventListener("click", saveProfileEdit);
  }
  var nameInput = document.getElementById("settings-name-input");
  if (nameInput) {
    nameInput.addEventListener("keydown", function (e) {
      if (e.key === "Enter") saveProfileEdit();
      if (e.key === "Escape") toggleProfileEdit(false);
    });
  }

  var deleteBtn = document.getElementById("delete-account-btn");
  var deleteConfirmBox = document.getElementById("delete-account-confirm");
  var deleteConfirmEmailEl = document.getElementById("delete-confirm-email");
  var deleteConfirmInput = document.getElementById("delete-confirm-input");
  var deleteConfirmBtn = document.getElementById("delete-confirm-btn");
  var deleteCancelBtn = document.getElementById("delete-cancel-btn");
  var deleteConfirmError = document.getElementById("delete-confirm-error");

  if (deleteBtn && deleteConfirmBox) {
    deleteBtn.addEventListener("click", function () {
      var accountEmail = document.getElementById("settings-email")
        ? document.getElementById("settings-email").textContent
        : "";
      if (deleteConfirmEmailEl) deleteConfirmEmailEl.textContent = accountEmail;
      if (deleteConfirmInput) deleteConfirmInput.value = "";
      if (deleteConfirmBtn) deleteConfirmBtn.disabled = true;
      if (deleteConfirmError) deleteConfirmError.classList.add("hidden");
      deleteConfirmBox.classList.remove("hidden");
      if (deleteConfirmInput) deleteConfirmInput.focus();
    });
  }

  if (deleteCancelBtn && deleteConfirmBox) {
    deleteCancelBtn.addEventListener("click", function () {
      deleteConfirmBox.classList.add("hidden");
    });
  }

  if (deleteConfirmInput && deleteConfirmBtn) {
    deleteConfirmInput.addEventListener("input", function () {
      var accountEmail = document.getElementById("settings-email")
        ? document.getElementById("settings-email").textContent.trim().toLowerCase()
        : "";
      var typed = deleteConfirmInput.value.trim().toLowerCase();
      deleteConfirmBtn.disabled = !(typed && typed === accountEmail);
    });
  }

  if (deleteConfirmBtn) {
    deleteConfirmBtn.addEventListener("click", async function () {
      deleteConfirmBtn.disabled = true;
      var originalText = deleteConfirmBtn.textContent;
      deleteConfirmBtn.textContent = "Deleting...";

      try {
        var res = await fetch(API_BASE + "/me", {
          method: "DELETE",
          headers: window.safelyAuth.authHeader(),
        });

        if (res.status === 401) {
          window.safelyAuth.logout();
          return;
        }
        if (!res.ok) throw new Error("Failed to delete account");

        window.safelyAuth.clearToken();
        window.location.href = "/?account_deleted=1";
      } catch (e) {
        if (deleteConfirmError) {
          deleteConfirmError.textContent =
            "Could not delete your account. Please try again.";
          deleteConfirmError.classList.remove("hidden");
        }
        deleteConfirmBtn.disabled = false;
        deleteConfirmBtn.textContent = originalText;
      }
    });
  }
  var googleConnectBtn = document.getElementById("google-connect-btn");
  if (googleConnectBtn) {
    wireGoogleButtonHover();
    googleConnectBtn.addEventListener("click", handleGoogleButtonClick);
  }

  // ============================================================
  // Plan & Billing - inline expanding section
  // ============================================================
  function showBillingToast(message) {
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
    }, 3000);
  }

  var planBillingToggle = document.getElementById("plan-billing-toggle");
  var planBillingExpanded = document.getElementById("plan-billing-expanded");
  var planBillingChevron = document.getElementById("plan-billing-chevron");
  var continueBtn = document.getElementById("plan-continue-btn");
  var cancelSubBtn = document.getElementById("cancel-subscription-btn");
  var cancelSubConfirm = document.getElementById("cancel-sub-confirm");
  var currentPlanBadge = document.getElementById("current-plan-badge");
  var selectedProductId = null;
  var selectedPlanName = null;
  var realSubscriptionStatus = null;
  var realSubscriptionPlan = null;

  function togglePlanSection(show) {
      if (!planBillingExpanded) return;
      planBillingExpanded.style.maxHeight = show ? "600px" : "0";
      if (planBillingChevron) {
        planBillingChevron.style.transform = show ? "rotate(180deg)" : "";
      }
  }

  if (planBillingToggle) {
    planBillingToggle.addEventListener("click", function () {
      var isCurrentlyOpen =
        planBillingExpanded.style.maxHeight &&
        planBillingExpanded.style.maxHeight !== "0px" &&
        planBillingExpanded.style.maxHeight !== "0";
      togglePlanSection(!isCurrentlyOpen);
    });
  }

  async function loadRealSubscriptionStatus() {
    try {
      var res = await fetch(API_BASE + "/billing/subscription-status", {
        headers: window.safelyAuth.authHeader(),
      });
      if (!res.ok) return;
      var data = await res.json();
      realSubscriptionStatus = data.status;
      realSubscriptionPlan = data.plan_name;

      var isActive = data.status === "active" || data.status === "trialing";
      var nameEl = document.getElementById("current-plan-name");
      var priceEl = document.getElementById("current-plan-price");

      if (isActive) {
        if (nameEl) nameEl.textContent = data.plan_name;
        if (currentPlanBadge) currentPlanBadge.classList.remove("hidden");
      } else {
        if (nameEl) nameEl.textContent = "No active plan";
        if (priceEl) priceEl.textContent = "Choose a plan below to get started";
        if (currentPlanBadge) currentPlanBadge.classList.add("hidden");
      }

      document.querySelectorAll(".plan-option").forEach(function (opt) {
        var check = opt.querySelector(".plan-check");
        if (!check) return;
        check.classList.toggle("hidden", !(isActive && opt.dataset.plan === data.plan_name));
      });
    } catch (e) {
      console.error("Safely: failed to load subscription status", e);
    }
  }
  loadRealSubscriptionStatus();

  // Detect a successful checkout redirect, show a friendly
  // confirmation, then clean Creem's appended parameters out of the
  // URL bar - purely cosmetic, since nothing here is read or trusted
  // for anything security-related.
  var checkoutParams = new URLSearchParams(window.location.search);
  if (checkoutParams.get("checkout") === "success") {
    showBillingToast("Welcome! Your subscription is now active.");
    history.replaceState(null, "", window.location.pathname);
    // The webhook may have already arrived by the time we're back
    // here - refresh the real status so "Active" shows up without
    // needing a manual reload.
    loadRealSubscriptionStatus();
  }

  document.querySelectorAll(".plan-option").forEach(function (opt) {
    opt.addEventListener("click", function () {
      document.querySelectorAll(".plan-option .plan-check").forEach(function (c) {
        c.classList.add("hidden");
      });
      var check = opt.querySelector(".plan-check");
      if (check) check.classList.remove("hidden");

      selectedProductId = opt.dataset.productId || null;
      selectedPlanName = opt.dataset.plan;
      if (continueBtn) continueBtn.disabled = false;
    });
  });

  if (continueBtn) {
    continueBtn.addEventListener("click", async function () {
      if (!selectedPlanName) return;

      if (!selectedProductId) {
        showBillingToast(selectedPlanName + " isn't available yet - check back soon.");
        return;
      }

      var isActive = realSubscriptionStatus === "active" || realSubscriptionStatus === "trialing";
      if (isActive && realSubscriptionPlan === selectedPlanName) {
        showBillingToast("You're already subscribed to " + selectedPlanName + ".");
        return;
      }

      var originalText = continueBtn.textContent;
      continueBtn.disabled = true;
      continueBtn.textContent = "Redirecting...";

      try {
        var res = await fetch(API_BASE + "/billing/checkout", {
          method: "POST",
          headers: Object.assign(
            { "Content-Type": "application/json" },
            window.safelyAuth.authHeader(),
          ),
          body: JSON.stringify({ product_id: selectedProductId }),
        });
        if (res.status === 401) {
          window.safelyAuth.logout();
          return;
        }
        if (!res.ok) throw new Error("Checkout creation failed");
        var data = await res.json();
        window.location.href = data.checkout_url;
      } catch (e) {
        console.error("Safely: failed to start checkout", e);
        showBillingToast("Couldn't start checkout. Please try again.");
        continueBtn.disabled = false;
        continueBtn.textContent = originalText;
      }
    });
  }

  function toggleCancelConfirm(show) {
      if (!cancelSubConfirm) return;
      cancelSubConfirm.style.maxHeight = show ? "200px" : "0";
  }

  if (cancelSubBtn) {
      cancelSubBtn.addEventListener("click", function () {
        var isActive = realSubscriptionStatus === "active" || realSubscriptionStatus === "trialing";
        if (!isActive) {
          showBillingToast("You don't have an active subscription to cancel.");
          return;
        }
        toggleCancelConfirm(true);
      });
  }

  var cancelSubConfirmNo = document.getElementById("cancel-sub-confirm-no");
  if (cancelSubConfirmNo) {
      cancelSubConfirmNo.addEventListener("click", function () {
        toggleCancelConfirm(false);
      });
  }

  var cancelSubConfirmYes = document.getElementById("cancel-sub-confirm-yes");
  if (cancelSubConfirmYes) {
      cancelSubConfirmYes.addEventListener("click", async function () {
        var originalText = cancelSubConfirmYes.textContent;
        cancelSubConfirmYes.disabled = true;
        cancelSubConfirmYes.textContent = "Canceling...";

        try {
          var res = await fetch(API_BASE + "/billing/cancel-subscription", {
            method: "POST",
            headers: window.safelyAuth.authHeader(),
          });
          if (res.status === 401) {
            window.safelyAuth.logout();
            return;
          }
          if (!res.ok) throw new Error("Cancel failed");

          // Update everything on screen directly, right now - the real database
          // row only updates later, once Creem's webhook actually arrives, so
          // waiting on a fresh fetch here would show stale, still-active data.
          realSubscriptionStatus = "canceled";
          realSubscriptionPlan = null;

          var nameEl = document.getElementById("current-plan-name");
          var priceEl = document.getElementById("current-plan-price");
          if (nameEl) nameEl.textContent = "No active plan";
          if (priceEl) priceEl.textContent = "Choose a plan below to get started";
          if (currentPlanBadge) currentPlanBadge.classList.add("hidden");
          document.querySelectorAll(".plan-option .plan-check").forEach(function (c) {
              c.classList.add("hidden");
          });

          toggleCancelConfirm(false);
          togglePlanSection(false);
          showBillingToast("Your subscription has been canceled.");
        } catch (e) {
          showBillingToast("Couldn't cancel your subscription. Please try again.");
          cancelSubConfirmYes.disabled = false;
          cancelSubConfirmYes.textContent = originalText;
        }
      });
  }

  var termsBtn = document.getElementById("terms-privacy-btn");
  var termsModal = document.getElementById("terms-privacy-modal");
  var termsClose = document.getElementById("terms-privacy-close");
  var termsBackdrop = document.getElementById("terms-privacy-backdrop");

  function toggleTermsModal(show) {
    if (!termsModal) return;
    termsModal.classList.toggle("hidden", !show);
    termsModal.classList.toggle("flex", show);
  }

  if (termsBtn) {
    termsBtn.addEventListener("click", function () {
      toggleTermsModal(true);
    });
  }
  if (termsClose) {
    termsClose.addEventListener("click", function () {
      toggleTermsModal(false);
    });
  }
  if (termsBackdrop) {
    termsBackdrop.addEventListener("click", function () {
      toggleTermsModal(false);
    });
  }
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") toggleTermsModal(false);
  });

  var avatarInput = document.getElementById("settings-avatar-input");
  if (avatarInput) {
    avatarInput.addEventListener("change", function (e) {
      var file = e.target.files && e.target.files[0];
      if (file) uploadAvatar(file);
    });
  }

  document.querySelectorAll(".detail-tab-btn").forEach(function (btn) {
    btn.addEventListener("click", function () {
      switchDetailTab(btn.dataset.detailTab);
    });
  });

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

  document.addEventListener("click", function (e) {
    var menu = document.getElementById("account-menu");
    if (menu && menu.open && !menu.contains(e.target)) {
      menu.open = false;
    }
  });
});
